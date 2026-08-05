use crate::agent::protocol::{AgentEventData, AgentEventEnvelope, PROTOCOL_VERSION};
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

pub fn append(
    connection: &mut Connection,
    workspace_id: &str,
    run_id: Uuid,
    event_id: Uuid,
    timestamp: DateTime<Utc>,
    data: AgentEventData,
) -> Result<AgentEventEnvelope, StorageError> {
    if matches!(
        &data,
        AgentEventData::RunCreated(_)
            | AgentEventData::RunStateChanged(_)
            | AgentEventData::RunCompleted(_)
    ) {
        return Err(StorageError::new(
            "agent_state_event_requires_run_repository",
            "run state events must be persisted with their run state change",
        ));
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let event = append_in_transaction(
        &transaction,
        workspace_id,
        run_id,
        event_id,
        timestamp,
        data,
    )?;
    transaction.commit().map_err(storage)?;
    Ok(event)
}

pub fn replay(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
    after_sequence: u64,
) -> Result<Vec<AgentEventEnvelope>, StorageError> {
    validate_workspace(workspace_id)?;
    let exists = connection
        .query_row(
            "SELECT 1 FROM agent_runs WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, run_id.to_string()],
            |_| Ok(()),
        )
        .optional()
        .map_err(storage)?
        .is_some();
    if !exists {
        return Err(StorageError::new(
            "agent_run_not_found",
            "agent run not found",
        ));
    }
    let after_sequence = i64::try_from(after_sequence).map_err(|_| {
        StorageError::new(
            "agent_sequence_invalid",
            "event sequence exceeds SQLite range",
        )
    })?;
    let mut statement = connection
        .prepare(
            "SELECT event_id, conversation_id, sequence, protocol_version, timestamp, event_json
             FROM agent_run_events
             WHERE workspace_id = ?1 AND run_id = ?2 AND sequence > ?3
             ORDER BY sequence ASC",
        )
        .map_err(storage)?;
    let rows = statement
        .query_map(
            params![workspace_id, run_id.to_string(), after_sequence],
            |row| {
                Ok(PersistedEvent {
                    event_id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    sequence: row.get(2)?,
                    protocol_version: row.get(3)?,
                    timestamp: row.get(4)?,
                    event_json: row.get(5)?,
                })
            },
        )
        .map_err(storage)?;
    rows.map(|row| decode_event(run_id, row.map_err(storage)?))
        .collect()
}

pub(crate) fn append_in_transaction(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    run_id: Uuid,
    event_id: Uuid,
    timestamp: DateTime<Utc>,
    data: AgentEventData,
) -> Result<AgentEventEnvelope, StorageError> {
    validate_workspace(workspace_id)?;
    let (conversation_id, next_sequence) = transaction
        .query_row(
            "SELECT conversation_id, next_sequence
             FROM agent_runs WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, run_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(storage)?
        .ok_or_else(|| StorageError::new("agent_run_not_found", "agent run not found"))?;
    let conversation_id = Uuid::parse_str(&conversation_id).map_err(decode)?;
    let sequence = u64::try_from(next_sequence).map_err(decode)?;
    let event = AgentEventEnvelope {
        protocol_version: PROTOCOL_VERSION,
        event_id,
        run_id,
        conversation_id,
        sequence,
        timestamp,
        data,
    };
    let event_json = serde_json::to_string(&event).map_err(encode)?;
    transaction
        .execute(
            "INSERT INTO agent_run_events
             (event_id, workspace_id, run_id, conversation_id, sequence,
              protocol_version, timestamp, event_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.event_id.to_string(),
                workspace_id,
                event.run_id.to_string(),
                event.conversation_id.to_string(),
                next_sequence,
                i64::from(event.protocol_version),
                timestamp_text(event.timestamp),
                event_json,
            ],
        )
        .map_err(event_insert)?;
    let changed = transaction
        .execute(
            "UPDATE agent_runs
             SET next_sequence = next_sequence + 1, updated_at = ?1
             WHERE workspace_id = ?2 AND id = ?3 AND next_sequence = ?4",
            params![
                timestamp_text(event.timestamp),
                workspace_id,
                run_id.to_string(),
                next_sequence
            ],
        )
        .map_err(storage)?;
    if changed != 1 {
        return Err(StorageError::new(
            "agent_sequence_conflict",
            "agent event sequence changed during allocation",
        ));
    }
    Ok(event)
}

struct PersistedEvent {
    event_id: String,
    conversation_id: String,
    sequence: i64,
    protocol_version: i64,
    timestamp: String,
    event_json: String,
}

fn decode_event(
    run_id: Uuid,
    persisted: PersistedEvent,
) -> Result<AgentEventEnvelope, StorageError> {
    let event: AgentEventEnvelope = serde_json::from_str(&persisted.event_json).map_err(decode)?;
    let sequence = u64::try_from(persisted.sequence).map_err(decode)?;
    let protocol_version = u16::try_from(persisted.protocol_version).map_err(decode)?;
    let event_id = Uuid::parse_str(&persisted.event_id).map_err(decode)?;
    let conversation_id = Uuid::parse_str(&persisted.conversation_id).map_err(decode)?;
    let timestamp = DateTime::parse_from_rfc3339(&persisted.timestamp)
        .map_err(decode)?
        .with_timezone(&Utc);
    if event.event_id != event_id
        || event.run_id != run_id
        || event.conversation_id != conversation_id
        || event.sequence != sequence
        || event.protocol_version != protocol_version
        || event.timestamp != timestamp
    {
        return Err(StorageError::new(
            "agent_event_corrupt",
            "agent event envelope does not match its storage identity",
        ));
    }
    Ok(event)
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

fn event_insert(error: rusqlite::Error) -> StorageError {
    if matches!(
        &error,
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == SQLITE_CONSTRAINT_PRIMARYKEY
    ) {
        return StorageError::new("agent_event_duplicate", "agent event ID already exists");
    }
    StorageError::new("agent_event_storage_failed", error.to_string())
}

fn storage(error: rusqlite::Error) -> StorageError {
    StorageError::new("agent_event_storage_failed", error.to_string())
}

fn encode(error: serde_json::Error) -> StorageError {
    StorageError::new("agent_event_encode_failed", error.to_string())
}

fn decode(error: impl std::fmt::Display) -> StorageError {
    StorageError::new("agent_event_decode_failed", error.to_string())
}
