use super::helpers::*;
use super::types::MAX_TOOL_ROUNDS;
use super::types::{
    AgentEventSink, AgentLoop, AgentLoopError, AgentLoopRequest, AgentLoopResult,
    CancellationToken, PermissionRequest, PermissionResolver, ToolExecutor, ToolInvocation,
};
use crate::agent::context::budget_context;
use crate::agent::protocol::{
    AgentEventData, AgentMessageRole, AgentRunState, EvidenceAttached, MessageCompleted,
    PermissionDecision, PermissionRequested, PermissionResolved, PermissionRisk, RunOutcome,
    ToolRequested, ToolStarted,
};
use crate::agent::runtime::model_adapter::ModelAdapter;
use crate::agent::runtime::state_machine::{RunGuards, RunStateMachine};
use crate::providers::capabilities::{ChatMessage, ChatRequest};
use crate::providers::profiles::ProviderCapability;
use futures_util::future::join_all;
use uuid::Uuid;

impl<'a, M: ?Sized, T: ?Sized, P: ?Sized> AgentLoop<'a, M, T, P>
where
    M: ModelAdapter,
    T: ToolExecutor,
    P: PermissionResolver,
{
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
        let mut messages =
            render_context_messages(&context, &request.context, &request.attachments);
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
                let decision = self
                    .permissions
                    .decide(
                        PermissionRequest {
                            permission_id,
                            tool_call_id: call.tool_call_id,
                            tool_id: call.tool_id.clone(),
                            tool_name: call.tool_name.clone(),
                            risk: call.risk,
                            arguments: call.arguments.clone(),
                        },
                        cancellation.clone(),
                    )
                    .await;
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

    async fn execute_tool_batch(
        &self,
        calls: &[super::types::PreparedToolCall],
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
    ) -> Result<Vec<crate::agent::protocol::AgentEventEnvelope>, AgentLoopError> {
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
        context: crate::agent::context::ContextReport,
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
        sink.record(AgentEventData::ErrorRaised(
            crate::agent::protocol::ErrorRaised {
                error: to_agent_error(&error),
                fatal: true,
            },
        ))
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
