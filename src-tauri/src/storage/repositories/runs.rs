mod create;
mod query;
mod transition;

pub use create::{create, create_in_transaction};
pub use query::{get, list_nonterminal};
pub use transition::{complete, finish, transition};

use super::events;
use crate::agent::protocol::{AgentEventEnvelope, AgentRunState, RunOutcome};
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentRun {
    pub id: Uuid,
    pub workspace_id: String,
    pub conversation_id: Uuid,
    pub user_message_id: Uuid,
    pub event_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentRunRecord {
    pub id: Uuid,
    pub workspace_id: String,
    pub conversation_id: Uuid,
    pub user_message_id: Uuid,
    pub state: AgentRunState,
    pub next_sequence: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunWithEvent {
    pub run: AgentRunRecord,
    pub event: AgentEventEnvelope,
}

pub(super) struct RawRun {
    pub(super) id: String,
    pub(super) workspace_id: String,
    pub(super) conversation_id: String,
    pub(super) user_message_id: String,
    pub(super) state: String,
    pub(super) next_sequence: i64,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) completed_at: Option<String>,
}

pub(super) fn read(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Option<AgentRunRecord>, StorageError> {
    connection.query_row(
        "SELECT id, workspace_id, conversation_id, user_message_id, state, next_sequence, created_at, updated_at, completed_at FROM agent_runs WHERE workspace_id = ?1 AND id = ?2",
        params![workspace_id, run_id.to_string()],
        row_to_raw_run,
    ).optional().map_err(storage)?.map(decode_run).transpose()
}

pub(super) fn row_to_raw_run(row: &Row<'_>) -> rusqlite::Result<RawRun> {
    Ok(RawRun {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        conversation_id: row.get(2)?,
        user_message_id: row.get(3)?,
        state: row.get(4)?,
        next_sequence: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        completed_at: row.get(8)?,
    })
}

pub(super) fn decode_run(raw: RawRun) -> Result<AgentRunRecord, StorageError> {
    Ok(AgentRunRecord {
        id: Uuid::parse_str(&raw.id).map_err(decode)?,
        workspace_id: raw.workspace_id,
        conversation_id: Uuid::parse_str(&raw.conversation_id).map_err(decode)?,
        user_message_id: Uuid::parse_str(&raw.user_message_id).map_err(decode)?,
        state: parse_state(&raw.state)?,
        next_sequence: u64::try_from(raw.next_sequence).map_err(decode)?,
        created_at: parse_timestamp(&raw.created_at)?,
        updated_at: parse_timestamp(&raw.updated_at)?,
        completed_at: raw
            .completed_at
            .map(|value| parse_timestamp(&value))
            .transpose()?,
    })
}

pub(super) fn outcome_state(outcome: RunOutcome) -> AgentRunState {
    match outcome {
        RunOutcome::Completed => AgentRunState::Completed,
        RunOutcome::Cancelled => AgentRunState::Cancelled,
        RunOutcome::Failed => AgentRunState::Failed,
        RunOutcome::Interrupted => AgentRunState::Interrupted,
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

pub(super) fn parse_state(value: &str) -> Result<AgentRunState, StorageError> {
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
            "agent_run_decode_failed",
            format!("unknown agent run state: {value}"),
        )),
    }
}

pub(super) fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(decode)
}

pub(super) fn timestamp_text(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
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

pub(super) fn run_insert(error: rusqlite::Error) -> StorageError {
    if matches!(&error, rusqlite::Error::SqliteFailure(code, _) if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY)
    {
        return StorageError::new("agent_run_duplicate", "agent run ID already exists");
    }
    StorageError::new("agent_run_storage_failed", error.to_string())
}

pub(super) fn storage(error: rusqlite::Error) -> StorageError {
    StorageError::new("agent_run_storage_failed", error.to_string())
}

pub(super) fn decode(error: impl std::fmt::Display) -> StorageError {
    StorageError::new("agent_run_decode_failed", error.to_string())
}

#[allow(dead_code)]
pub(super) fn _transaction_marker(_transaction: &Transaction<'_>) -> bool {
    true
}
