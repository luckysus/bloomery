use super::{AgentContextCheckpoint, AgentEventSink};
use crate::agent::protocol::{
    AgentEventData, AgentEventEnvelope, CheckpointReason, CheckpointSaved, RunOutcome,
    RunStateChanged,
};
use crate::storage::repositories::{checkpoints, events, runs};
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

pub trait AgentEventPublisher: Send {
    fn publish(&mut self, event: &AgentEventEnvelope) -> Result<(), String>;
}

impl<F> AgentEventPublisher for F
where
    F: FnMut(&AgentEventEnvelope) -> Result<(), String> + Send,
{
    fn publish(&mut self, event: &AgentEventEnvelope) -> Result<(), String> {
        self(event)
    }
}

pub struct NoopAgentEventPublisher;

impl AgentEventPublisher for NoopAgentEventPublisher {
    fn publish(&mut self, _event: &AgentEventEnvelope) -> Result<(), String> {
        Ok(())
    }
}

pub struct SqliteAgentEventSink<'a, P> {
    connection: &'a mut Connection,
    workspace_id: String,
    run_id: Uuid,
    publisher: P,
}

impl<'a, P> SqliteAgentEventSink<'a, P>
where
    P: AgentEventPublisher,
{
    pub fn new(
        connection: &'a mut Connection,
        workspace_id: impl Into<String>,
        run_id: Uuid,
        publisher: P,
    ) -> Self {
        Self {
            connection,
            workspace_id: workspace_id.into(),
            run_id,
            publisher,
        }
    }

    fn publish(&mut self, event: AgentEventEnvelope) -> Result<AgentEventEnvelope, String> {
        self.publisher.publish(&event)?;
        Ok(event)
    }
}

impl<P> AgentEventSink for SqliteAgentEventSink<'_, P>
where
    P: AgentEventPublisher,
{
    fn background_tasks(
        &mut self,
    ) -> Result<Vec<crate::tasks::agent_delivery::TaskDelivery>, String> {
        crate::tasks::agent_delivery::pending_for_run(
            self.connection,
            &self.workspace_id,
            self.run_id,
        )
    }
    fn record(&mut self, data: AgentEventData) -> Result<AgentEventEnvelope, String> {
        let event = events::append(
            self.connection,
            &self.workspace_id,
            self.run_id,
            Uuid::new_v4(),
            Utc::now(),
            data,
        )
        .map_err(|error| error.to_string())?;
        self.publish(event)
    }

    fn transition(&mut self, changed: RunStateChanged) -> Result<AgentEventEnvelope, String> {
        let event = runs::transition(
            self.connection,
            &self.workspace_id,
            self.run_id,
            changed,
            Utc::now(),
        )
        .map_err(|error| error.to_string())?
        .event;
        self.publish(event)
    }

    fn finish(
        &mut self,
        changed: RunStateChanged,
        outcome: RunOutcome,
        assistant_message_id: Option<Uuid>,
    ) -> Result<Vec<AgentEventEnvelope>, String> {
        let events = runs::finish(
            self.connection,
            &self.workspace_id,
            self.run_id,
            changed,
            outcome,
            assistant_message_id,
            Utc::now(),
        )
        .map_err(|error| error.to_string())?;
        for event in &events {
            self.publisher.publish(event)?;
        }
        Ok(events)
    }

    fn checkpoint(&mut self, checkpoint: AgentContextCheckpoint) -> Result<(), String> {
        let reason = match checkpoint.reason {
            crate::agent::runtime::ContextCheckpointReason::ModelCall => {
                CheckpointReason::ModelCall
            }
            crate::agent::runtime::ContextCheckpointReason::AssistantResult => {
                CheckpointReason::AssistantResult
            }
            crate::agent::runtime::ContextCheckpointReason::AssistantError => {
                CheckpointReason::AssistantError
            }
        };
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let received = crate::tasks::agent_delivery::ready_for_run(
            &transaction,
            &self.workspace_id,
            self.run_id,
        )?
        .into_iter()
        .filter(|delivery| {
            checkpoint
                .messages
                .iter()
                .any(|message| message.role == "user" && message.content == delivery.message())
        })
        .map(|delivery| delivery.task_id)
        .collect::<Vec<_>>();
        checkpoints::save(
            &transaction,
            &self.workspace_id,
            self.run_id,
            &checkpoint,
            Utc::now(),
        )
        .map_err(|error| error.to_string())?;
        let event = events::append_in_transaction(
            &transaction,
            &self.workspace_id,
            self.run_id,
            Uuid::new_v4(),
            Utc::now(),
            AgentEventData::CheckpointSaved(CheckpointSaved {
                reason,
                model_call_index: checkpoint.model_call_index,
                model_calls: checkpoint.model_calls,
                tool_calls: checkpoint.tool_calls,
                tool_round: checkpoint.tool_round,
                recovery_attempt: checkpoint.recovery_attempt,
            }),
        )
        .map_err(|error| error.to_string())?;
        crate::tasks::agent_delivery::acknowledge(
            &transaction,
            &self.workspace_id,
            self.run_id,
            &received,
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        self.publish(event).map(|_| ())
    }
}
