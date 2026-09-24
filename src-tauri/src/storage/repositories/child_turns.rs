use crate::agent::protocol::{AgentEventEnvelope, AgentRunState, RunOutcome, PROTOCOL_VERSION};
use crate::agent::runtime::TurnSnapshot;
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, TransactionBehavior};
use uuid::Uuid;

pub fn create(
    connection: &mut Connection,
    workspace_id: &str,
    snapshot: &TurnSnapshot,
    timestamp: DateTime<Utc>,
) -> Result<(), StorageError> {
    validate_workspace(workspace_id)?;
    let parent_turn_id = snapshot.parent_turn_id.ok_or_else(|| {
        StorageError::new(
            "agent_child_parent_missing",
            "child turn parent is required",
        )
    })?;
    if snapshot.turn_id.is_nil() || snapshot.session_id.is_nil() {
        return Err(StorageError::new(
            "agent_child_identity_invalid",
            "child turn and session IDs are required",
        ));
    }
    connection
        .execute(
            "INSERT INTO agent_child_turns
             (child_turn_id, workspace_id, parent_turn_id, parent_conversation_id,
              session_id, state, created_at, updated_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'created', ?6, ?6, NULL)",
            params![
                snapshot.turn_id.to_string(),
                workspace_id,
                parent_turn_id.to_string(),
                snapshot.session_id.to_string(),
                snapshot.session_id.to_string(),
                timestamp_text(timestamp),
            ],
        )
        .map_err(storage)?;
    Ok(())
}

pub fn append(
    connection: &mut Connection,
    workspace_id: &str,
    event: &AgentEventEnvelope,
) -> Result<AgentEventEnvelope, StorageError> {
    validate_workspace(workspace_id)?;
    if event.protocol_version != PROTOCOL_VERSION {
        return Err(StorageError::new(
            "agent_child_event_protocol_invalid",
            "child event protocol version is unsupported",
        ));
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let next_sequence: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1
             FROM agent_child_turn_events
             WHERE workspace_id = ?1 AND child_turn_id = ?2",
            params![workspace_id, event.run_id.to_string()],
            |row| row.get(0),
        )
        .map_err(storage)?;
    let sequence = u64::try_from(next_sequence).map_err(decode)?;
    let mut stored = event.clone();
    stored.sequence = sequence;
    let event_json = serde_json::to_string(&stored).map_err(encode)?;
    transaction
        .execute(
            "INSERT INTO agent_child_turn_events
             (event_id, workspace_id, child_turn_id, sequence,
              protocol_version, timestamp, event_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                stored.event_id.to_string(),
                workspace_id,
                stored.run_id.to_string(),
                next_sequence,
                i64::from(stored.protocol_version),
                timestamp_text(stored.timestamp),
                event_json,
            ],
        )
        .map_err(storage)?;
    transaction
        .execute(
            "UPDATE agent_child_turns SET updated_at = ?1
             WHERE workspace_id = ?2 AND child_turn_id = ?3",
            params![
                timestamp_text(stored.timestamp),
                workspace_id,
                stored.run_id.to_string()
            ],
        )
        .map_err(storage)?;
    transaction.commit().map_err(storage)?;
    Ok(stored)
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
        let event: AgentEventEnvelope = serde_json::from_str(&value).map_err(decode)?;
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
/// instead of attempting an implicit duplicate execution.
pub fn interrupt_orphans(
    connection: &Connection,
    workspace_id: &str,
    timestamp: DateTime<Utc>,
) -> Result<usize, StorageError> {
    validate_workspace(workspace_id)?;
    connection
        .execute(
            "UPDATE agent_child_turns
             SET state = 'interrupted', updated_at = ?1, completed_at = ?1
             WHERE workspace_id = ?2
               AND state NOT IN ('completed', 'cancelled', 'failed', 'interrupted')",
            params![timestamp_text(timestamp), workspace_id],
        )
        .map_err(storage)
}

fn outcome_state(outcome: RunOutcome) -> AgentRunState {
    match outcome {
        RunOutcome::Completed => AgentRunState::Completed,
        RunOutcome::Cancelled => AgentRunState::Cancelled,
        RunOutcome::Failed => AgentRunState::Failed,
        RunOutcome::Interrupted => AgentRunState::Interrupted,
    }
}

fn state_text(state: AgentRunState) -> &'static str {
    match state {
        AgentRunState::Created => "created",
        AgentRunState::Preparing => "preparing",
        AgentRunState::Generating => "generating",
        AgentRunState::AwaitingPermission => "awaiting_permission",
        AgentRunState::ExecutingTools => "executing_tools",
        AgentRunState::Verifying => "verifying",
        AgentRunState::Completing => "completing",
        AgentRunState::Completed => "completed",
        AgentRunState::Cancelled => "cancelled",
        AgentRunState::Failed => "failed",
        AgentRunState::Interrupted => "interrupted",
    }
}

fn validate_workspace(workspace_id: &str) -> Result<(), StorageError> {
    if workspace_id.trim().is_empty() || workspace_id.trim() != workspace_id {
        Err(StorageError::new(
            "agent_workspace_invalid",
            "workspace ID is invalid",
        ))
    } else {
        Ok(())
    }
}

fn timestamp_text(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

fn storage(error: rusqlite::Error) -> StorageError {
    StorageError::new("agent_child_storage_failed", error.to_string())
}

fn encode(error: serde_json::Error) -> StorageError {
    StorageError::new("agent_child_event_encode_failed", error.to_string())
}

fn decode(error: impl std::fmt::Display) -> StorageError {
    StorageError::new("agent_child_event_decode_failed", error.to_string())
}
