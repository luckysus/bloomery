use super::{
    is_terminal, outcome_state, read, state_text, storage, timestamp_text, validate_workspace,
    RunWithEvent,
};
use crate::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentRunState, RunCompleted, RunOutcome, RunStateChanged,
};
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, TransactionBehavior};
use uuid::Uuid;

pub fn transition(
    connection: &mut Connection,
    workspace_id: &str,
    run_id: Uuid,
    changed: RunStateChanged,
    timestamp: DateTime<Utc>,
) -> Result<RunWithEvent, StorageError> {
    validate_workspace(workspace_id)?;
    if is_terminal(changed.previous) {
        return Err(StorageError::new(
            "agent_run_terminal_transition_rejected",
            "terminal runs cannot transition again",
        ));
    }
    if is_terminal(changed.current) {
        return Err(StorageError::new(
            "agent_run_terminal_transition_requires_finish",
            "terminal state changes must be persisted with run completion",
        ));
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let current = read(&transaction, workspace_id, run_id)?
        .ok_or_else(|| StorageError::new("agent_run_not_found", "agent run not found"))?;
    if current.state != changed.previous {
        return Err(StorageError::new(
            "agent_run_state_conflict",
            format!("expected {:?}, found {:?}", changed.previous, current.state),
        ));
    }
    let changed_rows = transaction.execute("UPDATE agent_runs SET state = ?1, updated_at = ?2 WHERE workspace_id = ?3 AND id = ?4 AND state = ?5", params![state_text(changed.current), timestamp_text(timestamp), workspace_id, run_id.to_string(), state_text(changed.previous)]).map_err(storage)?;
    if changed_rows != 1 {
        return Err(StorageError::new(
            "agent_run_state_conflict",
            "agent run state changed before transition",
        ));
    }
    let event = super::events::append_in_transaction(
        &transaction,
        workspace_id,
        run_id,
        Uuid::new_v4(),
        timestamp,
        AgentEventData::RunStateChanged(changed),
    )?;
    let run = read(&transaction, workspace_id, run_id)?
        .ok_or_else(|| StorageError::new("agent_run_storage_failed", "run disappeared"))?;
    transaction.commit().map_err(storage)?;
    Ok(RunWithEvent { run, event })
}

pub fn finish(
    connection: &mut Connection,
    workspace_id: &str,
    run_id: Uuid,
    changed: RunStateChanged,
    outcome: RunOutcome,
    assistant_message_id: Option<Uuid>,
    timestamp: DateTime<Utc>,
) -> Result<Vec<AgentEventEnvelope>, StorageError> {
    validate_workspace(workspace_id)?;
    if is_terminal(changed.previous) {
        return Err(StorageError::new(
            "agent_run_terminal_completion_rejected",
            "terminal run cannot be completed again",
        ));
    }
    if !is_terminal(changed.current) || outcome_state(outcome) != changed.current {
        return Err(StorageError::new(
            "agent_run_completion_mismatch",
            "terminal state and run outcome do not match",
        ));
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let current = read(&transaction, workspace_id, run_id)?
        .ok_or_else(|| StorageError::new("agent_run_not_found", "agent run not found"))?;
    if current.state != changed.previous {
        return Err(StorageError::new(
            "agent_run_state_conflict",
            format!("expected {:?}, found {:?}", changed.previous, current.state),
        ));
    }
    let changed_rows = transaction.execute("UPDATE agent_runs SET state = ?1, updated_at = ?2, completed_at = ?2 WHERE workspace_id = ?3 AND id = ?4 AND state = ?5", params![state_text(changed.current), timestamp_text(timestamp), workspace_id, run_id.to_string(), state_text(changed.previous)]).map_err(storage)?;
    if changed_rows != 1 {
        return Err(StorageError::new(
            "agent_run_state_conflict",
            "agent run state changed before completion",
        ));
    }
    let state_event = super::events::append_in_transaction(
        &transaction,
        workspace_id,
        run_id,
        Uuid::new_v4(),
        timestamp,
        AgentEventData::RunStateChanged(changed),
    )?;
    let completed_event = super::events::append_in_transaction(
        &transaction,
        workspace_id,
        run_id,
        Uuid::new_v4(),
        timestamp,
        AgentEventData::RunCompleted(RunCompleted {
            outcome,
            assistant_message_id,
        }),
    )?;
    transaction.commit().map_err(storage)?;
    Ok(vec![state_event, completed_event])
}

pub fn complete(
    connection: &mut Connection,
    workspace_id: &str,
    run_id: Uuid,
    event_id: Uuid,
    timestamp: DateTime<Utc>,
    outcome: RunOutcome,
    assistant_message_id: Option<Uuid>,
) -> Result<RunWithEvent, StorageError> {
    validate_workspace(workspace_id)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let current = read(&transaction, workspace_id, run_id)?
        .ok_or_else(|| StorageError::new("agent_run_not_found", "agent run not found"))?;
    if current.state != AgentRunState::Completing {
        return Err(StorageError::new(
            "agent_run_not_completing",
            "agent run must be completing before it becomes terminal",
        ));
    }
    let target = outcome_state(outcome);
    let changed = transaction.execute("UPDATE agent_runs SET state = ?1, updated_at = ?2, completed_at = ?2 WHERE workspace_id = ?3 AND id = ?4 AND state = 'completing'", params![state_text(target), timestamp_text(timestamp), workspace_id, run_id.to_string()]).map_err(storage)?;
    if changed != 1 {
        return Err(StorageError::new(
            "agent_run_not_completing",
            "agent run state changed before completion",
        ));
    }
    let event = super::events::append_in_transaction(
        &transaction,
        workspace_id,
        run_id,
        event_id,
        timestamp,
        AgentEventData::RunCompleted(RunCompleted {
            outcome,
            assistant_message_id,
        }),
    )?;
    let run = read(&transaction, workspace_id, run_id)?.ok_or_else(|| {
        StorageError::new("agent_run_storage_failed", "completed run disappeared")
    })?;
    transaction.commit().map_err(storage)?;
    Ok(RunWithEvent { run, event })
}
