use bloomery::agent::context::{ContextItem, ContextSource};
use bloomery::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentMessageRole, AgentRunState, RunCompleted, RunOutcome,
    RunStateChanged,
};
use bloomery::agent::runtime::{
    AgentEventSink, AgentLoop, AgentLoopRequest, CancellationToken, ContextEntry, DenyPermissions,
    ModelAdapter, ModelFuture, NoopToolExecutor, PermissionRequest, PermissionResolver,
    ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation, ToolRegistration,
};
use bloomery::providers::capabilities::{
    ChatEvent, ChatRequest, ChatResponse, ChatToolCall, ChatUsage, ProviderCapabilities,
};
use bloomery::providers::http::{ProviderError, ProviderErrorCode};
use bloomery::providers::profiles::ProviderKind;
use chrono::Utc;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use uuid::Uuid;

struct ScriptedModel {
    capabilities: ProviderCapabilities,
    responses: Mutex<Vec<Result<ChatResponse, ProviderError>>>,
    requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedModel {
    fn answer(text: &str) -> Self {
        Self {
            capabilities: ProviderCapabilities::chat(ProviderKind::OpenAiCompatible, "test"),
            responses: Mutex::new(vec![Ok(ChatResponse {
                text: text.to_string(),
                usage: Some(ChatUsage {
                    prompt_tokens: 5,
                    completion_tokens: 3,
                    total_tokens: 8,
                }),
                ..ChatResponse::default()
            })]),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn script(responses: Vec<Result<ChatResponse, ProviderError>>) -> Self {
        Self {
            capabilities: ProviderCapabilities::chat(ProviderKind::OpenAiCompatible, "test"),
            responses: Mutex::new(responses.into_iter().rev().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn with_context_limit(limit: usize, response: ChatResponse) -> Self {
        let mut model = Self::script(vec![Ok(response)]);
        model.capabilities.context_window = Some(limit);
        model
    }
}

impl ModelAdapter for ScriptedModel {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn generate<'a>(
        &'a self,
        _request: ChatRequest,
        _on_event: &'a mut (dyn FnMut(ChatEvent) + Send),
        _is_cancelled: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> ModelFuture<'a> {
        self.requests
            .lock()
            .expect("scripted request mutex")
            .push(_request);
        let response = self
            .responses
            .lock()
            .expect("scripted model mutex")
            .pop()
            .expect("scripted response");
        Box::pin(async move { response })
    }
}

struct AllowPermissions;

impl PermissionResolver for AllowPermissions {
    fn decide(
        &self,
        _request: PermissionRequest,
        _cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = bloomery::agent::protocol::PermissionDecision> + Send>> {
        Box::pin(async { bloomery::agent::protocol::PermissionDecision::AllowOnce })
    }
}

struct StaticHandler {
    output: Value,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl ToolHandler for StaticHandler {
    fn execute(&self, arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
        self.calls.lock().expect("tool call mutex").push(arguments);
        let output = self.output.clone();
        Box::pin(async move { Ok(output) })
    }
}

struct TimedHandler {
    active: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
    delay: Duration,
}

impl ToolHandler for TimedHandler {
    fn execute(&self, _arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
        let active = Arc::clone(&self.active);
        let maximum = Arc::clone(&self.maximum);
        let delay = self.delay;
        Box::pin(async move {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(current, Ordering::SeqCst);
            tokio::time::sleep(delay).await;
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(json!({"ok": true}))
        })
    }
}

struct TestTools {
    registrations: Vec<ToolRegistration>,
}

impl ToolExecutor for TestTools {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        let registration = self
            .registrations
            .iter()
            .find(|registration| registration.spec.id == invocation.tool_id);
        match registration {
            Some(registration) => registration
                .handler
                .execute(invocation.arguments, cancellation),
            None => Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "test tool is not registered",
                ))
            }),
        }
    }
}

fn tool(
    id: &str,
    name: &str,
    risk: bloomery::agent::protocol::PermissionRisk,
    read_only: bool,
    handler: Arc<dyn ToolHandler>,
) -> ToolRegistration {
    ToolRegistration::new(
        bloomery::agent::tool_repair::ToolSpec {
            id: id.to_string(),
            name: name.to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"],
                "additionalProperties": false
            }),
            risk,
        },
        read_only,
        handler,
    )
}

fn call(id: &str, name: &str, arguments: &str) -> ChatToolCall {
    ChatToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments: arguments.to_string(),
    }
}

fn response(text: &str, tool_calls: Vec<ChatToolCall>) -> Result<ChatResponse, ProviderError> {
    Ok(ChatResponse {
        text: text.to_string(),
        tool_calls,
        ..ChatResponse::default()
    })
}

fn request(evidence: Option<bloomery::agent::runtime::EvidenceAttachment>) -> AgentLoopRequest {
    AgentLoopRequest {
        assistant_message_id: Uuid::new_v4(),
        context: vec![
            ContextEntry::new(ContextItem::new(
                "system",
                ContextSource::System,
                "Answer accurately.",
            )),
            ContextEntry::new(ContextItem::new(
                "request",
                ContextSource::CurrentRequest,
                "Find steel information.",
            )),
        ],
        output_reservation: 4,
        evidence,
    }
}

struct RecordingSink {
    run_id: Uuid,
    conversation_id: Uuid,
    next_sequence: u64,
    events: Vec<AgentEventEnvelope>,
}

impl RecordingSink {
    fn new() -> Self {
        Self {
            run_id: Uuid::new_v4(),
            conversation_id: Uuid::new_v4(),
            next_sequence: 1,
            events: Vec::new(),
        }
    }

    fn push(&mut self, data: AgentEventData) -> AgentEventEnvelope {
        let event = AgentEventEnvelope {
            protocol_version: 1,
            event_id: Uuid::new_v4(),
            run_id: self.run_id,
            conversation_id: self.conversation_id,
            sequence: self.next_sequence,
            timestamp: Utc::now(),
            data,
        };
        self.next_sequence += 1;
        self.events.push(event.clone());
        event
    }
}

impl AgentEventSink for RecordingSink {
    fn record(&mut self, data: AgentEventData) -> Result<AgentEventEnvelope, String> {
        Ok(self.push(data))
    }

    fn transition(&mut self, changed: RunStateChanged) -> Result<AgentEventEnvelope, String> {
        Ok(self.push(AgentEventData::RunStateChanged(changed)))
    }

    fn finish(
        &mut self,
        changed: RunStateChanged,
        outcome: RunOutcome,
        assistant_message_id: Option<Uuid>,
    ) -> Result<Vec<AgentEventEnvelope>, String> {
        let state = self.push(AgentEventData::RunStateChanged(changed));
        let completed = self.push(AgentEventData::RunCompleted(
            bloomery::agent::protocol::RunCompleted {
                outcome,
                assistant_message_id,
            },
        ));
        Ok(vec![state, completed])
    }
}

#[test]
fn direct_answer_streams_usage_and_completes_once() {
    let model = ScriptedModel::answer("Q355B is a low-alloy structural steel.");
    let mut sink = RecordingSink::new();
    let request = AgentLoopRequest {
        assistant_message_id: Uuid::new_v4(),
        context: vec![
            ContextEntry::new(ContextItem::new(
                "system",
                ContextSource::System,
                "Answer accurately.",
            )),
            ContextEntry::new(ContextItem::new(
                "request",
                ContextSource::CurrentRequest,
                "What is Q355B?",
            )),
        ],
        output_reservation: 128,
        evidence: None,
    };

    let result = tauri::async_runtime::block_on(
        AgentLoop::new(&model, &NoopToolExecutor, &DenyPermissions).run(
            request,
            &mut sink,
            CancellationToken::new(|| false),
        ),
    )
    .expect("direct answer succeeds");

    assert_eq!(result.outcome, RunOutcome::Completed);
    assert_eq!(result.answer, "Q355B is a low-alloy structural steel.");
    assert_eq!(result.usage.unwrap().total_tokens, 8);
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| matches!(event.data, AgentEventData::RunCompleted(_)))
            .count(),
        1
    );
    assert!(sink.events.iter().any(|event| matches!(
        &event.data,
        AgentEventData::MessageCompleted(message)
            if message.role == AgentMessageRole::Assistant
                && message.content == "Q355B is a low-alloy structural steel."
    )));
    assert!(sink.events.iter().any(|event| matches!(
        &event.data,
        AgentEventData::UsageUpdated(usage) if usage.total_tokens == 8
    )));
    assert!(sink.events.iter().any(|event| matches!(
        &event.data,
        AgentEventData::RunStateChanged(change)
            if change.current == AgentRunState::Generating
    )));
}

#[test]
fn one_automatic_tool_is_observed_before_the_final_answer() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let tools = TestTools {
        registrations: vec![tool(
            "search.v1",
            "search",
            bloomery::agent::protocol::PermissionRisk::Automatic,
            true,
            Arc::new(StaticHandler {
                output: json!({"answer": "Q355B"}),
                calls: Arc::clone(&calls),
            }),
        )],
    };
    let model = ScriptedModel::script(vec![
        response("", vec![call("call-1", "search", r#"{"query":"Q355B"}"#)]),
        response("Q355B is structural steel.", vec![]),
    ]);
    let mut sink = RecordingSink::new();

    let result =
        tauri::async_runtime::block_on(AgentLoop::new(&model, &tools, &AllowPermissions).run(
            request(None),
            &mut sink,
            CancellationToken::new(|| false),
        ))
        .expect("tool run succeeds");

    assert_eq!(result.outcome, RunOutcome::Completed);
    assert_eq!(result.answer, "Q355B is structural steel.");
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert!(sink.events.iter().any(|event| matches!(
        &event.data,
        AgentEventData::ToolCompleted(completed)
            if completed.outcome == bloomery::agent::protocol::ToolOutcome::Succeeded
    )));
    assert!(model.requests.lock().unwrap().len() >= 2);
}

#[test]
fn independent_read_tools_run_in_parallel() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let handler = || {
        Arc::new(TimedHandler {
            active: Arc::clone(&active),
            maximum: Arc::clone(&maximum),
            delay: Duration::from_millis(25),
        }) as Arc<dyn ToolHandler>
    };
    let tools = TestTools {
        registrations: vec![
            tool(
                "read-a.v1",
                "read_a",
                bloomery::agent::protocol::PermissionRisk::Automatic,
                true,
                handler(),
            ),
            tool(
                "read-b.v1",
                "read_b",
                bloomery::agent::protocol::PermissionRisk::Automatic,
                true,
                handler(),
            ),
        ],
    };
    let model = ScriptedModel::script(vec![
        response(
            "",
            vec![
                call("call-a", "read_a", r#"{"query":"a"}"#),
                call("call-b", "read_b", r#"{"query":"b"}"#),
            ],
        ),
        response("Both reads completed.", vec![]),
    ]);
    let mut sink = RecordingSink::new();

    tauri::async_runtime::block_on(AgentLoop::new(&model, &tools, &AllowPermissions).run(
        request(None),
        &mut sink,
        CancellationToken::new(|| false),
    ))
    .expect("parallel read run succeeds");

    assert_eq!(maximum.load(Ordering::SeqCst), 2);
}

#[test]
fn write_tools_are_serialized() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let handler = || {
        Arc::new(TimedHandler {
            active: Arc::clone(&active),
            maximum: Arc::clone(&maximum),
            delay: Duration::from_millis(25),
        }) as Arc<dyn ToolHandler>
    };
    let tools = TestTools {
        registrations: vec![
            tool(
                "write-a.v1",
                "write_a",
                bloomery::agent::protocol::PermissionRisk::Automatic,
                false,
                handler(),
            ),
            tool(
                "write-b.v1",
                "write_b",
                bloomery::agent::protocol::PermissionRisk::Automatic,
                false,
                handler(),
            ),
        ],
    };
    let model = ScriptedModel::script(vec![
        response(
            "",
            vec![
                call("call-a", "write_a", r#"{"query":"a"}"#),
                call("call-b", "write_b", r#"{"query":"b"}"#),
            ],
        ),
        response("Both writes completed.", vec![]),
    ]);
    let mut sink = RecordingSink::new();

    tauri::async_runtime::block_on(AgentLoop::new(&model, &tools, &AllowPermissions).run(
        request(None),
        &mut sink,
        CancellationToken::new(|| false),
    ))
    .expect("serial write run succeeds");

    assert_eq!(maximum.load(Ordering::SeqCst), 1);
}

#[test]
fn malformed_tool_call_is_repaired_with_a_bounded_model_retry() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let tools = TestTools {
        registrations: vec![tool(
            "search.v1",
            "search",
            bloomery::agent::protocol::PermissionRisk::Automatic,
            true,
            Arc::new(StaticHandler {
                output: json!({"ok": true}),
                calls: Arc::clone(&calls),
            }),
        )],
    };
    let model = ScriptedModel::script(vec![
        response("", vec![call("bad", "search", r#"{"query":}"#)]),
        response("", vec![call("fixed", "search", r#"{"query":"steel"}"#)]),
        response("Repaired and completed.", vec![]),
    ]);
    let mut sink = RecordingSink::new();

    tauri::async_runtime::block_on(AgentLoop::new(&model, &tools, &AllowPermissions).run(
        request(None),
        &mut sink,
        CancellationToken::new(|| false),
    ))
    .expect("repair run succeeds");

    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(model.requests.lock().unwrap().len(), 3);
}

#[test]
fn rag_answer_attaches_evidence_and_accepts_known_citations() {
    let model = ScriptedModel::script(vec![response("Yield is 355 MPa [1].", vec![])]);
    let mut sink = RecordingSink::new();
    let evidence = bloomery::agent::runtime::EvidenceAttachment {
        evidence_pack_id: Uuid::new_v4(),
        citation_numbers: vec![1],
    };

    let result = tauri::async_runtime::block_on(
        AgentLoop::new(&model, &NoopToolExecutor, &DenyPermissions).run(
            request(Some(evidence)),
            &mut sink,
            CancellationToken::new(|| false),
        ),
    )
    .expect("RAG answer succeeds");

    assert_eq!(result.outcome, RunOutcome::Completed);
    assert!(sink
        .events
        .iter()
        .any(|event| matches!(event.data, AgentEventData::EvidenceAttached(_))));
}

#[test]
fn provider_error_is_persisted_before_failed_completion() {
    let model = ScriptedModel::script(vec![Err(ProviderError::new(
        ProviderErrorCode::Network,
        None,
        "provider offline",
    ))]);
    let mut sink = RecordingSink::new();

    let error = tauri::async_runtime::block_on(
        AgentLoop::new(&model, &NoopToolExecutor, &DenyPermissions).run(
            request(None),
            &mut sink,
            CancellationToken::new(|| false),
        ),
    )
    .expect_err("provider failure must fail the run");

    assert!(error.to_string().contains("provider offline"));
    assert!(sink
        .events
        .iter()
        .any(|event| matches!(event.data, AgentEventData::ErrorRaised(_))));
    assert!(sink.events.iter().any(|event| matches!(
        event.data,
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Failed,
            ..
        })
    )));
}

#[test]
fn cancellation_completes_as_cancelled_without_a_second_terminal_event() {
    let model = ScriptedModel::answer("partial");
    let mut sink = RecordingSink::new();

    let result = tauri::async_runtime::block_on(
        AgentLoop::new(&model, &NoopToolExecutor, &DenyPermissions).run(
            request(None),
            &mut sink,
            CancellationToken::new(|| true),
        ),
    )
    .expect("cancellation is a normal outcome");

    assert_eq!(result.outcome, RunOutcome::Cancelled);
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| matches!(event.data, AgentEventData::RunCompleted(_)))
            .count(),
        1
    );
    assert!(sink.events.iter().any(|event| matches!(
        event.data,
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Cancelled,
            ..
        })
    )));
}

#[test]
fn required_context_overflow_fails_before_calling_the_provider() {
    let model = ScriptedModel::with_context_limit(
        12,
        ChatResponse {
            text: "should not run".to_string(),
            ..ChatResponse::default()
        },
    );
    let mut request = request(None);
    request.context[0] = ContextEntry::new(ContextItem::new(
        "security",
        ContextSource::Security,
        "this security rule is intentionally too long",
    ));
    let mut sink = RecordingSink::new();

    let error = tauri::async_runtime::block_on(
        AgentLoop::new(&model, &NoopToolExecutor, &DenyPermissions).run(
            request,
            &mut sink,
            CancellationToken::new(|| false),
        ),
    )
    .expect_err("required context overflow must fail");

    assert!(error.to_string().contains("required context"));
    assert!(model.requests.lock().unwrap().is_empty());
    assert!(sink.events.iter().any(|event| matches!(
        event.data,
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Failed,
            ..
        })
    )));
}

#[test]
fn selected_recent_turns_are_restored_to_chronological_provider_order() {
    let model = ScriptedModel::answer("ordered");
    let mut sink = RecordingSink::new();
    let request = AgentLoopRequest {
        assistant_message_id: Uuid::new_v4(),
        context: vec![
            ContextEntry::new(ContextItem::new("system", ContextSource::System, "system")),
            ContextEntry::new(ContextItem::new(
                "newest",
                ContextSource::RecentTurn {
                    newest_first_rank: 0,
                },
                "newest",
            )),
            ContextEntry::new(ContextItem::new(
                "oldest",
                ContextSource::RecentTurn {
                    newest_first_rank: 2,
                },
                "oldest",
            )),
            ContextEntry::new(ContextItem::new(
                "request",
                ContextSource::CurrentRequest,
                "request",
            )),
            ContextEntry::new(ContextItem::new(
                "middle",
                ContextSource::RecentTurn {
                    newest_first_rank: 1,
                },
                "middle",
            )),
        ],
        output_reservation: 4,
        evidence: None,
    };

    tauri::async_runtime::block_on(
        AgentLoop::new(&model, &NoopToolExecutor, &DenyPermissions).run(
            request,
            &mut sink,
            CancellationToken::new(|| false),
        ),
    )
    .expect("chronological context run succeeds");

    let requests = model.requests.lock().unwrap();
    assert_eq!(
        requests[0]
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["system", "oldest", "middle", "newest", "request"]
    );
}

#[test]
fn oversized_tool_output_is_bounded_before_the_next_model_call() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let tools = TestTools {
        registrations: vec![tool(
            "large.v1",
            "large",
            bloomery::agent::protocol::PermissionRisk::Automatic,
            true,
            Arc::new(StaticHandler {
                output: Value::String("x".repeat(128 * 1024)),
                calls,
            }),
        )],
    };
    let model = ScriptedModel::script(vec![
        response("", vec![call("large-call", "large", r#"{"query":"x"}"#)]),
        response("bounded", vec![]),
    ]);
    let mut sink = RecordingSink::new();

    tauri::async_runtime::block_on(AgentLoop::new(&model, &tools, &AllowPermissions).run(
        request(None),
        &mut sink,
        CancellationToken::new(|| false),
    ))
    .expect("bounded output run succeeds");

    let output = sink
        .events
        .iter()
        .find_map(|event| match &event.data {
            AgentEventData::ToolCompleted(completed) => completed.output.as_ref(),
            _ => None,
        })
        .expect("tool output event");
    assert_eq!(output["truncated"], true);
    assert!(serde_json::to_string(output).unwrap().len() < 34 * 1024);
}

#[test]
fn denied_write_tool_is_never_executed_and_is_returned_as_an_observation() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let tools = TestTools {
        registrations: vec![tool(
            "write.v1",
            "write",
            bloomery::agent::protocol::PermissionRisk::ConfirmationRequired,
            false,
            Arc::new(StaticHandler {
                output: json!({"should_not": "run"}),
                calls: Arc::clone(&calls),
            }),
        )],
    };
    let model = ScriptedModel::script(vec![
        response("", vec![call("write-call", "write", r#"{"query":"x"}"#)]),
        response("Permission was respected.", vec![]),
    ]);
    let mut sink = RecordingSink::new();

    let result =
        tauri::async_runtime::block_on(AgentLoop::new(&model, &tools, &DenyPermissions).run(
            request(None),
            &mut sink,
            CancellationToken::new(|| false),
        ))
        .expect("denial remains recoverable");

    assert_eq!(result.outcome, RunOutcome::Completed);
    assert!(calls.lock().unwrap().is_empty());
    assert!(sink
        .events
        .iter()
        .any(|event| matches!(event.data, AgentEventData::PermissionRequested(_))));
    assert!(sink.events.iter().any(|event| matches!(
        &event.data,
        AgentEventData::ToolCompleted(completed)
            if completed.outcome == bloomery::agent::protocol::ToolOutcome::Failed
    )));
}

#[test]
fn citation_to_missing_evidence_fails_before_completion() {
    let model = ScriptedModel::script(vec![response("value [2]", vec![])]);
    let mut sink = RecordingSink::new();
    let evidence = bloomery::agent::runtime::EvidenceAttachment {
        evidence_pack_id: Uuid::new_v4(),
        citation_numbers: vec![1],
    };

    let error = tauri::async_runtime::block_on(
        AgentLoop::new(&model, &NoopToolExecutor, &DenyPermissions).run(
            request(Some(evidence)),
            &mut sink,
            CancellationToken::new(|| false),
        ),
    )
    .expect_err("unknown citation must fail");

    assert!(error.to_string().contains("unavailable evidence"));
    assert!(sink.events.iter().any(|event| matches!(
        event.data,
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Failed,
            ..
        })
    )));
}
