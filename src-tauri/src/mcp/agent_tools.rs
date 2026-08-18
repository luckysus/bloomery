use crate::{
    agent::{
        runtime::{
            CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler,
            ToolInvocation, ToolRegistration,
        },
        tool_repair::ToolSpec,
    },
    mcp::{McpError, McpSupervisor},
    tools::{ToolDefinition, ToolRegistry, ToolSource},
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_MCP_TOOLS_FOR_AGENT_PROMPT: usize = 8;

pub trait McpToolCaller: Send + Sync {
    fn call(
        &self,
        tool_name: String,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> ToolFuture;
}

pub struct McpToolBinding {
    pub definition: ToolDefinition,
    pub caller: Arc<dyn McpToolCaller>,
}

pub struct McpToolExecutor {
    registrations: Vec<ToolRegistration>,
}

impl McpToolExecutor {
    pub async fn from_supervisors(
        supervisors: Vec<Arc<Mutex<McpSupervisor>>>,
    ) -> Result<Self, String> {
        Self::from_supervisors_for_query(supervisors, "").await
    }

    pub async fn from_supervisors_for_query(
        supervisors: Vec<Arc<Mutex<McpSupervisor>>>,
        query: &str,
    ) -> Result<Self, String> {
        let mut bindings = Vec::new();
        for supervisor in supervisors {
            let guard = supervisor.lock().await;
            let definitions = match guard.client() {
                Ok(client) => match client.tool_definitions().await {
                    Ok(definitions) => definitions,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            drop(guard);
            for definition in definitions {
                bindings.push(McpToolBinding {
                    definition,
                    caller: Arc::new(SupervisorCaller {
                        supervisor: supervisor.clone(),
                    }),
                });
            }
        }
        Self::from_bindings_for_query(bindings, query)
    }

    pub fn from_bindings_for_query(
        bindings: Vec<McpToolBinding>,
        query: &str,
    ) -> Result<Self, String> {
        let bindings = select_bindings_for_query(bindings, query);
        Self::from_bindings(bindings)
    }

    pub fn from_bindings(bindings: Vec<McpToolBinding>) -> Result<Self, String> {
        let mut registry = ToolRegistry::new();
        let mut registrations = Vec::with_capacity(bindings.len());
        for binding in bindings {
            if !matches!(binding.definition.source, ToolSource::Mcp { .. }) {
                return Err(format!(
                    "MCP tool executor accepts only MCP sources: {}",
                    binding.definition.id
                ));
            }
            registry
                .register(binding.definition.clone())
                .map_err(|error| error.to_string())?;
            let definition = binding.definition;
            let handler = ForwardingHandler {
                caller: binding.caller,
                tool_name: definition.name.clone(),
            };
            registrations.push(ToolRegistration::new(
                ToolSpec {
                    id: definition.id.to_string(),
                    name: definition.name,
                    input_schema: definition.input_schema,
                    risk: definition.risk,
                },
                definition.read_only,
                Arc::new(handler),
            ));
        }
        Ok(Self { registrations })
    }
}

fn select_bindings_for_query(bindings: Vec<McpToolBinding>, query: &str) -> Vec<McpToolBinding> {
    if bindings.len() <= MAX_MCP_TOOLS_FOR_AGENT_PROMPT {
        return bindings;
    }
    let terms = query_terms(query);
    let mut scored = bindings
        .into_iter()
        .enumerate()
        .map(|(index, binding)| {
            let score = score_binding(&binding, &terms, query);
            (index, score, binding)
        })
        .collect::<Vec<_>>();
    if scored.iter().any(|(_, score, _)| *score > 0) {
        scored.retain(|(_, score, _)| *score > 0);
        scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    }
    scored
        .into_iter()
        .take(MAX_MCP_TOOLS_FOR_AGENT_PROMPT)
        .map(|(_, _, binding)| binding)
        .collect()
}

fn score_binding(binding: &McpToolBinding, terms: &[String], query: &str) -> usize {
    let text = format!(
        "{} {} {}",
        binding.definition.id, binding.definition.name, binding.definition.description
    )
    .to_lowercase();
    let mut score = terms.iter().filter(|term| text.contains(*term)).count();
    if !binding.definition.name.trim().is_empty()
        && query
            .to_lowercase()
            .contains(&binding.definition.name.to_lowercase())
    {
        score += 2;
    }
    score
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_string)
        .collect()
}

impl ToolExecutor for McpToolExecutor {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        let Some(registration) = self.registrations.iter().find(|registration| {
            registration.spec.id == invocation.tool_id
                && registration.spec.name == invocation.tool_name
        }) else {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "MCP tool is not registered",
                ))
            });
        };
        registration
            .handler
            .execute(invocation.arguments, cancellation)
    }
}

struct ForwardingHandler {
    caller: Arc<dyn McpToolCaller>,
    tool_name: String,
}

struct SupervisorCaller {
    supervisor: Arc<Mutex<McpSupervisor>>,
}

impl McpToolCaller for SupervisorCaller {
    fn call(
        &self,
        tool_name: String,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> ToolFuture {
        let supervisor = self.supervisor.clone();
        Box::pin(async move {
            let guard = supervisor.lock().await;
            let client = guard.client().map_err(mcp_tool_error)?;
            let result = client
                .call_tool(&tool_name, arguments, &cancellation)
                .await
                .map_err(mcp_tool_error)?;
            if result.is_error {
                return Err(ToolExecutionError::new(
                    "mcp_tool_error",
                    serde_json::to_string(&result)
                        .unwrap_or_else(|_| "MCP server returned a tool error".to_string()),
                ));
            }
            serde_json::to_value(result).map_err(|error| {
                ToolExecutionError::new("mcp_result_serialization", error.to_string())
            })
        })
    }
}

fn mcp_tool_error(error: McpError) -> ToolExecutionError {
    if matches!(error, McpError::Cancelled) {
        return ToolExecutionError::cancelled();
    }
    let code = match error {
        McpError::Timeout => "mcp_timeout",
        McpError::Protocol { .. } => "mcp_protocol_error",
        McpError::Initialization(_) => "mcp_initialization_error",
        McpError::Transport(_) => "mcp_transport_error",
        McpError::InvalidArguments => "mcp_invalid_arguments",
        McpError::InvalidConfiguration(_) | McpError::InvalidTransport(_) => {
            "mcp_configuration_error"
        }
        McpError::ServerIdentityMissing
        | McpError::InvalidServerVersion(_)
        | McpError::InvalidToolId(_)
        | McpError::ServerVersionChanged { .. } => "mcp_server_error",
        McpError::Cancelled => "cancelled",
    };
    ToolExecutionError::new(code, error.to_string())
}

impl ToolHandler for ForwardingHandler {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        self.caller
            .call(self.tool_name.clone(), arguments, cancellation)
    }
}
