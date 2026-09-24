use super::helpers::*;
use super::types::{
    AgentContextCheckpoint, AgentEventSink, AgentInputKind, AgentLoop, AgentLoopError,
    AgentLoopRequest, AgentLoopResult, CancellationToken, ContextCheckpointReason,
    PermissionRequest, PermissionResolver, ToolExecutor, ToolInvocation,
};
use crate::agent::context::budget_context;
use crate::agent::protocol::{
    AgentEventData, AgentMessageRole, AgentRunState, EvidenceAttached, MessageCompleted,
    PermissionDecision, PermissionRequested, PermissionResolved, PermissionRisk, RunOutcome,
    ToolRequested,
};
use crate::agent::runtime::model_adapter::ModelAdapter;
use crate::agent::runtime::state_machine::{RunGuards, RunStateMachine};
use crate::providers::capabilities::{ChatMessage, ChatRequest, ChatToolCall};
use crate::providers::profiles::ProviderCapability;
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
        request
            .limits
            .validate()
            .map_err(AgentLoopError::Internal)?;
        let limits = request.limits.clone();
        let mut resume = request.resume.clone();
        if let Some(resume_state) = resume.as_mut() {
            if matches!(
                resume_state.state,
                AgentRunState::Completed
                    | AgentRunState::Cancelled
                    | AgentRunState::Failed
                    | AgentRunState::Interrupted
            ) {
                return Err(AgentLoopError::Internal(
                    "cannot resume an agent loop from a terminal state".to_string(),
                ));
            }
            resume_state.checkpoint.recovery_attempt =
                resume_state.checkpoint.recovery_attempt.saturating_add(1);
            if resume_state.checkpoint.recovery_attempt > limits.max_recovery_attempts {
                return Err(AgentLoopError::Internal(
                    "agent recovery attempt limit exceeded".to_string(),
                ));
            }
        }
        let recovery_attempt = resume
            .as_ref()
            .map(|resume| resume.checkpoint.recovery_attempt)
            .unwrap_or_default();
        let cancellation = limits
            .deadline_ms
            .map(|deadline| cancellation.with_deadline(std::time::Duration::from_millis(deadline)))
            .unwrap_or(cancellation);
        let model_capabilities = self.model.capabilities().clone();
        model_capabilities
            .require(ProviderCapability::Chat)
            .map_err(|error| AgentLoopError::Capability(error.to_string()))?;
        let items = request
            .context
            .iter()
            .map(|entry| entry.item.clone())
            .collect::<Vec<_>>();
        let context = match budget_context(
            &items,
            model_capabilities.context_window,
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
        let mut machine = resume
            .as_ref()
            .map(|resume| RunStateMachine::restore(resume.state))
            .unwrap_or_default();
        if resume.is_none() {
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
        } else if machine.state() == AgentRunState::Preparing {
            self.change_state(&mut machine, AgentRunState::Generating, sink)?;
        }
        let mut messages = resume
            .as_ref()
            .map(|resume| resume.checkpoint.messages.clone())
            .unwrap_or_else(|| {
                render_context_messages(&context, &request.context, &request.attachments)
            });
        let mut resume_natural_stop = resume.as_ref().is_some_and(|resume| {
            resume.checkpoint.reason == ContextCheckpointReason::AssistantResult
        });
        let mut answer = if resume_natural_stop {
            resume
                .as_ref()
                .and_then(|resume| {
                    resume
                        .checkpoint
                        .messages
                        .iter()
                        .rev()
                        .find(|message| message.role == "assistant")
                })
                .map(|message| message.content.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let mut reasoning = if resume_natural_stop {
            resume
                .as_ref()
                .and_then(|resume| {
                    resume
                        .checkpoint
                        .messages
                        .iter()
                        .rev()
                        .find(|message| message.role == "assistant")
                })
                .and_then(|message| message.reasoning_content.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let mut reasoning_ms: u64 = 0;
        let mut usage = None;
        let mut tool_round = resume
            .as_ref()
            .map(|resume| resume.checkpoint.tool_round)
            .unwrap_or_default();
        let mut model_calls = resume
            .as_ref()
            .map(|resume| resume.checkpoint.model_calls)
            .unwrap_or_default();
        let mut tool_calls = resume
            .as_ref()
            .map(|resume| resume.checkpoint.tool_calls)
            .unwrap_or_default();
        let tool_snapshot = self.tools.snapshot().map_err(AgentLoopError::Internal)?;
        let tool_registrations = tool_snapshot.registrations().to_vec();
        let tool_payload = if tool_registrations.is_empty() {
            None
        } else {
            if !model_capabilities.tool_calls {
                return self.fail(
                    sink,
                    machine.state(),
                    request.assistant_message_id,
                    AgentLoopError::Capability(
                        "configured model does not support tool calls".to_string(),
                    ),
                );
            }
            Some(tool_definitions(&tool_registrations))
        };
        let mut resumable_tools = resume
            .as_ref()
            .map(|resume| resume.pending_tools.clone())
            .unwrap_or_default();
        loop {
            if resume_natural_stop {
                resume_natural_stop = false;
                let assistant_result_recorded = resume
                    .as_ref()
                    .map(|resume| resume.assistant_result_recorded)
                    .unwrap_or(false);
                if !assistant_result_recorded {
                    sink.record(AgentEventData::MessageCompleted(MessageCompleted {
                        message_id: request.assistant_message_id,
                        role: AgentMessageRole::Assistant,
                        content: answer.clone(),
                        partial: false,
                    }))
                    .map_err(AgentLoopError::EventSink)?;
                }
                if machine.state() == AgentRunState::Generating {
                    self.change_state(&mut machine, AgentRunState::Verifying, sink)?;
                }
                if machine.state() == AgentRunState::Verifying {
                    if let Err(error) = validate_citations(&answer, request.evidence.as_ref()) {
                        return self.fail(
                            sink,
                            machine.state(),
                            request.assistant_message_id,
                            error,
                        );
                    }
                    let queued = request
                        .input_queue
                        .take(AgentInputKind::Steering)
                        .map_err(AgentLoopError::Internal)?;
                    let queued = if queued.is_empty() {
                        request
                            .input_queue
                            .take(AgentInputKind::FollowUp)
                            .map_err(AgentLoopError::Internal)?
                    } else {
                        queued
                    };
                    if !queued.is_empty() {
                        append_input_messages(&mut messages, queued);
                        self.change_state(&mut machine, AgentRunState::Generating, sink)?;
                        continue;
                    }
                    self.change_state(&mut machine, AgentRunState::Completing, sink)?;
                }
                if machine.state() == AgentRunState::Completing {
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
                        reasoning,
                        reasoning_ms,
                        usage,
                        context,
                    });
                }
            }
            if !resumable_tools.is_empty() {
                if let Some(max_tool_rounds) = limits.max_tool_rounds {
                    if tool_round >= max_tool_rounds {
                        return self.fail(
                            sink,
                            machine.state(),
                            request.assistant_message_id,
                            AgentLoopError::Limit {
                                kind: "tool_rounds",
                                limit: max_tool_rounds,
                                observed: tool_round + 1,
                            },
                        );
                    }
                }
                let observed_tool_calls = tool_calls.saturating_add(resumable_tools.len());
                if let Some(limit) = limits.max_tool_calls {
                    if observed_tool_calls > limit {
                        return self.fail(
                            sink,
                            machine.state(),
                            request.assistant_message_id,
                            AgentLoopError::Limit {
                                kind: "tool_calls",
                                limit,
                                observed: observed_tool_calls,
                            },
                        );
                    }
                }
                tool_round += 1;
                tool_calls = observed_tool_calls;
                let pending = std::mem::take(&mut resumable_tools);
                let model_calls = pending
                    .iter()
                    .map(|call| ChatToolCall {
                        id: call.tool_call_id.to_string(),
                        name: call.tool_name.clone(),
                        arguments: serde_json::to_string(&call.arguments).unwrap_or_default(),
                    })
                    .collect::<Vec<_>>();
                messages.push(ChatMessage::assistant_tool_calls_with_reasoning(
                    model_calls,
                    String::new(),
                ));
                let observations = match self
                    .resume_tool_batch(
                        &pending,
                        &tool_snapshot,
                        &tool_registrations,
                        &mut machine,
                        sink,
                        &cancellation,
                    )
                    .await
                {
                    Ok((_, observations)) => observations,
                    Err(error) => {
                        if cancellation.is_cancelled() {
                            return self.cancel(
                                &mut machine,
                                sink,
                                request.assistant_message_id,
                                context,
                                answer,
                            );
                        }
                        return self.fail(
                            sink,
                            machine.state(),
                            request.assistant_message_id,
                            error,
                        );
                    }
                };
                messages.extend(observations);
                let queued = request
                    .input_queue
                    .take(AgentInputKind::Steering)
                    .map_err(AgentLoopError::Internal)?;
                append_input_messages(&mut messages, queued);
                continue;
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
            if let Some(limit) = limits.max_model_calls {
                if model_calls >= limit {
                    return self.fail(
                        sink,
                        machine.state(),
                        request.assistant_message_id,
                        AgentLoopError::Limit {
                            kind: "model_calls",
                            limit,
                            observed: model_calls + 1,
                        },
                    );
                }
            }
            model_calls += 1;
            let mut request_messages = messages.clone();
            if let Some(context) = self.hooks.before_model() {
                request_messages.push(context);
            }
            let request_messages = budget_chat_messages(
                request_messages,
                model_capabilities.context_window,
                request.output_reservation,
                tool_payload.as_ref(),
            );
            sink.checkpoint(AgentContextCheckpoint {
                reason: ContextCheckpointReason::ModelCall,
                model_call_index: model_calls - 1,
                model_calls,
                tool_calls,
                tool_round,
                recovery_attempt,
                messages: checkpoint_messages(&request_messages),
            })
            .map_err(AgentLoopError::EventSink)?;
            let chat_request = ChatRequest {
                messages: request_messages.clone(),
                temperature: 0.2,
                tools: tool_payload.clone(),
                response_format: None,
                reasoning_effort: None,
                max_tokens: None,
                stop: None,
            };
            let (response, streamed_text, current_reasoning_ms) = match self
                .generate_with_recovery(
                    chat_request,
                    request.assistant_message_id,
                    sink,
                    &cancellation,
                )
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    sink.checkpoint(AgentContextCheckpoint {
                        reason: ContextCheckpointReason::AssistantError,
                        model_call_index: model_calls - 1,
                        model_calls,
                        tool_calls,
                        tool_round,
                        recovery_attempt,
                        messages: checkpoint_messages(&request_messages),
                    })
                    .map_err(AgentLoopError::EventSink)?;
                    if cancellation.is_cancelled() {
                        return self.cancel(
                            &mut machine,
                            sink,
                            request.assistant_message_id,
                            context,
                            answer,
                        );
                    }
                    return self.fail(sink, machine.state(), request.assistant_message_id, error);
                }
            };
            let mut assistant_checkpoint = request_messages.clone();
            assistant_checkpoint.push(crate::providers::capabilities::ChatMessage {
                role: "assistant".to_string(),
                content: response.text.clone(),
                images: Vec::new(),
                reasoning_content: (!response.reasoning.is_empty())
                    .then_some(response.reasoning.clone()),
                tool_call_id: None,
                tool_calls: response.tool_calls.clone(),
            });
            if !response.reasoning.is_empty() {
                if !reasoning.is_empty() {
                    reasoning.push_str("\n\n");
                }
                reasoning.push_str(&response.reasoning);
            }
            reasoning_ms = reasoning_ms.saturating_add(current_reasoning_ms);
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
                // Match Vetta's checkpoint contract: assistant_result is a
                // resumable natural stop, not the intermediate assistant
                // message that still has pending tool calls.
                sink.checkpoint(AgentContextCheckpoint {
                    reason: ContextCheckpointReason::AssistantResult,
                    model_call_index: model_calls - 1,
                    model_calls,
                    tool_calls,
                    tool_round,
                    recovery_attempt,
                    messages: checkpoint_messages(&assistant_checkpoint),
                })
                .map_err(AgentLoopError::EventSink)?;
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
                let queued = request
                    .input_queue
                    .take(AgentInputKind::Steering)
                    .map_err(AgentLoopError::Internal)?;
                let queued = if queued.is_empty() {
                    request
                        .input_queue
                        .take(AgentInputKind::FollowUp)
                        .map_err(AgentLoopError::Internal)?
                } else {
                    queued
                };
                if !queued.is_empty() {
                    append_input_messages(&mut messages, queued);
                    self.change_state(&mut machine, AgentRunState::Generating, sink)?;
                    continue;
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
                    reasoning,
                    reasoning_ms,
                    usage,
                    context,
                });
            }
            if let Some(max_tool_rounds) = limits.max_tool_rounds {
                if tool_round >= max_tool_rounds {
                    return self.fail(
                        sink,
                        machine.state(),
                        request.assistant_message_id,
                        AgentLoopError::Limit {
                            kind: "tool_rounds",
                            limit: max_tool_rounds,
                            observed: tool_round + 1,
                        },
                    );
                }
            }
            tool_round += 1;
            let mut repaired = match self
                .repair_tool_calls(
                    &messages,
                    &response.tool_calls,
                    tool_payload.as_ref(),
                    request.assistant_message_id,
                    sink,
                    &cancellation,
                    &tool_registrations,
                    &mut model_calls,
                    limits.max_model_calls,
                )
                .await
            {
                Ok(repaired) => repaired,
                Err(error) => {
                    if cancellation.is_cancelled() {
                        return self.cancel(
                            &mut machine,
                            sink,
                            request.assistant_message_id,
                            context,
                            answer,
                        );
                    }
                    return self.fail(sink, machine.state(), request.assistant_message_id, error);
                }
            };
            let observed_tool_calls = tool_calls.saturating_add(repaired.calls.len());
            if let Some(limit) = limits.max_tool_calls {
                if observed_tool_calls > limit {
                    return self.fail(
                        sink,
                        machine.state(),
                        request.assistant_message_id,
                        AgentLoopError::Limit {
                            kind: "tool_calls",
                            limit,
                            observed: observed_tool_calls,
                        },
                    );
                }
            }
            tool_calls = observed_tool_calls;
            let mut hook_blocked = Vec::new();
            for call in &mut repaired.calls {
                let invocation = ToolInvocation {
                    tool_call_id: call.tool_call_id,
                    tool_id: call.tool_id.clone(),
                    tool_name: call.tool_name.clone(),
                    arguments: call.arguments.clone(),
                };
                match self.hooks.pre_tool_use(&invocation) {
                    Ok(super::types::HookDecision::Continue) => {}
                    Ok(super::types::HookDecision::Replace(arguments)) => {
                        let validation = tool_registrations
                            .iter()
                            .find(|registration| registration.spec.id == call.tool_id)
                            .ok_or_else(|| "hook tool is not registered".to_string())
                            .and_then(|registration| {
                                registration
                                    .spec
                                    .validate_arguments(&arguments)
                                    .map_err(|error| format!("invalid hook replacement: {error}"))
                            });
                        match validation {
                            Ok(()) => call.arguments = arguments,
                            Err(error) => hook_blocked.push((call.tool_call_id, error)),
                        }
                    }
                    Ok(super::types::HookDecision::Block(message)) => {
                        hook_blocked.push((call.tool_call_id, message));
                    }
                    Err(error) => hook_blocked
                        .push((call.tool_call_id, format!("pre-tool hook failed: {error}"))),
                }
            }
            messages.push(ChatMessage::assistant_tool_calls_with_reasoning(
                repaired.model_calls.clone(),
                response.reasoning.clone(),
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
            let tool_names = repaired
                .calls
                .iter()
                .map(|call| call.tool_name.clone())
                .collect::<Vec<_>>();
            for call in repaired.calls {
                if let Some((_, message)) =
                    hook_blocked.iter().find(|(id, _)| *id == call.tool_call_id)
                {
                    denied.push((call, Some(message.clone())));
                    continue;
                }
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
                    denied.push((call, None));
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
            self.hooks.after_tool_round(&tool_names);
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
                    self.execute_tool_batch(&approved, &tool_snapshot, &cancellation, sink)
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
            let queued = request
                .input_queue
                .take(AgentInputKind::Steering)
                .map_err(AgentLoopError::Internal)?;
            append_input_messages(&mut messages, queued);
        }
    }
}
