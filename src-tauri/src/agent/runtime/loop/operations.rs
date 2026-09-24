use super::helpers::*;
use super::types::{
    AgentEventSink, AgentLoop, AgentLoopError, AgentLoopResult, CancellationToken,
    PermissionResolver, ResumableToolCall, RuntimeToolSnapshot, ToolExecutionError, ToolExecutor,
    ToolFuture, ToolInvocation,
};
use crate::agent::protocol::{
    AgentEventData, AgentMessageRole, AgentRunState, MessageCompleted, PermissionDecision,
    PermissionResolved, RunOutcome, ToolStarted,
};
use crate::agent::runtime::model_adapter::ModelAdapter;
use crate::agent::runtime::state_machine::{RunGuards, RunStateMachine};
use crate::providers::capabilities::{ChatMessage, ChatRequest, ChatToolCall};
use crate::providers::http::ProviderErrorCode;
use futures_util::future::join_all;
use std::time::{Duration, Instant};
use uuid::Uuid;

impl<'a, M: ?Sized, T: ?Sized, P: ?Sized> AgentLoop<'a, M, T, P>
where
    M: ModelAdapter,
    T: ToolExecutor,
    P: PermissionResolver,
{
    pub(super) async fn resume_tool_batch(
        &self,
        pending: &[ResumableToolCall],
        snapshot: &RuntimeToolSnapshot,
        registrations: &[super::types::ToolRegistration],
        machine: &mut RunStateMachine,
        sink: &mut dyn AgentEventSink,
        cancellation: &CancellationToken,
    ) -> Result<(Vec<ChatToolCall>, Vec<ChatMessage>), AgentLoopError> {
        if pending.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let model_calls = pending
            .iter()
            .map(|call| {
                Ok(ChatToolCall {
                    id: call.tool_call_id.to_string(),
                    name: call.tool_name.clone(),
                    arguments: serde_json::to_string(&call.arguments)
                        .map_err(|error| AgentLoopError::Internal(error.to_string()))?,
                })
            })
            .collect::<Result<Vec<_>, AgentLoopError>>()?;
        let specs = registrations
            .iter()
            .map(|registration| registration.spec.clone())
            .collect::<Vec<_>>();
        let mut prepared = prepare_tool_calls(&model_calls, &specs, registrations)
            .map_err(AgentLoopError::ToolRepair)?;
        if prepared.len() != pending.len() {
            return Err(AgentLoopError::Internal(
                "resumed tool batch changed its call count".to_string(),
            ));
        }
        for (call, persisted) in prepared.iter_mut().zip(pending) {
            call.tool_call_id = persisted.tool_call_id;
        }

        let mut hook_blocked = Vec::new();
        for call in &mut prepared {
            let invocation = ToolInvocation {
                tool_call_id: call.tool_call_id,
                tool_id: call.tool_id.clone(),
                tool_name: call.tool_name.clone(),
                arguments: call.arguments.clone(),
            };
            match self.hooks.pre_tool_use(&invocation) {
                Ok(super::types::HookDecision::Continue) => {}
                Ok(super::types::HookDecision::Replace(arguments)) => {
                    let validation = registrations
                        .iter()
                        .find(|registration| registration.spec.id == call.tool_id)
                        .ok_or_else(|| "resumed tool is not registered".to_string())
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
                Err(error) => {
                    hook_blocked.push((call.tool_call_id, format!("pre-tool hook failed: {error}")))
                }
            }
        }

        let mut approved = Vec::new();
        let mut denied = Vec::new();
        for (call, persisted) in prepared.into_iter().zip(pending) {
            if let Some((_, message)) = hook_blocked.iter().find(|(id, _)| *id == call.tool_call_id)
            {
                denied.push((call, Some(message.clone())));
                continue;
            }
            if call.risk == crate::agent::protocol::PermissionRisk::Automatic {
                approved.push(call);
                continue;
            }
            let decision = persisted.decision.ok_or_else(|| {
                AgentLoopError::Internal(
                    "resumed permission is missing its user decision".to_string(),
                )
            })?;
            sink.record(AgentEventData::PermissionResolved(PermissionResolved {
                permission_id: persisted.permission_id.ok_or_else(|| {
                    AgentLoopError::Internal("resumed permission ID is missing".to_string())
                })?,
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
            return Err(AgentLoopError::Provider(
                crate::providers::http::ProviderError::cancelled(),
            ));
        }
        let mut observations = denied_observations(sink, denied)?;
        self.hooks.after_tool_round(
            &pending
                .iter()
                .map(|call| call.tool_name.clone())
                .collect::<Vec<_>>(),
        );
        if approved.is_empty() {
            if machine.state() == AgentRunState::AwaitingPermission {
                self.change_state_with_guards(
                    machine,
                    AgentRunState::Generating,
                    RunGuards::default(),
                    sink,
                )?;
            }
        } else {
            if machine.state() == AgentRunState::AwaitingPermission {
                self.change_state_with_guards(
                    machine,
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
                self.execute_tool_batch(&approved, snapshot, cancellation, sink)
                    .await?,
            );
            if cancellation.is_cancelled() {
                return Err(AgentLoopError::Provider(
                    crate::providers::http::ProviderError::cancelled(),
                ));
            }
            self.change_state(machine, AgentRunState::Generating, sink)?;
        }
        Ok((model_calls, observations))
    }

    pub(super) async fn generate_with_recovery(
        &self,
        chat_request: ChatRequest,
        message_id: Uuid,
        sink: &mut dyn AgentEventSink,
        cancellation: &CancellationToken,
    ) -> Result<(crate::providers::capabilities::ChatResponse, String, u64), AgentLoopError> {
        let mut retried = false;
        loop {
            match self
                .generate(chat_request.clone(), message_id, sink, cancellation)
                .await
            {
                Ok(result) => return Ok(result),
                Err(AgentLoopError::Provider(error))
                    if !retried
                        && matches!(
                            error.code(),
                            ProviderErrorCode::Network | ProviderErrorCode::Timeout
                        )
                        && !cancellation.is_cancelled() =>
                {
                    retried = true;
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) async fn execute_tool_batch(
        &self,
        calls: &[super::types::PreparedToolCall],
        snapshot: &RuntimeToolSnapshot,
        cancellation: &CancellationToken,
        sink: &mut dyn AgentEventSink,
    ) -> Result<Vec<ChatMessage>, AgentLoopError> {
        let mut observations = Vec::with_capacity(calls.len());
        let mut index = 0usize;
        while index < calls.len() {
            if cancellation.is_cancelled() {
                break;
            }
            if calls[index].concurrency == crate::tools::ConcurrencyPolicy::ParallelRead {
                let start = index;
                while index < calls.len()
                    && calls[index].concurrency == crate::tools::ConcurrencyPolicy::ParallelRead
                {
                    sink.record(AgentEventData::ToolStarted(ToolStarted {
                        tool_call_id: calls[index].tool_call_id,
                    }))
                    .map_err(AgentLoopError::EventSink)?;
                    index += 1;
                }
                let futures = calls[start..index]
                    .iter()
                    .map(|call| self.execute_tool(call, snapshot, cancellation.clone()));
                let results = join_all(futures).await;
                for (call, result) in calls[start..index].iter().zip(results) {
                    observations.push(record_tool_result(
                        sink,
                        call,
                        self.artifact_store,
                        self.apply_post_hook(call, result),
                    )?);
                }
            } else {
                let call = &calls[index];
                sink.record(AgentEventData::ToolStarted(ToolStarted {
                    tool_call_id: call.tool_call_id,
                }))
                .map_err(AgentLoopError::EventSink)?;
                let result = self
                    .execute_tool(call, snapshot, cancellation.clone())
                    .await;
                observations.push(record_tool_result(
                    sink,
                    call,
                    self.artifact_store,
                    self.apply_post_hook(call, result),
                )?);
                index += 1;
            }
        }
        Ok(observations)
    }

    fn execute_tool(
        &self,
        call: &super::types::PreparedToolCall,
        snapshot: &RuntimeToolSnapshot,
        cancellation: CancellationToken,
    ) -> ToolFuture {
        let Some(registration) = snapshot.registrations().iter().find(|registration| {
            registration.spec.id == call.tool_id && registration.spec.name == call.tool_name
        }) else {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_snapshot_mismatch",
                    "tool is not present in the immutable turn snapshot",
                ))
            });
        };
        let timeout = call.timeout;
        let future = registration
            .handler
            .execute(call.arguments.clone(), cancellation);
        Box::pin(async move {
            tokio::time::timeout(timeout, future).await.map_err(|_| {
                ToolExecutionError::new("timeout", "tool execution exceeded its timeout")
            })?
        })
    }

    pub(super) fn apply_post_hook(
        &self,
        call: &super::types::PreparedToolCall,
        result: Result<serde_json::Value, ToolExecutionError>,
    ) -> Result<serde_json::Value, ToolExecutionError> {
        let invocation = ToolInvocation {
            tool_call_id: call.tool_call_id,
            tool_id: call.tool_id.clone(),
            tool_name: call.tool_name.clone(),
            arguments: call.arguments.clone(),
        };
        match self.hooks.post_tool_use(&invocation, &result) {
            Ok(super::types::HookDecision::Continue) => result,
            Ok(super::types::HookDecision::Replace(output)) => Ok(output),
            Ok(super::types::HookDecision::Block(message)) => {
                Err(ToolExecutionError::new("hook_blocked", message))
            }
            Err(error) => Err(ToolExecutionError::new(
                "hook_execution",
                format!("post-tool hook failed: {error}"),
            )),
        }
    }

    pub(super) fn change_state(
        &self,
        machine: &mut RunStateMachine,
        target: AgentRunState,
        sink: &mut dyn AgentEventSink,
    ) -> Result<(), AgentLoopError> {
        self.change_state_with_guards(machine, target, RunGuards::default(), sink)
    }

    pub(super) fn change_state_with_guards(
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

    pub(super) fn save_checkpoint(
        &self,
        sink: &mut dyn AgentEventSink,
        checkpoint: super::types::AgentContextCheckpoint,
        timeout_ms: u64,
    ) -> Result<(), AgentLoopError> {
        let started = Instant::now();
        sink.checkpoint(checkpoint)
            .map_err(AgentLoopError::EventSink)?;
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        if elapsed_ms > timeout_ms {
            return Err(AgentLoopError::CheckpointTimeout {
                limit_ms: timeout_ms,
                elapsed_ms,
            });
        }
        Ok(())
    }

    pub(super) fn finish(
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

    pub(super) fn cancel(
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
            reasoning: String::new(),
            reasoning_ms: 0,
            usage: None,
            context,
        })
    }

    pub(super) fn fail<R>(
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
