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
                .map_err(|_| {
                    crate::providers::http::ProviderError::new(
                        crate::providers::http::ProviderErrorCode::ProviderResponse,
                        None,
                        "test model poisoned",
                    )
                })?
                .pop()
                .ok_or_else(|| {
                    crate::providers::http::ProviderError::new(
                        crate::providers::http::ProviderErrorCode::ProviderResponse,
                        None,
                        "test model exhausted",
                    )
                })
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
        self.registration
            .handler
            .execute(invocation.arguments, cancellation)
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
    assert_eq!(
        task.registrations()[0].spec.input_schema["required"],
        json!(["task"])
    );
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

#[tokio::test]
async fn parent_runtime_owns_child_turn_and_keeps_child_events() {
    let runtime = RuntimeHost::default();
    let parent_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    runtime
        .begin_turn(TurnSnapshot {
            turn_id: parent_id,
            session_id,
            parent_turn_id: None,
            child_turn_limit: 2,
            provider: "test".to_string(),
            model: "test".to_string(),
            model_context_window: Some(8_192),
            output_reservation: 2_048,
            reasoning_reservation: 1_024,
            limits: AgentLoopLimits::default(),
            tool_ids: Vec::new(),
            tool_snapshot: Vec::new(),
        })
        .expect("parent turn");
    let task = SubagentTool::new_for_parent(
        model(vec![ChatResponse {
            text: "child conclusion".to_string(),
            ..ChatResponse::default()
        }]),
        Arc::new(crate::agent::runtime::NoopToolExecutor),
        Arc::new(DenyPermissions),
        Arc::new(NoopAgentHooks),
        runtime.clone(),
        parent_id,
    );
    let output = task
        .execute(
            ToolInvocation {
                tool_call_id: Uuid::new_v4(),
                tool_id: TASK_TOOL_ID.to_string(),
                tool_name: TASK_TOOL_NAME.to_string(),
                arguments: json!({"task": "answer"}),
            },
            runtime.cancellation_token(parent_id),
        )
        .await
        .expect("child result");
    let child_id = output["child_turn_id"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("child turn id");
    assert_eq!(output["conclusion"], "child conclusion");
    assert!(!runtime
        .child_events(child_id)
        .expect("child events")
        .is_empty());
    assert!(runtime
        .active_children(parent_id)
        .expect("active children")
        .is_empty());
    runtime.finish_turn(parent_id);
}
