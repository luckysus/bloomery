use super::types::{AgentEventSink, AgentLoop, AgentLoopError, AgentLoopResult, CancellationToken};
use crate::agent::protocol::AgentEventData;
use crate::agent::protocol::{AgentMessageRole, MessageCompleted};
use crate::providers::capabilities::ChatMessage;
use crate::providers::http::ProviderErrorCode;
use std::time::Duration;

use super::super::model_adapter::ModelAdapter;
use super::super::{PermissionResolver, ToolExecutor};

impl<'a, M: ?Sized, T: ?Sized, P: ?Sized> AgentLoop<'a, M, T, P>
where
    M: ModelAdapter,
    T: ToolExecutor,
    P: PermissionResolver,
{
    pub(super) fn cancel_or_timeout(
        &self,
        cancellation: &CancellationToken,
        machine: &mut super::super::state_machine::RunStateMachine,
        sink: &mut dyn AgentEventSink,
        message_id: uuid::Uuid,
        context: crate::agent::context::ContextReport,
        answer: String,
    ) -> Result<AgentLoopResult, AgentLoopError> {
        if cancellation.deadline_exceeded() {
            if !answer.is_empty() {
                sink.record(AgentEventData::MessageCompleted(MessageCompleted {
                    message_id,
                    role: AgentMessageRole::Assistant,
                    content: answer,
                    partial: true,
                }))
                .map_err(AgentLoopError::EventSink)?;
            }
            self.fail(
                sink,
                machine.state(),
                message_id,
                AgentLoopError::Provider(crate::providers::http::ProviderError::new(
                    ProviderErrorCode::Timeout,
                    None,
                    "Agent total run deadline exceeded",
                )),
            )
        } else {
            self.cancel(machine, sink, message_id, context, answer)
        }
    }

    pub(super) async fn await_background_results(
        &self,
        sink: &mut dyn AgentEventSink,
        cancellation: &CancellationToken,
        wait: bool,
    ) -> Result<Vec<ChatMessage>, AgentLoopError> {
        let mut progress = std::collections::HashMap::new();
        loop {
            if cancellation.is_cancelled() {
                return Err(AgentLoopError::Provider(
                    crate::providers::http::ProviderError::cancelled(),
                ));
            }
            let tasks = sink.background_tasks().map_err(AgentLoopError::EventSink)?;
            for task in &tasks {
                let current = (task.state, task.progress);
                if progress.insert(task.task_id, current) != Some(current) {
                    sink.record(AgentEventData::TaskProgress(
                        crate::agent::protocol::TaskProgress {
                            task_id: task.task_id,
                            kind: task.kind.clone(),
                            state: serde_json::from_value(
                                serde_json::to_value(task.state)
                                    .map_err(|error| AgentLoopError::Internal(error.to_string()))?,
                            )
                            .map_err(|error| AgentLoopError::Internal(error.to_string()))?,
                            progress: task.progress,
                        },
                    ))
                    .map_err(AgentLoopError::EventSink)?;
                }
            }
            let ready = tasks
                .iter()
                .filter(|task| crate::tasks::agent_delivery::is_terminal(task.state))
                .map(|task| ChatMessage::new("user", task.message()))
                .collect::<Vec<_>>();
            if !ready.is_empty() || tasks.is_empty() || !wait {
                return Ok(ready);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}
