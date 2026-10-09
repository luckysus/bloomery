use super::{is_terminal, outcome_state, state_text, storage, timestamp_text};
use crate::agent::protocol::{AgentEventData, AgentEventEnvelope, PROTOCOL_VERSION};
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Transaction};
use uuid::Uuid;

pub(super) fn append_data_in_transaction(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    child_turn_id: Uuid,
    conversation_id: Uuid,
    event_id: Uuid,
    timestamp: DateTime<Utc>,
    data: AgentEventData,
) -> Result<AgentEventEnvelope, StorageError> {
    let next_sequence: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM agent_child_turn_events WHERE workspace_id = ?1 AND child_turn_id = ?2",
        params![workspace_id, child_turn_id.to_string()], |row| row.get(0),
    ).map_err(storage)?;
    let sequence = u64::try_from(next_sequence).map_err(super::decode)?;
    let stored = AgentEventEnvelope {
        protocol_version: PROTOCOL_VERSION,
        event_id,
        run_id: child_turn_id,
        conversation_id,
        sequence,
        timestamp,
        data,
    };
    let event_json = serde_json::to_string(&stored).map_err(super::encode)?;
    transaction.execute(
        "INSERT INTO agent_child_turn_events (event_id, workspace_id, child_turn_id, sequence, protocol_version, timestamp, event_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![stored.event_id.to_string(), workspace_id, child_turn_id.to_string(), next_sequence, i64::from(stored.protocol_version), timestamp_text(stored.timestamp), event_json],
    ).map_err(storage)?;
    Ok(stored)
}

pub(super) fn update_state_for_event(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    child_turn_id: Uuid,
    data: &AgentEventData,
    timestamp: DateTime<Utc>,
) -> Result<(), StorageError> {
    let (state, completed) = match data {
        AgentEventData::RunStateChanged(changed) => (changed.current, is_terminal(changed.current)),
        AgentEventData::RunCompleted(completed) => (outcome_state(completed.outcome), true),
        _ => return Ok(()),
    };
    transaction.execute(
        "UPDATE agent_child_turns SET state = ?1, updated_at = ?2, completed_at = CASE WHEN ?3 = 1 THEN ?2 ELSE completed_at END WHERE workspace_id = ?4 AND child_turn_id = ?5",
        params![state_text(state), timestamp_text(timestamp), i64::from(completed), workspace_id, child_turn_id.to_string()],
    ).map_err(storage)?;
    Ok(())
}
