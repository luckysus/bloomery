use super::helpers::*;
use super::types::{
    AgentEventSink, AgentLoop, AgentLoopError, AgentLoopResult, CancellationToken,
    PermissionResolver, ToolExecutionError, ToolExecutor, ToolInvocation,
};
use crate::agent::protocol::{
    AgentEventData, AgentMessageRole, AgentRunState, MessageCompleted, RunOutcome, ToolStarted,
};
use crate::agent::runtime::model_adapter::ModelAdapter;
use crate::agent::runtime::state_machine::{RunGuards, RunStateMachine};
use crate::providers::capabilities::{ChatMessage, ChatRequest};
use crate::providers::http::ProviderErrorCode;
use futures_util::future::join_all;
use std::time::Duration;
use uuid::Uuid;

impl<'a, M: ?Sized, T: ?Sized, P: ?Sized> AgentLoop<'a, M, T, P>
where
    M: ModelAdapter,
    T: ToolExecutor,
    P: PermissionResolver,
{
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
