use super::{
    AgentEventSink, AgentHooks, AgentLoop, AgentLoopRequest, CancellationToken, ContextEntry,
    ModelAdapter, PermissionResolver,
    ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation, ToolRegistration,
};
use crate::agent::context::{ContextItem, ContextSource};
use crate::agent::protocol::{AgentEventData, AgentEventEnvelope, RunOutcome, RunStateChanged};
use crate::agent::tool_repair::ToolSpec;
use crate::providers::profiles::ProviderCapability;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

pub const MAX_SUBAGENT_TOOL_ROUNDS: usize = 30;
const TASK_TOOL_ID: &str = "agent.task";
const TASK_TOOL_NAME: &str = "task";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskRequest {
    task: String,
}

pub struct SubagentTool {
    registration: ToolRegistration,
}

impl SubagentTool {
    pub fn new(
        model: Arc<dyn ModelAdapter>,
        tools: Arc<dyn ToolExecutor>,
        permissions: Arc<dyn PermissionResolver>,
        hooks: Arc<dyn AgentHooks>,
    ) -> Self {
        let handler = SubagentHandler {
            model,
            tools,
            permissions,
            hooks,
        };
        Self {
            registration: ToolRegistration::new(
                ToolSpec {
                    id: TASK_TOOL_ID.to_string(),
                    name: TASK_TOOL_NAME.to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {"task": {"type": "string", "minLength": 1}},
                        "required": ["task"],
                        "additionalProperties": false
                    }),
                    risk: crate::agent::protocol::PermissionRisk::Automatic,
                },
                true,
                Arc::new(handler),
            ),
        }
    }
}

impl ToolExecutor for SubagentTool {
    fn registrations(&self) -> &[ToolRegistration] {
        std::slice::from_ref(&self.registration)
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        self.registration.handler.execute(invocation.arguments, cancellation)
    }
}

struct SubagentHandler {
    model: Arc<dyn ModelAdapter>,
    tools: Arc<dyn ToolExecutor>,
    permissions: Arc<dyn PermissionResolver>,
    hooks: Arc<dyn AgentHooks>,
}

impl ToolHandler for SubagentHandler {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        let model = Arc::clone(&self.model);
        let tools = Arc::clone(&self.tools);
        let permissions = Arc::clone(&self.permissions);
        let hooks = Arc::clone(&self.hooks);
        Box::pin(async move {
            let task = serde_json::from_value::<TaskRequest>(arguments)
                .map_err(|error| ToolExecutionError::new("invalid_task", error.to_string()))?;
            let task = task.task.trim().to_string();
            if task.is_empty() {
                return Err(ToolExecutionError::new(
                    "invalid_task",
                    "task must not be empty",
                ));
            }
            if tools.registrations().iter().any(|registration| {
                registration.spec.id == TASK_TOOL_ID || registration.spec.name == TASK_TOOL_NAME
            }) {
                return Err(ToolExecutionError::new(
                    "subagent_configuration_error",
                    "subagent tools must not include task",
                ));
            }
            if !model.capabilities().supports(ProviderCapability::Chat) {
                return Err(ToolExecutionError::new(
                    "subagent_configuration_error",
                    "subagent model does not support chat",
                ));
            }
            let request = AgentLoopRequest {
                assistant_message_id: Uuid::new_v4(),
                context: vec![
                    ContextEntry::new(ContextItem::new(
                        "subagent-system",
                        ContextSource::System,
                        "You are a focused subagent. Complete only the delegated task and return a concise conclusion. Do not delegate further.",
                    )),
                    ContextEntry::new(ContextItem::new(
                        "subagent-task",
                        ContextSource::CurrentRequest,
                        task,
                    )),
                ],
                output_reservation: 2_048,
                evidence: None,
                attachments: Vec::new(),
            };
            let runner = AgentLoop::new_with_hooks(
                model.as_ref(),
                tools.as_ref(),
                permissions.as_ref(),
                hooks.as_ref(),
            );
            let mut sink = NullAgentEventSink::new();
            let result = runner
                .run_with_max_tool_rounds(
                    request,
                    &mut sink,
                    cancellation,
                    MAX_SUBAGENT_TOOL_ROUNDS,
                )
                .await
                .map_err(|error| {
                    let code = if error.to_string().contains("maximum tool rounds") {
                        "subagent_turn_limit"
                    } else {
                        "subagent_execution_error"
                    };
                    ToolExecutionError::new(code, "subagent execution failed")
                })?;
            if result.outcome != RunOutcome::Completed {
                return Err(ToolExecutionError::new(
                    "subagent_execution_error",
                    "subagent did not complete",
                ));
            }
            Ok(json!({"conclusion": result.answer}))
        })
    }
}

pub struct SnapshotToolExecutor {
    registrations: Vec<ToolRegistration>,
}

impl SnapshotToolExecutor {
    pub fn from(source: &dyn ToolExecutor) -> Self {
        Self {
            registrations: source
                .registrations()
                .iter()
                .filter(|registration| {
                    registration.spec.id != TASK_TOOL_ID && registration.spec.name != TASK_TOOL_NAME
                })
                .cloned()
                .collect(),
        }
    }
}

impl ToolExecutor for SnapshotToolExecutor {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        let Some(registration) = self.registrations.iter().find(|registration| {
            registration.spec.id == invocation.tool_id && registration.spec.name == invocation.tool_name
        }) else {
            return Box::pin(async {
                Err(ToolExecutionError::new("tool_not_registered", "tool is not registered"))
            });
        };
        registration.handler.execute(invocation.arguments, cancellation)
    }
}

struct NullAgentEventSink {
    sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::runtime::{DenyPermissions, NoopAgentHooks};
    use crate::providers::capabilities::{ChatResponse, ProviderCapabilities};
    use crate::providers::profiles::{ProviderCapability, ProviderKind};
    use std::sync::Mutex;

    struct TestModel {
        responses: Mutex<Vec<ChatResponse>>,
        capabilities: ProviderCapabilities,
    }

    impl ModelAdapter for TestModel {
        fn capabilities(&self) -> &ProviderCapabilities {
            &self.capabilities
        }

        fn generate<'a>(
            &'a self,
            _request: crate::providers::capabilities::ChatRequest,
            _on_event: &'a mut (dyn FnMut(crate::providers::capabilities::ChatEvent) + Send),
            _is_cancelled: &'a (dyn Fn() -> bool + Send + Sync),
        ) -> super::super::ModelFuture<'a> {
            Box::pin(async move {
                self.responses
                    .lock()
                    .map_err(|_| crate::providers::http::ProviderError::new(
                        crate::providers::http::ProviderErrorCode::ProviderResponse,
                        None,
                        "test model poisoned",
                    ))?
                    .pop()
                    .ok_or_else(|| crate::providers::http::ProviderError::new(
                        crate::providers::http::ProviderErrorCode::ProviderResponse,
                        None,
                        "test model exhausted",
                    ))
            })
        }
    }

    struct TestTools {
        registration: ToolRegistration,
    }

    impl ToolExecutor for TestTools {
        fn registrations(&self) -> &[ToolRegistration] {
            std::slice::from_ref(&self.registration)
        }

        fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
            self.registration.handler.execute(invocation.arguments, cancellation)
        }
    }

    struct EchoHandler;

    impl ToolHandler for EchoHandler {
        fn execute(&self, arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
            Box::pin(async move { Ok(json!({"echo": arguments["value"]})) })
        }
    }

    fn model(responses: Vec<ChatResponse>) -> Arc<dyn ModelAdapter> {
        Arc::new(TestModel {
            responses: Mutex::new(responses),
            capabilities: ProviderCapabilities {
                provider_kind: ProviderKind::OpenAiCompatible,
                model_id: "test".to_string(),
                capabilities: vec![ProviderCapability::Chat],
                context_window: Some(8_192),
                streaming: false,
                tool_calls: true,
                json_schema: true,
                max_batch_size: None,
            },
        })
    }

    #[test]
    fn child_uses_fresh_request_and_cannot_see_task() {
        let tools: Arc<dyn ToolExecutor> = Arc::new(TestTools {
            registration: ToolRegistration::new(
                ToolSpec {
                    id: "test.echo".to_string(),
                    name: "echo".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {"value": {"type": "string"}},
                        "required": ["value"],
                        "additionalProperties": false
                    }),
                    risk: crate::agent::protocol::PermissionRisk::Automatic,
                },
                true,
                Arc::new(EchoHandler),
            ),
        });
        let task = SubagentTool::new(
            model(vec![ChatResponse {
                text: "child conclusion".to_string(),
                ..ChatResponse::default()
            }]),
            tools,
            Arc::new(DenyPermissions),
            Arc::new(NoopAgentHooks),
        );
        assert_eq!(task.registrations()[0].spec.name, TASK_TOOL_NAME);
        assert_eq!(task.registrations()[0].spec.input_schema["required"], json!(["task"]));
    }

    #[test]
    fn snapshot_filters_recursive_task() {
        let source = SubagentTool::new(
            model(vec![]),
            Arc::new(crate::agent::runtime::NoopToolExecutor),
            Arc::new(DenyPermissions),
            Arc::new(NoopAgentHooks),
        );
        let snapshot = SnapshotToolExecutor::from(&source);
        assert!(snapshot.registrations().is_empty());
    }
}

impl NullAgentEventSink {
    fn new() -> Self {
        Self { sequence: 0 }
    }

    fn event(&mut self, data: AgentEventData) -> AgentEventEnvelope {
        self.sequence += 1;
        AgentEventEnvelope {
            protocol_version: crate::agent::protocol::PROTOCOL_VERSION,
            event_id: Uuid::new_v4(),
            run_id: Uuid::nil(),
            conversation_id: Uuid::nil(),
            sequence: self.sequence,
            timestamp: chrono::Utc::now(),
            data,
        }
    }
}

impl AgentEventSink for NullAgentEventSink {
    fn record(&mut self, data: AgentEventData) -> Result<AgentEventEnvelope, String> {
        Ok(self.event(data))
    }

    fn transition(&mut self, changed: RunStateChanged) -> Result<AgentEventEnvelope, String> {
        Ok(self.event(AgentEventData::RunStateChanged(changed)))
    }

    fn finish(
        &mut self,
        changed: RunStateChanged,
        outcome: RunOutcome,
        assistant_message_id: Option<Uuid>,
    ) -> Result<Vec<AgentEventEnvelope>, String> {
        Ok(vec![
            self.event(AgentEventData::RunStateChanged(changed)),
            self.event(AgentEventData::RunCompleted(
                crate::agent::protocol::RunCompleted {
                    outcome,
                    assistant_message_id,
                },
            )),
        ])
    }
}
