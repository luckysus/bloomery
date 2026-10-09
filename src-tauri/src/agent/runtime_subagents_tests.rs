use super::*;
use crate::agent::protocol::{
    AgentError, AgentErrorCategory, AgentEventData, AgentEventEnvelope, ErrorRaised,
    EvidenceAttached, RunOutcome, PROTOCOL_VERSION,
};
use crate::agent::runtime::{DenyPermissions, NoopAgentHooks};
use crate::providers::capabilities::{ChatResponse, ProviderCapabilities};
use crate::providers::profiles::{ProviderCapability, ProviderKind};
use chrono::Utc;
use std::sync::Mutex;

struct TestModel {
    responses: Mutex<Vec<ChatResponse>>,
    capabilities: ProviderCapabilities,
}

struct FailingModel {
    capabilities: ProviderCapabilities,
}

impl ModelAdapter for FailingModel {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn generate<'a>(
        &'a self,
        _request: crate::providers::capabilities::ChatRequest,
        _on_event: &'a mut (dyn FnMut(crate::providers::capabilities::ChatEvent) + Send),
        _is_cancelled: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> super::super::ModelFuture<'a> {
        Box::pin(async {
            Err(crate::providers::http::ProviderError::new(
                crate::providers::http::ProviderErrorCode::ProviderResponse,
                None,
                "child model failed",
            ))
        })
    }
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

#[test]
fn child_result_details_preserve_evidence_and_errors() {
    let child_turn_id = Uuid::new_v4();
    let events = vec![
        AgentEventEnvelope {
            protocol_version: PROTOCOL_VERSION,
            event_id: Uuid::new_v4(),
            run_id: child_turn_id,
            conversation_id: Uuid::new_v4(),
            sequence: 1,
            timestamp: Utc::now(),
            data: AgentEventData::EvidenceAttached(EvidenceAttached {
                evidence_pack_id: Uuid::new_v4(),
                citation_numbers: vec![1, 3],
            }),
        },
        AgentEventEnvelope {
            protocol_version: PROTOCOL_VERSION,
            event_id: Uuid::new_v4(),
            run_id: child_turn_id,
            conversation_id: Uuid::new_v4(),
            sequence: 2,
            timestamp: Utc::now(),
            data: AgentEventData::ErrorRaised(ErrorRaised {
                error: AgentError {
                    code: "child_failed".to_string(),
                    category: AgentErrorCategory::Internal,
                    message: "synthetic failure".to_string(),
                    retryable: false,
                    details: None,
                },
                fatal: true,
            }),
        },
    ];

    let details = child_result_details(child_turn_id, Some(RunOutcome::Failed), &events);
    assert_eq!(details["outcome"], "failed");
    assert_eq!(details["evidence"][0]["citation_numbers"], json!([1, 3]));
    assert_eq!(details["errors"][0]["error"]["code"], "child_failed");
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
    assert_eq!(output["outcome"], "completed");
    assert!(output["evidence"].as_array().is_some_and(Vec::is_empty));
    assert!(output["errors"].as_array().is_some_and(Vec::is_empty));
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

#[tokio::test]
async fn failed_child_returns_structured_error_details_to_parent() {
    let runtime = RuntimeHost::default();
    let parent_id = Uuid::new_v4();
    runtime
        .begin_turn(TurnSnapshot {
            turn_id: parent_id,
            session_id: Uuid::new_v4(),
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
        Arc::new(FailingModel {
            capabilities: ProviderCapabilities::chat(ProviderKind::OpenAiCompatible, "test"),
        }),
        Arc::new(crate::agent::runtime::NoopToolExecutor),
        Arc::new(DenyPermissions),
        Arc::new(NoopAgentHooks),
        runtime.clone(),
        parent_id,
    );

    let error = task
        .execute(
            ToolInvocation {
                tool_call_id: Uuid::new_v4(),
                tool_id: TASK_TOOL_ID.to_string(),
                tool_name: TASK_TOOL_NAME.to_string(),
                arguments: json!({"task": "fail"}),
            },
            runtime.cancellation_token(parent_id),
        )
        .await
        .expect_err("child failure should reach parent");
    let details = error.details.expect("structured child details");
    assert!(details["child_turn_id"].as_str().is_some());
    assert!(details["errors"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty()));
    runtime.finish_turn(parent_id);
}

struct CapturingModel {
    requests: Arc<Mutex<Vec<crate::providers::capabilities::ChatRequest>>>,
    capabilities: ProviderCapabilities,
}

impl ModelAdapter for CapturingModel {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn generate<'a>(
        &'a self,
        request: crate::providers::capabilities::ChatRequest,
        _on_event: &'a mut (dyn FnMut(crate::providers::capabilities::ChatEvent) + Send),
        _is_cancelled: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> super::super::ModelFuture<'a> {
        self.requests.lock().unwrap().push(request);
        Box::pin(async {
            Ok(ChatResponse {
                text: "expert conclusion".to_string(),
                ..ChatResponse::default()
            })
        })
    }
}

#[derive(Default)]
struct RecordingStore {
    snapshots: Mutex<Vec<TurnSnapshot>>,
}

impl ChildTurnStore for RecordingStore {
    fn begin(&self, snapshot: &TurnSnapshot) -> Result<(), String> {
        self.snapshots.lock().unwrap().push(snapshot.clone());
        Ok(())
    }
    fn append(&self, _event: &AgentEventEnvelope) -> Result<(), String> {
        Ok(())
    }
    fn finish(&self, _child_turn_id: Uuid, _outcome: RunOutcome) -> Result<(), String> {
        Ok(())
    }
}

fn echo_tools() -> Arc<dyn ToolExecutor> {
    Arc::new(TestTools {
        registration: ToolRegistration::new(
            ToolSpec {
                id: "test.echo".to_string(),
                name: "echo".to_string(),
                input_schema: json!({"type":"object", "properties":{"value":{"type":"string"}}, "required":["value"], "additionalProperties":false}),
                risk: crate::agent::protocol::PermissionRisk::Automatic,
            },
            true,
            Arc::new(EchoHandler),
        ),
    })
}

#[tokio::test]
async fn expert_task_selects_its_model_prompt_tools_and_bounded_snapshot() {
    let runtime = RuntimeHost::default();
    let parent_id = Uuid::new_v4();
    let tools = echo_tools();
    runtime
        .begin_turn(TurnSnapshot {
            turn_id: parent_id,
            session_id: Uuid::new_v4(),
            parent_turn_id: None,
            child_turn_limit: 2,
            provider: "parent-provider".to_string(),
            model: "parent-model".to_string(),
            model_context_window: Some(8192),
            output_reservation: 1024,
            reasoning_reservation: 512,
            limits: AgentLoopLimits {
                max_model_calls: Some(5),
                max_tool_calls: Some(10),
                max_tool_rounds: Some(4),
                model_request_timeout_ms: 77000,
                max_network_retries: 1,
                model_temperature: 700,
                model_max_tokens: Some(1536),
                ..AgentLoopLimits::default()
            },
            tool_ids: Vec::new(),
            tool_snapshot: Vec::new(),
        })
        .unwrap();
    runtime
        .set_tool_snapshot(parent_id, tools.as_ref())
        .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::new(RecordingStore::default());
    let expert = SubagentProfile {
        id: "steel".to_string(),
        system_prompt: "Use steel-specific evidence.".to_string(),
        model: Arc::new(CapturingModel {
            requests: requests.clone(),
            capabilities: ProviderCapabilities::chat(
                ProviderKind::OpenAiCompatible,
                "expert-model",
            ),
        }),
        tools: tools.clone(),
        limits: AgentLoopLimits {
            max_model_calls: Some(3),
            max_tool_calls: Some(100),
            ..AgentLoopLimits::default()
        },
        provider: "expert-provider".to_string(),
        model_name: "expert-model".to_string(),
    };
    let task = SubagentTool::new_for_parent_with_profiles(
        model(Vec::new()),
        tools,
        Arc::new(DenyPermissions),
        Arc::new(NoopAgentHooks),
        runtime.clone(),
        parent_id,
        store.clone(),
        vec![expert],
    );
    let output = task
        .execute(
            ToolInvocation {
                tool_call_id: Uuid::new_v4(),
                tool_id: TASK_TOOL_ID.to_string(),
                tool_name: TASK_TOOL_NAME.to_string(),
                arguments: json!({"task":"Review local evidence", "agent_id":"steel"}),
            },
            CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(output["conclusion"], "expert conclusion");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].messages[0]
        .content
        .contains("Use steel-specific evidence."));
    assert_eq!(requests[0].temperature, 0.7);
    assert_eq!(requests[0].max_tokens, Some(1536));
    let snapshots = store.snapshots.lock().unwrap();
    let snapshot = &snapshots[0];
    assert_eq!(snapshot.provider, "expert-provider");
    assert_eq!(snapshot.model, "expert-model");
    assert_eq!(snapshot.output_reservation, 1024);
    assert_eq!(snapshot.reasoning_reservation, 512);
    assert_eq!(snapshot.limits.max_model_calls, Some(3));
    assert_eq!(snapshot.limits.max_tool_calls, Some(10));
    assert_eq!(snapshot.limits.max_tool_rounds, Some(4));
    assert_eq!(snapshot.limits.model_request_timeout_ms, 77000);
    assert_eq!(snapshot.limits.max_network_retries, 1);
    assert_eq!(snapshot.tool_ids, ["test.echo"]);
    runtime.finish_turn(parent_id);
}

#[tokio::test]
async fn disabled_expert_and_tools_outside_parent_scope_cannot_be_selected() {
    let task = SubagentTool::new(
        model(Vec::new()),
        echo_tools(),
        Arc::new(DenyPermissions),
        Arc::new(NoopAgentHooks),
    );
    let error = task
        .execute(
            ToolInvocation {
                tool_call_id: Uuid::new_v4(),
                tool_id: TASK_TOOL_ID.to_string(),
                tool_name: TASK_TOOL_NAME.to_string(),
                arguments: json!({"task":"answer", "agent_id":"disabled"}),
            },
            CancellationToken::new(|| false),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "agent_profile_disabled");
    let tools = echo_tools();
    let parent = crate::agent::runtime::NoopToolExecutor;
    assert!(SnapshotToolExecutor::intersect(tools.as_ref(), &parent)
        .registrations()
        .is_empty());
    let mut matching = SnapshotToolExecutor::intersect(tools.as_ref(), tools.as_ref());
    assert_eq!(matching.registrations().len(), 1);
    matching.restrict_to_ids(&["other.tool".to_string()]);
    assert!(matching.registrations().is_empty());
}

#[tokio::test]
async fn child_snapshot_preserves_invocation_identity_for_training_sources() {
    struct IdentityHandler;
    impl ToolHandler for IdentityHandler {
        fn execute(&self, _arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
            Box::pin(async {
                Err(ToolExecutionError::new(
                    "identity_lost",
                    "invocation required",
                ))
            })
        }
        fn execute_for_invocation(
            &self,
            invocation: ToolInvocation,
            _cancellation: CancellationToken,
        ) -> ToolFuture {
            Box::pin(async move { Ok(json!({"tool_call_id":invocation.tool_call_id})) })
        }
    }
    let tools = TestTools {
        registration: ToolRegistration::new(
            ToolSpec {
                id: "test.identity".to_string(),
                name: "identity".to_string(),
                input_schema: json!({"type":"object"}),
                risk: crate::agent::protocol::PermissionRisk::Automatic,
            },
            true,
            Arc::new(IdentityHandler),
        ),
    };
    let invocation = ToolInvocation {
        tool_call_id: Uuid::new_v4(),
        tool_id: "test.identity".to_string(),
        tool_name: "identity".to_string(),
        arguments: json!({}),
    };
    let id = invocation.tool_call_id;
    let output = SnapshotToolExecutor::from(&tools)
        .execute(invocation, CancellationToken::new(|| false))
        .await
        .unwrap();
    assert_eq!(output["tool_call_id"], id.to_string());
}
