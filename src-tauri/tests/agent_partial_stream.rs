use chrono::Utc;
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use suna::agent::context::{ContextItem, ContextSource};
use suna::agent::protocol::{
    AgentEventData, AgentEventEnvelope, PermissionRisk, RunCompleted, RunOutcome, RunStateChanged,
};
use suna::agent::runtime::{
    AgentContextCheckpoint, AgentEventSink, AgentInputQueue, AgentLoop, AgentLoopLimits,
    AgentLoopRequest, CancellationToken, ContextCheckpointReason, ContextEntry, DenyPermissions,
    ModelAdapter, ModelFuture, NoopToolExecutor, ToolExecutor, ToolFuture, ToolHandler,
    ToolInvocation, ToolRegistration,
};
use suna::agent::tool_repair::ToolSpec;
use suna::providers::capabilities::{
    ChatEvent, ChatRequest, ChatResponse, ChatToolCall, ProviderCapabilities,
};
use suna::providers::http::{ProviderError, ProviderErrorCode};
use suna::providers::profiles::ProviderKind;
use uuid::Uuid;

#[derive(Default)]
struct Sink {
    events: Vec<AgentEventEnvelope>,
    checkpoints: Vec<AgentContextCheckpoint>,
}

impl AgentEventSink for Sink {
    fn record(&mut self, data: AgentEventData) -> Result<AgentEventEnvelope, String> {
        let event = AgentEventEnvelope {
            protocol_version: 1,
            event_id: Uuid::new_v4(),
            run_id: Uuid::nil(),
            conversation_id: Uuid::nil(),
            sequence: self.events.len() as u64 + 1,
            timestamp: Utc::now(),
            data,
        };
        self.events.push(event.clone());
        Ok(event)
    }

    fn transition(&mut self, changed: RunStateChanged) -> Result<AgentEventEnvelope, String> {
        self.record(AgentEventData::RunStateChanged(changed))
    }

    fn finish(
        &mut self,
        changed: RunStateChanged,
        outcome: RunOutcome,
        assistant_message_id: Option<Uuid>,
    ) -> Result<Vec<AgentEventEnvelope>, String> {
        Ok(vec![
            self.transition(changed)?,
            self.record(AgentEventData::RunCompleted(RunCompleted {
                outcome,
                assistant_message_id,
            }))?,
        ])
    }

    fn checkpoint(&mut self, checkpoint: AgentContextCheckpoint) -> Result<(), String> {
        self.checkpoints.push(checkpoint);
        Ok(())
    }
}

fn request() -> AgentLoopRequest {
    AgentLoopRequest {
        assistant_message_id: Uuid::new_v4(),
        context: vec![
            ContextEntry::new(ContextItem::new("system", ContextSource::System, "Answer.")),
            ContextEntry::new(ContextItem::new(
                "request",
                ContextSource::CurrentRequest,
                "Search first.",
            )),
        ],
        output_reservation: 64,
        reasoning_reservation: 0,
        evidence: None,
        attachments: vec![],
        limits: AgentLoopLimits::default(),
        input_queue: AgentInputQueue::default(),
        resume: None,
    }
}

struct SearchTool(Vec<ToolRegistration>);

impl SearchTool {
    fn new() -> Self {
        Self(vec![ToolRegistration::new(
            ToolSpec {
                id: "test.search".into(),
                name: "search".into(),
                input_schema: json!({"type": "object", "additionalProperties": false}),
                risk: PermissionRisk::Automatic,
            },
            true,
            Arc::new(SearchHandler),
        )])
    }
}

struct SearchHandler;

impl ToolHandler for SearchHandler {
    fn execute(&self, _: serde_json::Value, _: CancellationToken) -> ToolFuture {
        Box::pin(async { Ok(json!({"found": true})) })
    }
}

impl ToolExecutor for SearchTool {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.0
    }

    fn execute(&self, _: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        self.0[0].handler.execute(json!({}), cancellation)
    }
}

#[derive(Clone, Copy)]
enum Fault {
    Cancel,
    Timeout,
    Network,
}

struct PartialModel {
    capabilities: ProviderCapabilities,
    fault: Fault,
    cancelled: Arc<AtomicBool>,
    attempts: AtomicUsize,
    tool_round: bool,
}

impl ModelAdapter for PartialModel {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn generate<'a>(
        &'a self,
        _: ChatRequest,
        on_event: &'a mut (dyn FnMut(ChatEvent) + Send),
        _: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> ModelFuture<'a> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
        if self.tool_round && attempt == 0 {
            on_event(ChatEvent::TextDelta("A".into()));
            return Box::pin(async {
                Ok(ChatResponse {
                    text: "A".into(),
                    tool_calls: vec![ChatToolCall {
                        id: "search-1".into(),
                        name: "search".into(),
                        arguments: "{}".into(),
                    }],
                    ..ChatResponse::default()
                })
            });
        }
        on_event(ChatEvent::ReasoningDelta("checking".into()));
        on_event(ChatEvent::TextDelta("B".into()));
        if matches!(self.fault, Fault::Cancel) {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        Box::pin(async move {
            if !matches!(self.fault, Fault::Network) {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(ProviderError::new(
                ProviderErrorCode::Network,
                None,
                "offline",
            ))
        })
    }
}

#[test]
fn partial_text_survives_cancel_timeout_and_provider_failure_once() {
    for tool_round in [false, true] {
        for fault in [Fault::Cancel, Fault::Timeout, Fault::Network] {
            let cancelled = Arc::new(AtomicBool::new(false));
            let model = PartialModel {
                capabilities: ProviderCapabilities::chat(ProviderKind::OpenAiCompatible, "test"),
                fault,
                cancelled: cancelled.clone(),
                attempts: AtomicUsize::new(0),
                tool_round,
            };
            let mut input = request();
            input.limits.model_request_timeout_ms = 10;
            input.limits.max_network_retries = 3;
            let tools = SearchTool::new();
            let mut sink = Sink::default();
            let result = tauri::async_runtime::block_on(
                AgentLoop::new(&model, &tools, &DenyPermissions).run(
                    input,
                    &mut sink,
                    CancellationToken::new(move || cancelled.load(Ordering::SeqCst)),
                ),
            );
            let expected = if tool_round { "AB" } else { "B" };
            let expected_outcome = if matches!(fault, Fault::Cancel) {
                let result = result.expect("user cancellation returns partial output");
                assert_eq!(result.answer, expected);
                assert_eq!(result.reasoning, "checking");
                RunOutcome::Cancelled
            } else {
                let error = result.expect_err("timeout and provider failure stay errors");
                if matches!(fault, Fault::Timeout) {
                    assert!(error.to_string().contains("single model request"));
                } else {
                    assert!(error.to_string().contains("offline"));
                }
                RunOutcome::Failed
            };
            assert_eq!(
                model.attempts.load(Ordering::SeqCst),
                if tool_round { 2 } else { 1 }
            );
            let completed: Vec<_> = sink
                .events
                .iter()
                .filter_map(|event| match &event.data {
                    AgentEventData::MessageCompleted(message) => Some(message),
                    _ => None,
                })
                .collect();
            assert_eq!(completed.len(), 1);
            assert_eq!(completed[0].content, expected);
            assert!(completed[0].partial);
            let terminals: Vec<_> = sink
                .events
                .iter()
                .filter_map(|event| match &event.data {
                    AgentEventData::RunCompleted(run) => Some(run),
                    _ => None,
                })
                .collect();
            assert_eq!(terminals.len(), 1);
            assert_eq!(terminals[0].outcome, expected_outcome);
            let checkpoint = sink.checkpoints.last().expect("partial error checkpoint");
            assert_eq!(checkpoint.reason, ContextCheckpointReason::AssistantError);
            let partial = checkpoint.messages.last().unwrap();
            assert_eq!(partial.role, "assistant");
            assert_eq!(partial.content, "B");
            assert_eq!(partial.reasoning_content.as_deref(), Some("checking"));
        }
    }
}

struct ContextLimitModel {
    capabilities: ProviderCapabilities,
    requests: Mutex<Vec<ChatRequest>>,
}

impl ModelAdapter for ContextLimitModel {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn generate<'a>(
        &'a self,
        request: ChatRequest,
        _: &'a mut (dyn FnMut(ChatEvent) + Send),
        _: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> ModelFuture<'a> {
        let mut requests = self.requests.lock().unwrap();
        requests.push(request);
        let first = requests.len() == 1;
        Box::pin(async move {
            if first {
                Err(ProviderError::new(
                    ProviderErrorCode::ContextLimit,
                    None,
                    "too long",
                ))
            } else {
                Ok(ChatResponse {
                    text: "ok".into(),
                    ..ChatResponse::default()
                })
            }
        })
    }
}

#[test]
fn context_retry_shrinks_the_model_window_with_exact_completion_reservations() {
    let mut capabilities = ProviderCapabilities::chat(ProviderKind::OpenAiCompatible, "test");
    capabilities.context_window = Some(1_024);
    let model = ContextLimitModel {
        capabilities,
        requests: Mutex::new(vec![]),
    };
    let mut input = request();
    input.context[1].item.content = "x".repeat(2_100);
    input.reasoning_reservation = 96;
    input.limits.context_budget = Some(4_096);
    input.limits.model_max_tokens = Some(7);
    let mut sink = Sink::default();
    let result = tauri::async_runtime::block_on(
        AgentLoop::new(&model, &NoopToolExecutor, &DenyPermissions).run(
            input,
            &mut sink,
            CancellationToken::new(|| false),
        ),
    )
    .expect("context retry respects actual reservations");
    assert_eq!(result.answer, "ok");
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1].messages.last().unwrap().content.len()
            < requests[0].messages.last().unwrap().content.len()
    );
    assert_eq!(requests[1].max_tokens, Some(7));
}
