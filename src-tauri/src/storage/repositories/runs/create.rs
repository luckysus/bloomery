use super::{NewAgentRun, RunWithEvent};
use crate::storage::StorageError;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

pub fn create(
    connection: &mut Connection,
    new_run: NewAgentRun,
) -> Result<RunWithEvent, StorageError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(super::storage)?;
    let created = create_in_transaction(&transaction, new_run)?;
    transaction.commit().map_err(super::storage)?;
    Ok(created)
}

pub fn create_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    new_run: NewAgentRun,
) -> Result<RunWithEvent, StorageError> {
    super::validate_workspace(&new_run.workspace_id)?;
    let message_exists = transaction.query_row("SELECT 1 FROM messages m JOIN conversations c ON c.id = m.conversation_id WHERE m.workspace_id = ?1 AND c.workspace_id = ?1 AND m.conversation_id = ?2 AND m.id = ?3 AND m.role = 'user'", rusqlite::params![new_run.workspace_id, new_run.conversation_id.to_string(), new_run.user_message_id.to_string()], |_| Ok(())).optional().map_err(super::storage)?.is_some();
    if !message_exists {
        return Err(StorageError::new(
            "agent_user_message_not_found",
            "user message does not belong to the requested conversation and workspace",
        ));
    }
    let timestamp = super::timestamp_text(new_run.timestamp);
    transaction.execute("INSERT INTO agent_runs (id, workspace_id, conversation_id, user_message_id, state, next_sequence, created_at, updated_at, completed_at) VALUES (?1, ?2, ?3, ?4, 'created', 1, ?5, ?5, NULL)", rusqlite::params![new_run.id.to_string(), new_run.workspace_id, new_run.conversation_id.to_string(), new_run.user_message_id.to_string(), timestamp]).map_err(super::run_insert)?;
    let event = super::events::append_in_transaction(
        transaction,
        &new_run.workspace_id,
        new_run.id,
        new_run.event_id,
        new_run.timestamp,
        crate::agent::protocol::AgentEventData::RunCreated(crate::agent::protocol::RunCreated {
            state: crate::agent::protocol::AgentRunState::Created,
            user_message_id: new_run.user_message_id,
        }),
    )?;
    let run = super::read(transaction, &new_run.workspace_id, new_run.id)?
        .ok_or_else(|| StorageError::new("agent_run_storage_failed", "created run disappeared"))?;
    Ok(RunWithEvent { run, event })
}
