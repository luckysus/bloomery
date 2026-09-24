use super::{
    append_data_in_transaction, get, is_terminal, list, outcome_state, state_text, storage,
    timestamp_text, validate_workspace, ChildTurnRecord,
};
use crate::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentRunState, RunCompleted, RunOutcome, RunStateChanged,
};
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChildTurnCommandResult {
    pub child: ChildTurnRecord,
    pub events: Vec<AgentEventEnvelope>,
    pub replay_only: bool,
}

pub fn finish(
    connection: &Connection,
    workspace_id: &str,
    child_turn_id: Uuid,
    outcome: RunOutcome,
    timestamp: DateTime<Utc>,
) -> Result<(), StorageError> {
    validate_workspace(workspace_id)?;
    let state = outcome_state(outcome);
    connection
        .execute(
            "UPDATE agent_child_turns
             SET state = ?1, updated_at = ?2, completed_at = ?2
             WHERE workspace_id = ?3 AND child_turn_id = ?4
               AND state NOT IN ('completed', 'cancelled', 'failed', 'interrupted')",
            params![
                state_text(state),
                timestamp_text(timestamp),
                workspace_id,
                child_turn_id.to_string()
            ],
        )
        .map_err(storage)?;
    Ok(())
}

pub fn cancel(
    connection: &mut Connection,
    workspace_id: &str,
    child_turn_id: Uuid,
    timestamp: DateTime<Utc>,
) -> Result<ChildTurnCommandResult, StorageError> {
    validate_workspace(workspace_id)?;
    let current = get(connection, workspace_id, child_turn_id)?
        .ok_or_else(|| StorageError::new("agent_child_not_found", "child turn not found"))?;
    if is_terminal(current.state) {
        return Ok(ChildTurnCommandResult {
            child: current,
            events: replay(connection, workspace_id, child_turn_id, 0)?,
            replay_only: true,
        });
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let state_event = append_data_in_transaction(
        &transaction,
        workspace_id,
        child_turn_id,
        current.session_id,
        Uuid::new_v4(),
        timestamp,
        AgentEventData::RunStateChanged(RunStateChanged {
            previous: current.state,
            current: AgentRunState::Cancelled,
            reason: Some("user_cancelled".to_string()),
        }),
    )?;
    let completed_event = append_data_in_transaction(
        &transaction,
        workspace_id,
        child_turn_id,
        current.session_id,
        Uuid::new_v4(),
        timestamp,
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Cancelled,
            assistant_message_id: None,
        }),
    )?;
    let changed = transaction
        .execute(
            "UPDATE agent_child_turns
             SET state = 'cancelled', updated_at = ?1, completed_at = ?1
             WHERE workspace_id = ?2 AND child_turn_id = ?3
               AND state NOT IN ('completed', 'cancelled', 'failed', 'interrupted')",
            params![
                timestamp_text(timestamp),
                workspace_id,
                child_turn_id.to_string()
            ],
        )
        .map_err(storage)?;
    if changed != 1 {
        return Err(StorageError::new(
            "agent_child_state_conflict",
            "child turn state changed before cancellation",
        ));
    }
    transaction.commit().map_err(storage)?;
    Ok(ChildTurnCommandResult {
        child: get(connection, workspace_id, child_turn_id)?.ok_or_else(|| {
            StorageError::new("agent_child_storage_failed", "cancelled child disappeared")
        })?,
        events: vec![state_event, completed_event],
        replay_only: false,
    })
}

pub fn replay(
    connection: &Connection,
    workspace_id: &str,
    child_turn_id: Uuid,
    after_sequence: u64,
) -> Result<Vec<AgentEventEnvelope>, StorageError> {
    validate_workspace(workspace_id)?;
    let after_sequence = i64::try_from(after_sequence).map_err(|_| {
        StorageError::new(
            "agent_sequence_invalid",
            "child event sequence exceeds SQLite range",
        )
    })?;
    let mut statement = connection
        .prepare(
            "SELECT event_json FROM agent_child_turn_events
             WHERE workspace_id = ?1 AND child_turn_id = ?2 AND sequence > ?3
             ORDER BY sequence ASC",
        )
        .map_err(storage)?;
    let rows = statement
        .query_map(
            params![workspace_id, child_turn_id.to_string(), after_sequence],
            |row| row.get::<_, String>(0),
        )
        .map_err(storage)?;
    rows.map(|row| {
        let value = row.map_err(storage)?;
        let event: AgentEventEnvelope = serde_json::from_str(&value).map_err(super::decode)?;
        if event.run_id != child_turn_id {
            return Err(StorageError::new(
                "agent_child_event_corrupt",
                "child event run identity does not match storage identity",
            ));
        }
        Ok(event)
    })
    .collect()
}

/// A child task cannot be safely replayed after process termination because
/// its provider request may have reached the outside world. Mark it orphaned
/// and persist terminal events instead of attempting an implicit duplicate.
pub fn interrupt_orphans(
    connection: &mut Connection,
    workspace_id: &str,
    timestamp: DateTime<Utc>,
) -> Result<usize, StorageError> {
    validate_workspace(workspace_id)?;
    let orphaned = list(connection, workspace_id, None)?
        .into_iter()
        .filter(|child| !is_terminal(child.state))
        .collect::<Vec<_>>();
    if orphaned.is_empty() {
        return Ok(0);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    for child in &orphaned {
        append_data_in_transaction(
            &transaction,
            workspace_id,
            child.child_turn_id,
            child.session_id,
            Uuid::new_v4(),
            timestamp,
            AgentEventData::RunStateChanged(RunStateChanged {
                previous: child.state,
                current: AgentRunState::Interrupted,
                reason: Some("app_restart".to_string()),
            }),
        )?;
        append_data_in_transaction(
            &transaction,
            workspace_id,
            child.child_turn_id,
            child.session_id,
            Uuid::new_v4(),
            timestamp,
            AgentEventData::RunCompleted(RunCompleted {
                outcome: RunOutcome::Interrupted,
                assistant_message_id: None,
            }),
        )?;
        let changed = transaction
            .execute(
                "UPDATE agent_child_turns
                 SET state = 'interrupted', updated_at = ?1, completed_at = ?1
                 WHERE workspace_id = ?2 AND child_turn_id = ?3
                   AND state NOT IN ('completed', 'cancelled', 'failed', 'interrupted')",
                params![
                    timestamp_text(timestamp),
                    workspace_id,
                    child.child_turn_id.to_string()
                ],
            )
            .map_err(storage)?;
        if changed != 1 {
            return Err(StorageError::new(
                "agent_child_state_conflict",
                "child turn state changed during orphan recovery",
            ));
        }
    }
    transaction.commit().map_err(storage)?;
    Ok(orphaned.len())
}
