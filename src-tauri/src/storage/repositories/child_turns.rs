use crate::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentRunState, RunOutcome, PROTOCOL_VERSION,
};
use crate::agent::runtime::TurnSnapshot;
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;
#[path = "child_turns_control.rs"]
mod control;
#[path = "child_turns/events.rs"]
mod events;
pub use control::{cancel, finish, interrupt_orphans, replay, ChildTurnCommandResult};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChildTurnRecord {
    pub child_turn_id: Uuid,
    pub workspace_id: String,
    pub parent_turn_id: Uuid,
    pub parent_conversation_id: Uuid,
    pub session_id: Uuid,
    pub state: AgentRunState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub agent_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub task_summary: String,
}
pub fn create(
    connection: &mut Connection,
    workspace_id: &str,
    snapshot: &TurnSnapshot,
    timestamp: DateTime<Utc>,
) -> Result<(), StorageError> {
    create_for_task(connection, workspace_id, snapshot, "", None, timestamp)
}
pub fn create_for_task(
    connection: &mut Connection,
    workspace_id: &str,
    snapshot: &TurnSnapshot,
    task: &str,
    agent_id: Option<&str>,
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
    let metadata = json!({
        "snapshot": snapshot,
        "task_summary": task.chars().take(512).collect::<String>(),
        "agent_id": agent_id,
    })
    .to_string();
    if metadata.len() > 512 * 1024 {
        return Err(StorageError::new(
            "agent_child_snapshot_too_large",
            "child snapshot exceeds 512 KiB",
        ));
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    transaction
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
    transaction.execute(
        "INSERT INTO settings (workspace_id, key, value_json, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![workspace_id, metadata_key(snapshot.turn_id), metadata, timestamp_text(timestamp)],
    ).map_err(storage)?;
    transaction.commit().map_err(storage)?;
    Ok(())
}
pub fn get(
    connection: &Connection,
    workspace_id: &str,
    child_turn_id: Uuid,
) -> Result<Option<ChildTurnRecord>, StorageError> {
    validate_workspace(workspace_id)?;
    let record = connection
        .query_row(
            "SELECT child_turn_id, workspace_id, parent_turn_id,
                    parent_conversation_id, session_id, state,
                    created_at, updated_at, completed_at
             FROM agent_child_turns
             WHERE workspace_id = ?1 AND child_turn_id = ?2",
            params![workspace_id, child_turn_id.to_string()],
            row_to_record,
        )
        .optional()
        .map_err(storage)?
        .map(decode_record)
        .transpose()?;
    record
        .map(|mut record| {
            hydrate_metadata(connection, workspace_id, &mut record)?;
            Ok(record)
        })
        .transpose()
}
pub fn list(
    connection: &Connection,
    workspace_id: &str,
    parent_turn_id: Option<Uuid>,
) -> Result<Vec<ChildTurnRecord>, StorageError> {
    validate_workspace(workspace_id)?;
    let (sql, parameters): (&str, Vec<String>) = if let Some(parent_turn_id) = parent_turn_id {
        (
            "SELECT child_turn_id, workspace_id, parent_turn_id,
                    parent_conversation_id, session_id, state,
                    created_at, updated_at, completed_at
             FROM agent_child_turns
             WHERE workspace_id = ?1 AND parent_turn_id = ?2
             ORDER BY created_at ASC, child_turn_id ASC",
            vec![workspace_id.to_string(), parent_turn_id.to_string()],
        )
    } else {
        (
            "SELECT child_turn_id, workspace_id, parent_turn_id,
                    parent_conversation_id, session_id, state,
                    created_at, updated_at, completed_at
             FROM agent_child_turns
             WHERE workspace_id = ?1
             ORDER BY created_at ASC, child_turn_id ASC",
            vec![workspace_id.to_string()],
        )
    };
    let mut statement = connection.prepare(sql).map_err(storage)?;
    let rows = if parameters.len() == 2 {
        statement
            .query_map(params![parameters[0], parameters[1]], row_to_record)
            .map_err(storage)?
    } else {
        statement
            .query_map(params![parameters[0]], row_to_record)
            .map_err(storage)?
    };
    let mut records = rows
        .map(|row| row.map_err(storage).and_then(decode_record))
        .collect::<Result<Vec<_>, _>>()?;
    for record in &mut records {
        hydrate_metadata(connection, workspace_id, record)?;
    }
    Ok(records)
}
fn metadata_key(child_turn_id: Uuid) -> String {
    format!("agent.child_turn.{child_turn_id}")
}
fn hydrate_metadata(
    connection: &Connection,
    workspace_id: &str,
    record: &mut ChildTurnRecord,
) -> Result<(), StorageError> {
    let raw = crate::storage::repositories::settings::get(
        connection,
        workspace_id,
        &metadata_key(record.child_turn_id),
    )
    .map_err(|error| StorageError::new("agent_child_snapshot_read_failed", error))?;
    if let Some(raw) = raw {
        let metadata: serde_json::Value = serde_json::from_str(&raw).map_err(decode)?;
        record.agent_id = metadata["agent_id"].as_str().map(str::to_string);
        record.provider = metadata["snapshot"]["provider"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        record.model = metadata["snapshot"]["model"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        record.task_summary = metadata["task_summary"]
            .as_str()
            .unwrap_or_default()
            .to_string();
    }
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
    let stored = events::append_data_in_transaction(
        &transaction,
        workspace_id,
        event.run_id,
        event.conversation_id,
        event.event_id,
        event.timestamp,
        event.data.clone(),
    )?;
    events::update_state_for_event(
        &transaction,
        workspace_id,
        stored.run_id,
        &stored.data,
        stored.timestamp,
    )?;
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
pub(super) fn append_data_in_transaction(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    child_turn_id: Uuid,
    conversation_id: Uuid,
    event_id: Uuid,
    timestamp: DateTime<Utc>,
    data: AgentEventData,
) -> Result<AgentEventEnvelope, StorageError> {
    events::append_data_in_transaction(
        transaction,
        workspace_id,
        child_turn_id,
        conversation_id,
        event_id,
        timestamp,
        data,
    )
}
fn row_to_record(row: &Row<'_>) -> rusqlite::Result<RawChildTurn> {
    Ok(RawChildTurn {
        child_turn_id: row.get(0)?,
        workspace_id: row.get(1)?,
        parent_turn_id: row.get(2)?,
        parent_conversation_id: row.get(3)?,
        session_id: row.get(4)?,
        state: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        completed_at: row.get(8)?,
    })
}
struct RawChildTurn {
    child_turn_id: String,
    workspace_id: String,
    parent_turn_id: String,
    parent_conversation_id: String,
    session_id: String,
    state: String,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
}
fn decode_record(raw: RawChildTurn) -> Result<ChildTurnRecord, StorageError> {
    Ok(ChildTurnRecord {
        child_turn_id: parse_uuid(&raw.child_turn_id)?,
        workspace_id: raw.workspace_id,
        parent_turn_id: parse_uuid(&raw.parent_turn_id)?,
        parent_conversation_id: parse_uuid(&raw.parent_conversation_id)?,
        session_id: parse_uuid(&raw.session_id)?,
        state: parse_state(&raw.state)?,
        created_at: parse_timestamp(&raw.created_at)?,
        updated_at: parse_timestamp(&raw.updated_at)?,
        completed_at: raw
            .completed_at
            .map(|value| parse_timestamp(&value))
            .transpose()?,
        agent_id: None,
        provider: String::new(),
        model: String::new(),
        task_summary: String::new(),
    })
}
fn parse_uuid(value: &str) -> Result<Uuid, StorageError> {
    Uuid::parse_str(value).map_err(decode)
}
fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(decode)
}
fn parse_state(value: &str) -> Result<AgentRunState, StorageError> {
    match value {
        "created" => Ok(AgentRunState::Created),
        "preparing" => Ok(AgentRunState::Preparing),
        "generating" => Ok(AgentRunState::Generating),
        "awaiting_permission" => Ok(AgentRunState::AwaitingPermission),
        "executing_tools" => Ok(AgentRunState::ExecutingTools),
        "verifying" => Ok(AgentRunState::Verifying),
        "completing" => Ok(AgentRunState::Completing),
        "completed" => Ok(AgentRunState::Completed),
        "cancelled" => Ok(AgentRunState::Cancelled),
        "failed" => Ok(AgentRunState::Failed),
        "interrupted" => Ok(AgentRunState::Interrupted),
        _ => Err(StorageError::new(
            "agent_child_decode_failed",
            format!("unknown child turn state: {value}"),
        )),
    }
}

pub(super) fn is_terminal(state: AgentRunState) -> bool {
    matches!(
        state,
        AgentRunState::Completed
            | AgentRunState::Cancelled
            | AgentRunState::Failed
            | AgentRunState::Interrupted
    )
}

pub(super) fn outcome_state(outcome: RunOutcome) -> AgentRunState {
    match outcome {
        RunOutcome::Completed => AgentRunState::Completed,
        RunOutcome::Cancelled => AgentRunState::Cancelled,
        RunOutcome::Failed => AgentRunState::Failed,
        RunOutcome::Interrupted => AgentRunState::Interrupted,
    }
}

pub(super) fn state_text(state: AgentRunState) -> &'static str {
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

pub(super) fn validate_workspace(workspace_id: &str) -> Result<(), StorageError> {
    if workspace_id.trim().is_empty() || workspace_id.trim() != workspace_id {
        Err(StorageError::new(
            "agent_workspace_invalid",
            "workspace ID is invalid",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn timestamp_text(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

pub(super) fn storage(error: rusqlite::Error) -> StorageError {
    StorageError::new("agent_child_storage_failed", error.to_string())
}

pub(super) fn encode(error: serde_json::Error) -> StorageError {
    StorageError::new("agent_child_event_encode_failed", error.to_string())
}

pub(super) fn decode(error: impl std::fmt::Display) -> StorageError {
    StorageError::new("agent_child_event_decode_failed", error.to_string())
}
