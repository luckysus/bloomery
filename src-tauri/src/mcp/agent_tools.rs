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
