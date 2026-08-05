use super::model_adapter::ModelAdapter;
use super::state_machine::{RunGuards, RunStateMachine};
use crate::agent::context::{
    budget_context, ContextBudgetError, ContextItem, ContextReport, ContextSource,
};
use crate::agent::protocol::{
    AgentError, AgentErrorCategory, AgentEventData, AgentEventEnvelope, AgentMessageRole,
    AgentRunState, ErrorRaised, EvidenceAttached, MessageCompleted, MessageDelta,
    PermissionDecision, PermissionRequested, PermissionResolved, PermissionRisk, RunOutcome,
    RunStateChanged, ToolCompleted, ToolOutcome, ToolRequested, ToolStarted, UsageUpdated,
};
use crate::agent::tool_repair::{repair_tool_call, ToolRepairError, ToolSpec, MAX_REPAIR_RETRIES};
use crate::providers::capabilities::{
    ChatEvent, ChatMessage, ChatRequest, ChatResponse, ChatToolCall, ChatUsage,
};
use crate::providers::http::{ProviderError, ProviderErrorCode};
use crate::providers::profiles::ProviderCapability;
use futures_util::future::join_all;
use serde_json::{json, Value};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

const MAX_TOOL_ROUNDS: usize = 8;
const MAX_TOOL_OUTPUT_BYTES: usize = 32 * 1024;

pub type ToolFuture =
    Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + 'static>>;

#[derive(Clone)]
pub struct CancellationToken(Arc<dyn Fn() -> bool + Send + Sync>);

impl CancellationToken {
    pub fn new(callback: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(callback))
    }

    pub fn is_cancelled(&self) -> bool {
        (self.0)()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionError {
    pub code: String,
    pub message: String,
    pub cancelled: bool,
}

impl ToolExecutionError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            cancelled: false,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            code: "cancelled".to_string(),
            message: "tool execution was cancelled".to_string(),
            cancelled: true,
        }
    }
}

#[derive(Clone)]
pub struct ToolRegistration {
    pub spec: ToolSpec,
    pub read_only: bool,
    pub handler: Arc<dyn ToolHandler>,
}

impl ToolRegistration {
    pub fn new(spec: ToolSpec, read_only: bool, handler: Arc<dyn ToolHandler>) -> Self {
        Self {
            spec,
            read_only,
            handler,
        }
    }
}

pub trait ToolHandler: Send + Sync {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture;
}

pub trait ToolExecutor: Send + Sync {
    fn registrations(&self) -> &[ToolRegistration];

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture;
}

#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub tool_call_id: Uuid,
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone)]
pub struct EvidenceAttachment {
    pub evidence_pack_id: Uuid,
    pub citation_numbers: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct ContextEntry {
    pub item: ContextItem,
    pub role: AgentMessageRole,
}

impl ContextEntry {
    pub fn new(item: ContextItem) -> Self {
        let role = match item.source {
            ContextSource::CurrentRequest | ContextSource::RecentTurn { .. } => {
                AgentMessageRole::User
            }
            _ => AgentMessageRole::System,
        };
        Self { item, role }
    }

    pub fn with_role(item: ContextItem, role: AgentMessageRole) -> Self {
        Self { item, role }
    }
}

#[derive(Debug, Clone)]
pub struct AgentLoopRequest {
    pub assistant_message_id: Uuid,
    pub context: Vec<ContextEntry>,
    pub output_reservation: usize,
    pub evidence: Option<EvidenceAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLoopResult {
    pub outcome: RunOutcome,
    pub answer: String,
    pub usage: Option<ChatUsage>,
    pub context: ContextReport,
}

pub trait PermissionResolver: Send + Sync {
    fn decide(&self, request: &PermissionRequest) -> PermissionDecision;
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub permission_id: Uuid,
    pub tool_call_id: Uuid,
    pub tool_name: String,
    pub risk: PermissionRisk,
    pub arguments: Value,
}

pub struct DenyPermissions;

impl PermissionResolver for DenyPermissions {
    fn decide(&self, _request: &PermissionRequest) -> PermissionDecision {
        PermissionDecision::Deny
    }
}

pub struct NoopToolExecutor;

impl ToolExecutor for NoopToolExecutor {
    fn registrations(&self) -> &[ToolRegistration] {
        &[]
    }

    fn execute(&self, _invocation: ToolInvocation, _cancellation: CancellationToken) -> ToolFuture {
        Box::pin(async {
            Err(ToolExecutionError::new(
                "tool_not_registered",
                "no local tools are registered",
            ))
        })
    }
}

pub trait AgentEventSink: Send {
    fn record(&mut self, data: AgentEventData) -> Result<AgentEventEnvelope, String>;

    fn transition(&mut self, changed: RunStateChanged) -> Result<AgentEventEnvelope, String>;

    fn finish(
        &mut self,
        changed: RunStateChanged,
        outcome: RunOutcome,
        assistant_message_id: Option<Uuid>,
    ) -> Result<Vec<AgentEventEnvelope>, String>;
}

#[derive(Debug)]
pub enum AgentLoopError {
    Context(ContextBudgetError),
    Provider(ProviderError),
    Capability(String),
    ToolRepair(ToolRepairError),
    Tool(String),
    Citation(String),
    EventSink(String),
    Internal(String),
}

impl fmt::Display for AgentLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(error) => write!(formatter, "context: {error}"),
            Self::Provider(error) => write!(formatter, "provider: {error}"),
            Self::Capability(message)
            | Self::Tool(message)
            | Self::Citation(message)
            | Self::EventSink(message)
            | Self::Internal(message) => formatter.write_str(message),
            Self::ToolRepair(error) => write!(formatter, "tool repair: {error}"),
        }
    }
}

impl std::error::Error for AgentLoopError {}

pub struct AgentLoop<'a, M: ?Sized, T: ?Sized, P: ?Sized> {
    model: &'a M,
    tools: &'a T,
    permissions: &'a P,
}

#[derive(Debug, Clone)]
struct PreparedToolCall {
    model_call_id: String,
    tool_call_id: Uuid,
    tool_id: String,
    tool_name: String,
    arguments: Value,
    risk: PermissionRisk,
    read_only: bool,
}

struct RepairedToolBatch {
    model_calls: Vec<ChatToolCall>,
    calls: Vec<PreparedToolCall>,
}

impl<'a, M: ?Sized, T: ?Sized, P: ?Sized> AgentLoop<'a, M, T, P>
where
    M: ModelAdapter,
    T: ToolExecutor,
    P: PermissionResolver,
{
    pub fn new(model: &'a M, tools: &'a T, permissions: &'a P) -> Self {
        Self {
            model,
            tools,
            permissions,
        }
    }

    pub async fn run(
        &self,
        request: AgentLoopRequest,
        sink: &mut dyn AgentEventSink,
        cancellation: CancellationToken,
    ) -> Result<AgentLoopResult, AgentLoopError> {
        self.model
            .capabilities()
            .require(ProviderCapability::Chat)
            .map_err(|error| AgentLoopError::Capability(error.to_string()))?;

        let items = request
            .context
            .iter()
            .map(|entry| entry.item.clone())
            .collect::<Vec<_>>();
        let context = match budget_context(
            &items,
            self.model.capabilities().context_window,
            request.output_reservation,
        ) {
            Ok(report) => report,
            Err(error) => {
                return self.fail(
                    sink,
                    AgentRunState::Created,
                    request.assistant_message_id,
                    AgentLoopError::Context(error),
                )
            }
        };

        let mut machine = RunStateMachine::new();
        self.change_state(&mut machine, AgentRunState::Preparing, sink)?;
        if cancellation.is_cancelled() {
            return self.cancel(
                &mut machine,
                sink,
                request.assistant_message_id,
                context,
                String::new(),
            );
        }
        self.change_state(&mut machine, AgentRunState::Generating, sink)?;

        if let Some(evidence) = &request.evidence {
            sink.record(AgentEventData::EvidenceAttached(EvidenceAttached {
                evidence_pack_id: evidence.evidence_pack_id,
                citation_numbers: evidence.citation_numbers.clone(),
            }))
            .map_err(AgentLoopError::EventSink)?;
        }

        let mut messages = render_context_messages(&context, &request.context);
        let mut answer = String::new();
        let mut usage = None;
        let mut tool_round = 0usize;

        loop {
            if cancellation.is_cancelled() {
                return self.cancel(
                    &mut machine,
                    sink,
                    request.assistant_message_id,
                    context,
                    answer,
                );
            }
            let tool_payload = if self.tools.registrations().is_empty() {
                None
            } else {
                if !self.model.capabilities().tool_calls {
                    return self.fail(
                        sink,
                        machine.state(),
                        request.assistant_message_id,
                        AgentLoopError::Capability(
                            "configured model does not support tool calls".to_string(),
                        ),
                    );
                }
                Some(tool_definitions(self.tools.registrations()))
            };
            let chat_request = ChatRequest {
                messages: messages.clone(),
                temperature: 0.2,
                tools: tool_payload.clone(),
                response_format: None,
            };
            let (response, streamed_text) = match self
                .generate(
                    chat_request,
                    request.assistant_message_id,
                    sink,
                    &cancellation,
                )
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    return self.fail(sink, machine.state(), request.assistant_message_id, error)
                }
            };
            answer = append_response_text(
                answer,
                &response,
                &streamed_text,
                sink,
                request.assistant_message_id,
            )?;
            if let Some(current) = &response.usage {
                record_usage(sink, current)?;
                usage = Some(add_usage(usage.take(), current));
            }
            if response.cancelled || cancellation.is_cancelled() {
                return self.cancel(
                    &mut machine,
                    sink,
                    request.assistant_message_id,
                    context,
                    answer,
                );
            }
            if response.tool_calls.is_empty() {
                sink.record(AgentEventData::MessageCompleted(MessageCompleted {
                    message_id: request.assistant_message_id,
                    role: AgentMessageRole::Assistant,
                    content: answer.clone(),
                    partial: false,
                }))
                .map_err(AgentLoopError::EventSink)?;
                self.change_state(&mut machine, AgentRunState::Verifying, sink)?;
                if let Err(error) = validate_citations(&answer, request.evidence.as_ref()) {
                    return self.fail(sink, machine.state(), request.assistant_message_id, error);
                }
                self.change_state(&mut machine, AgentRunState::Completing, sink)?;
                self.finish(
                    &mut machine,
                    sink,
                    AgentRunState::Completed,
                    RunOutcome::Completed,
                    Some(request.assistant_message_id),
                )?;
                return Ok(AgentLoopResult {
                    outcome: RunOutcome::Completed,
                    answer,
                    usage,
                    context,
                });
            }

            if tool_round >= MAX_TOOL_ROUNDS {
                return self.fail(
                    sink,
                    machine.state(),
                    request.assistant_message_id,
                    AgentLoopError::Tool("maximum tool rounds exceeded".to_string()),
                );
            }
            tool_round += 1;
            let repaired = match self
                .repair_tool_calls(
                    &messages,
                    &response.tool_calls,
                    tool_payload.as_ref(),
                    request.assistant_message_id,
                    sink,
                    &cancellation,
                )
                .await
            {
                Ok(repaired) => repaired,
                Err(error) => {
                    return self.fail(sink, machine.state(), request.assistant_message_id, error)
                }
            };
            messages.push(ChatMessage::assistant_tool_calls(
                repaired.model_calls.clone(),
            ));
            for call in &repaired.calls {
                sink.record(AgentEventData::ToolRequested(ToolRequested {
                    tool_call_id: call.tool_call_id,
                    tool_id: call.tool_id.clone(),
                    tool_name: call.tool_name.clone(),
                    arguments: call.arguments.clone(),
                }))
                .map_err(AgentLoopError::EventSink)?;
            }
            let pending_permissions = repaired
                .calls
                .iter()
                .filter(|call| call.risk != PermissionRisk::Automatic)
                .count();
            if pending_permissions > 0 {
                self.change_state_with_guards(
                    &mut machine,
                    AgentRunState::AwaitingPermission,
                    RunGuards {
                        unresolved_tool_calls: repaired.calls.len(),
                        unresolved_permissions: pending_permissions,
                        executable_tool_calls: 0,
                    },
                    sink,
                )?;
            }
            let mut approved = Vec::new();
            let mut denied = Vec::new();
            for call in repaired.calls {
                if call.risk == PermissionRisk::Automatic {
                    approved.push(call);
                    continue;
                }
                let permission_id = Uuid::new_v4();
                sink.record(AgentEventData::PermissionRequested(PermissionRequested {
                    permission_id,
                    tool_call_id: call.tool_call_id,
                    risk: call.risk,
                    reason: "This tool is outside the automatic read-only boundary.".to_string(),
                    summary: format!("Run {}", call.tool_name),
                }))
                .map_err(AgentLoopError::EventSink)?;
                let decision = self.permissions.decide(&PermissionRequest {
                    permission_id,
                    tool_call_id: call.tool_call_id,
                    tool_name: call.tool_name.clone(),
                    risk: call.risk,
                    arguments: call.arguments.clone(),
                });
                sink.record(AgentEventData::PermissionResolved(PermissionResolved {
                    permission_id,
                    decision,
                }))
                .map_err(AgentLoopError::EventSink)?;
                if decision == PermissionDecision::Deny {
                    denied.push(call);
                } else {
                    approved.push(call);
                }
            }
            if cancellation.is_cancelled() {
                return self.cancel(
                    &mut machine,
                    sink,
                    request.assistant_message_id,
                    context,
                    answer,
                );
            }
            let mut observations = denied_observations(sink, denied)?;
            if approved.is_empty() {
                if pending_permissions > 0 {
                    self.change_state_with_guards(
                        &mut machine,
                        AgentRunState::Generating,
                        RunGuards::default(),
                        sink,
                    )?;
                }
            } else {
                if pending_permissions == 0 {
                    self.change_state_with_guards(
                        &mut machine,
                        AgentRunState::ExecutingTools,
                        RunGuards {
                            unresolved_tool_calls: approved.len(),
                            unresolved_permissions: 0,
                            executable_tool_calls: approved.len(),
                        },
                        sink,
                    )?;
                } else {
                    self.change_state_with_guards(
                        &mut machine,
                        AgentRunState::ExecutingTools,
                        RunGuards {
                            unresolved_tool_calls: approved.len(),
                            unresolved_permissions: 0,
                            executable_tool_calls: approved.len(),
                        },
                        sink,
                    )?;
                }
                observations.extend(
                    self.execute_tool_batch(&approved, &cancellation, sink)
                        .await?,
                );
                if cancellation.is_cancelled() {
                    return self.cancel(
                        &mut machine,
                        sink,
                        request.assistant_message_id,
                        context,
                        answer,
                    );
                }
                self.change_state(&mut machine, AgentRunState::Generating, sink)?;
            }
            messages.extend(observations);
        }
    }

    async fn generate(
        &self,
        request: ChatRequest,
        message_id: Uuid,
        sink: &mut dyn AgentEventSink,
        cancellation: &CancellationToken,
    ) -> Result<(ChatResponse, String), AgentLoopError> {
        let mut streamed_text = String::new();
        let mut sink_error = None;
        let mut on_event = |event: ChatEvent| match event {
            ChatEvent::TextDelta(delta) => {
                streamed_text.push_str(&delta);
                if sink_error.is_none() {
                    if let Err(error) = sink.record(AgentEventData::MessageDelta(MessageDelta {
                        message_id,
                        role: AgentMessageRole::Assistant,
                        delta,
                    })) {
                        sink_error = Some(error);
                    }
                }
            }
            ChatEvent::Usage(usage) => {
                if sink_error.is_none() {
                    if let Err(error) = sink.record(AgentEventData::UsageUpdated(UsageUpdated {
                        prompt_tokens: usage.prompt_tokens,
                        completion_tokens: usage.completion_tokens,
                        total_tokens: usage.total_tokens,
                    })) {
                        sink_error = Some(error);
                    }
                }
            }
            ChatEvent::ToolCallDelta(_) => {}
        };
        let response = self
            .model
            .generate(request, &mut on_event, &*cancellation.0)
            .await
            .map_err(AgentLoopError::Provider)?;
        drop(on_event);
        if let Some(error) = sink_error {
            return Err(AgentLoopError::EventSink(error));
        }
        Ok((response, streamed_text))
    }

    async fn repair_tool_calls(
        &self,
        messages: &[ChatMessage],
        initial: &[ChatToolCall],
        tool_payload: Option<&Value>,
        message_id: Uuid,
        sink: &mut dyn AgentEventSink,
        cancellation: &CancellationToken,
    ) -> Result<RepairedToolBatch, AgentLoopError> {
        let specs = self
            .tools
            .registrations()
            .iter()
            .map(|registration| registration.spec.clone())
            .collect::<Vec<_>>();
        let mut model_calls = initial.to_vec();
        let mut last_error = None;
        for attempt in 0..=MAX_REPAIR_RETRIES {
            match prepare_tool_calls(&model_calls, &specs, self.tools.registrations()) {
                Ok(calls) => {
                    return Ok(RepairedToolBatch { model_calls, calls });
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt == MAX_REPAIR_RETRIES {
                        break;
                    }
                }
            }
            let feedback = last_error
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "tool call is invalid".to_string());
            let mut repair_messages = messages.to_vec();
            repair_messages.push(ChatMessage::new(
                "system",
                format!(
                    "The previous tool call failed validation: {feedback}. Return corrected tool calls only."
                ),
            ));
            let (response, _) = self
                .generate(
                    ChatRequest {
                        messages: repair_messages,
                        temperature: 0.0,
                        tools: tool_payload.cloned(),
                        response_format: None,
                    },
                    message_id,
                    sink,
                    cancellation,
                )
                .await?;
            if response.cancelled || cancellation.is_cancelled() {
                return Err(AgentLoopError::Provider(ProviderError::cancelled()));
            }
            model_calls = response.tool_calls;
            if model_calls.is_empty() {
                last_error = Some(ToolRepairError::InvalidEnvelope {
                    message: "repair response did not contain a tool call".to_string(),
                });
            }
        }
        Err(AgentLoopError::ToolRepair(
            ToolRepairError::RetryExhausted {
                attempts: MAX_REPAIR_RETRIES + 1,
                last: Box::new(last_error.unwrap_or(ToolRepairError::InvalidEnvelope {
                    message: "tool call repair failed".to_string(),
                })),
            },
        ))
    }

    async fn execute_tool_batch(
        &self,
        calls: &[PreparedToolCall],
        cancellation: &CancellationToken,
        sink: &mut dyn AgentEventSink,
    ) -> Result<Vec<ChatMessage>, AgentLoopError> {
        let mut observations = Vec::with_capacity(calls.len());
        let mut index = 0usize;
        while index < calls.len() {
            if cancellation.is_cancelled() {
                break;
            }
            if calls[index].read_only {
                let start = index;
                while index < calls.len() && calls[index].read_only {
                    sink.record(AgentEventData::ToolStarted(ToolStarted {
                        tool_call_id: calls[index].tool_call_id,
                    }))
                    .map_err(AgentLoopError::EventSink)?;
                    index += 1;
                }
                let futures = calls[start..index].iter().map(|call| {
                    self.tools.execute(
                        ToolInvocation {
                            tool_call_id: call.tool_call_id,
                            tool_id: call.tool_id.clone(),
                            tool_name: call.tool_name.clone(),
                            arguments: call.arguments.clone(),
                        },
                        cancellation.clone(),
                    )
                });
                let results = join_all(futures).await;
                for (call, result) in calls[start..index].iter().zip(results) {
                    observations.push(record_tool_result(sink, call, result)?);
                }
            } else {
                let call = &calls[index];
                sink.record(AgentEventData::ToolStarted(ToolStarted {
                    tool_call_id: call.tool_call_id,
                }))
                .map_err(AgentLoopError::EventSink)?;
                let result = self
                    .tools
                    .execute(
                        ToolInvocation {
                            tool_call_id: call.tool_call_id,
                            tool_id: call.tool_id.clone(),
                            tool_name: call.tool_name.clone(),
                            arguments: call.arguments.clone(),
                        },
                        cancellation.clone(),
                    )
                    .await;
                observations.push(record_tool_result(sink, call, result)?);
                index += 1;
            }
        }
        Ok(observations)
    }

    fn change_state(
        &self,
        machine: &mut RunStateMachine,
        target: AgentRunState,
        sink: &mut dyn AgentEventSink,
    ) -> Result<(), AgentLoopError> {
        self.change_state_with_guards(machine, target, RunGuards::default(), sink)
    }

    fn change_state_with_guards(
        &self,
        machine: &mut RunStateMachine,
        target: AgentRunState,
        guards: RunGuards,
        sink: &mut dyn AgentEventSink,
    ) -> Result<(), AgentLoopError> {
        let changed = machine
            .transition(target, guards)
            .map_err(|error| AgentLoopError::Internal(error.to_string()))?;
        sink.transition(changed)
            .map_err(AgentLoopError::EventSink)?;
        Ok(())
    }

    fn finish(
        &self,
        machine: &mut RunStateMachine,
        sink: &mut dyn AgentEventSink,
        target: AgentRunState,
        outcome: RunOutcome,
        assistant_message_id: Option<Uuid>,
    ) -> Result<Vec<AgentEventEnvelope>, AgentLoopError> {
        let changed = machine
            .transition(target, RunGuards::default())
            .map_err(|error| AgentLoopError::Internal(error.to_string()))?;
        sink.finish(changed, outcome, assistant_message_id)
            .map_err(AgentLoopError::EventSink)
    }

    fn cancel(
        &self,
        machine: &mut RunStateMachine,
        sink: &mut dyn AgentEventSink,
        assistant_message_id: Uuid,
        context: ContextReport,
        answer: String,
    ) -> Result<AgentLoopResult, AgentLoopError> {
        if !answer.is_empty() {
            sink.record(AgentEventData::MessageCompleted(MessageCompleted {
                message_id: assistant_message_id,
                role: AgentMessageRole::Assistant,
                content: answer.clone(),
                partial: true,
            }))
            .map_err(AgentLoopError::EventSink)?;
        }
        self.finish(
            machine,
            sink,
            AgentRunState::Cancelled,
            RunOutcome::Cancelled,
            Some(assistant_message_id),
        )?;
        Ok(AgentLoopResult {
            outcome: RunOutcome::Cancelled,
            answer,
            usage: None,
            context,
        })
    }

    fn fail<R>(
        &self,
        sink: &mut dyn AgentEventSink,
        state: AgentRunState,
        assistant_message_id: Uuid,
        error: AgentLoopError,
    ) -> Result<R, AgentLoopError> {
        let mut machine = RunStateMachine::restore(state);
        let agent_error = to_agent_error(&error);
        sink.record(AgentEventData::ErrorRaised(ErrorRaised {
            error: agent_error,
            fatal: true,
        }))
        .map_err(AgentLoopError::EventSink)?;
        self.finish(
            &mut machine,
            sink,
            AgentRunState::Failed,
            RunOutcome::Failed,
            Some(assistant_message_id),
        )?;
        Err(error)
    }
}

fn prepare_tool_calls(
    model_calls: &[ChatToolCall],
    specs: &[ToolSpec],
    registrations: &[ToolRegistration],
) -> Result<Vec<PreparedToolCall>, ToolRepairError> {
    let mut prepared = Vec::with_capacity(model_calls.len());
    for model_call in model_calls {
        let raw = serde_json::to_string(&json!({
            "name": model_call.name,
            "arguments": model_call.arguments,
        }))
        .map_err(|error| ToolRepairError::InvalidJson {
            message: error.to_string(),
        })?;
        let repaired = repair_tool_call(&raw, specs)?;
        let registration = registrations
            .iter()
            .find(|registration| registration.spec.id == repaired.tool_id)
            .ok_or_else(|| ToolRepairError::UnknownTool {
                name: repaired.tool_name.clone(),
            })?;
        let model_call_id = if model_call.id.trim().is_empty() {
            format!("tool-{}", Uuid::new_v4())
        } else {
            model_call.id.clone()
        };
        prepared.push(PreparedToolCall {
            model_call_id,
            tool_call_id: Uuid::new_v4(),
            tool_id: repaired.tool_id,
            tool_name: repaired.tool_name,
            arguments: repaired.arguments,
            risk: repaired.risk,
            read_only: registration.read_only,
        });
    }
    Ok(prepared)
}

fn tool_definitions(registrations: &[ToolRegistration]) -> Value {
    Value::Array(
        registrations
            .iter()
            .map(|registration| {
                json!({
                    "type": "function",
                    "function": {
                        "name": registration.spec.name,
                        "description": format!("Bloomery tool {}", registration.spec.name),
                        "parameters": registration.spec.input_schema,
                    }
                })
            })
            .collect(),
    )
}

fn record_usage(sink: &mut dyn AgentEventSink, usage: &ChatUsage) -> Result<(), AgentLoopError> {
    sink.record(AgentEventData::UsageUpdated(UsageUpdated {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
    }))
    .map_err(AgentLoopError::EventSink)?;
    Ok(())
}

fn add_usage(previous: Option<ChatUsage>, current: &ChatUsage) -> ChatUsage {
    let mut total = previous.unwrap_or_default();
    total.prompt_tokens = total.prompt_tokens.saturating_add(current.prompt_tokens);
    total.completion_tokens = total
        .completion_tokens
        .saturating_add(current.completion_tokens);
    total.total_tokens = total.total_tokens.saturating_add(current.total_tokens);
    total
}

fn denied_observations(
    sink: &mut dyn AgentEventSink,
    calls: Vec<PreparedToolCall>,
) -> Result<Vec<ChatMessage>, AgentLoopError> {
    calls
        .into_iter()
        .map(|call| {
            let error = AgentError {
                code: "permission_denied".to_string(),
                category: AgentErrorCategory::ToolPermission,
                message: format!("permission denied for tool {}", call.tool_name),
                retryable: false,
                details: None,
            };
            sink.record(AgentEventData::ToolCompleted(ToolCompleted {
                tool_call_id: call.tool_call_id,
                outcome: ToolOutcome::Failed,
                output: None,
                error: Some(error.clone()),
            }))
            .map_err(AgentLoopError::EventSink)?;
            Ok(ChatMessage::tool_result(
                call.model_call_id,
                format!("tool {} was denied: {}", call.tool_name, error.message),
            ))
        })
        .collect()
}

fn record_tool_result(
    sink: &mut dyn AgentEventSink,
    call: &PreparedToolCall,
    result: Result<Value, ToolExecutionError>,
) -> Result<ChatMessage, AgentLoopError> {
    match result {
        Ok(value) => {
            let (output, observation) = bounded_tool_output(value)?;
            sink.record(AgentEventData::ToolCompleted(ToolCompleted {
                tool_call_id: call.tool_call_id,
                outcome: ToolOutcome::Succeeded,
                output: Some(output),
                error: None,
            }))
            .map_err(AgentLoopError::EventSink)?;
            Ok(ChatMessage::tool_result(
                call.model_call_id.clone(),
                observation,
            ))
        }
        Err(error) => {
            let outcome = if error.cancelled {
                ToolOutcome::Cancelled
            } else {
                ToolOutcome::Failed
            };
            let agent_error = AgentError {
                code: format!("tool_{}", error.code),
                category: AgentErrorCategory::ToolPermission,
                message: error.message.clone(),
                retryable: false,
                details: None,
            };
            sink.record(AgentEventData::ToolCompleted(ToolCompleted {
                tool_call_id: call.tool_call_id,
                outcome,
                output: None,
                error: Some(agent_error),
            }))
            .map_err(AgentLoopError::EventSink)?;
            Ok(ChatMessage::tool_result(
                call.model_call_id.clone(),
                format!("tool {} failed: {}", call.tool_name, error.message),
            ))
        }
    }
}

fn bounded_tool_output(value: Value) -> Result<(Value, String), AgentLoopError> {
    let serialized = serde_json::to_string(&value).map_err(|error| {
        AgentLoopError::Tool(format!("tool output serialization failed: {error}"))
    })?;
    if serialized.len() <= MAX_TOOL_OUTPUT_BYTES {
        return Ok((value, serialized));
    }
    let preview = truncate_utf8(&serialized, MAX_TOOL_OUTPUT_BYTES);
    let bounded = json!({
        "truncated": true,
        "bytes": serialized.len(),
        "preview": preview,
    });
    let observation = serde_json::to_string(&bounded)
        .map_err(|error| AgentLoopError::Tool(format!("bounded tool output failed: {error}")))?;
    Ok((bounded, observation))
}

fn truncate_utf8(value: &str, limit: usize) -> String {
    let mut end = limit.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn append_response_text(
    mut answer: String,
    response: &ChatResponse,
    streamed_text: &str,
    sink: &mut dyn AgentEventSink,
    message_id: Uuid,
) -> Result<String, AgentLoopError> {
    let trailing = response
        .text
        .strip_prefix(streamed_text)
        .unwrap_or(response.text.as_str());
    if !trailing.is_empty() {
        sink.record(AgentEventData::MessageDelta(MessageDelta {
            message_id,
            role: AgentMessageRole::Assistant,
            delta: trailing.to_string(),
        }))
        .map_err(AgentLoopError::EventSink)?;
    }
    answer.push_str(&response.text);
    Ok(answer)
}

fn render_context_messages(report: &ContextReport, entries: &[ContextEntry]) -> Vec<ChatMessage> {
    let selected = report
        .included_items
        .iter()
        .filter_map(|item| entries.iter().find(|entry| entry.item.id == item.id))
        .collect::<Vec<_>>();
    let mut messages = Vec::new();
    for entry in selected.iter().filter(|entry| {
        !matches!(
            entry.item.source,
            ContextSource::RecentTurn { .. } | ContextSource::CurrentRequest
        )
    }) {
        messages.push(ChatMessage::new(
            role_text(entry.role),
            entry.item.content.clone(),
        ));
    }
    let mut recent = selected
        .iter()
        .filter_map(|entry| match entry.item.source {
            ContextSource::RecentTurn { newest_first_rank } => Some((newest_first_rank, *entry)),
            _ => None,
        })
        .collect::<Vec<_>>();
    recent.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, entry) in recent {
        messages.push(ChatMessage::new(
            role_text(entry.role),
            entry.item.content.clone(),
        ));
    }
    if let Some(entry) = selected
        .iter()
        .find(|entry| entry.item.source == ContextSource::CurrentRequest)
    {
        messages.push(ChatMessage::new(
            role_text(entry.role),
            entry.item.content.clone(),
        ));
    }
    messages
}

fn role_text(role: AgentMessageRole) -> &'static str {
    match role {
        AgentMessageRole::User => "user",
        AgentMessageRole::Assistant => "assistant",
        AgentMessageRole::Tool => "tool",
        AgentMessageRole::System => "system",
    }
}

fn validate_citations(
    answer: &str,
    evidence: Option<&EvidenceAttachment>,
) -> Result<(), AgentLoopError> {
    let Some(evidence) = evidence else {
        return Ok(());
    };
    for citation in bracketed_numbers(answer) {
        if !evidence.citation_numbers.contains(&citation) {
            return Err(AgentLoopError::Citation(format!(
                "answer cites unavailable evidence [{citation}]"
            )));
        }
    }
    Ok(())
}

fn bracketed_numbers(text: &str) -> Vec<u32> {
    let mut numbers = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else {
            break;
        };
        let candidate = after[..end].trim();
        if let Ok(number) = candidate.parse::<u32>() {
            numbers.push(number);
        }
        rest = &after[end + 1..];
    }
    numbers
}

fn to_agent_error(error: &AgentLoopError) -> AgentError {
    let (category, code, retryable) = match error {
        AgentLoopError::Context(_error) => (
            AgentErrorCategory::ModelCapability,
            "context_budget_exceeded".to_string(),
            false,
        ),
        AgentLoopError::Provider(error) => (
            match error.code() {
                ProviderErrorCode::Authentication => AgentErrorCategory::Authentication,
                ProviderErrorCode::Quota => AgentErrorCategory::Quota,
                ProviderErrorCode::Network | ProviderErrorCode::Timeout => {
                    AgentErrorCategory::Network
                }
                ProviderErrorCode::UnsupportedCapability => AgentErrorCategory::ModelCapability,
                ProviderErrorCode::Cancelled => AgentErrorCategory::Network,
                ProviderErrorCode::ProviderResponse => AgentErrorCategory::Internal,
            },
            format!("provider_{}", error.code().as_str()),
            matches!(
                error.code(),
                ProviderErrorCode::Network | ProviderErrorCode::Timeout
            ),
        ),
        AgentLoopError::ToolRepair(error) => (
            AgentErrorCategory::ToolPermission,
            error.code().to_string(),
            false,
        ),
        AgentLoopError::Citation(_) => (
            AgentErrorCategory::Indexing,
            "citation_invalid".to_string(),
            false,
        ),
        AgentLoopError::Capability(_) => (
            AgentErrorCategory::ModelCapability,
            "provider_capability_missing".to_string(),
            false,
        ),
        AgentLoopError::Tool(_) => (
            AgentErrorCategory::ToolPermission,
            "tool_failed".to_string(),
            false,
        ),
        AgentLoopError::EventSink(_) | AgentLoopError::Internal(_) => (
            AgentErrorCategory::Internal,
            "agent_runtime_failed".to_string(),
            false,
        ),
    };
    AgentError {
        code,
        category,
        message: error.to_string(),
        retryable,
        details: None,
    }
}
