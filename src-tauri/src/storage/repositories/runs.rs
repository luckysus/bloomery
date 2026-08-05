use super::events;
use crate::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentRunState, RunCompleted, RunCreated, RunOutcome,
};
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct RunWithEvent {
    pub run: AgentRunRecord,
    pub event: AgentEventEnvelope,
}

pub fn create(
    connection: &mut Connection,
    new_run: NewAgentRun,
) -> Result<RunWithEvent, StorageError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let created = create_in_transaction(&transaction, new_run)?;
    transaction.commit().map_err(storage)?;
    Ok(created)
}

pub fn create_in_transaction(
    transaction: &Transaction<'_>,
    new_run: NewAgentRun,
) -> Result<RunWithEvent, StorageError> {
    validate_workspace(&new_run.workspace_id)?;
    let message_exists = transaction
        .query_row(
            "SELECT 1
             FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             WHERE m.workspace_id = ?1 AND c.workspace_id = ?1
               AND m.conversation_id = ?2 AND m.id = ?3 AND m.role = 'user'",
            params![
                new_run.workspace_id,
                new_run.conversation_id.to_string(),
                new_run.user_message_id.to_string()
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(storage)?
        .is_some();
    if !message_exists {
        return Err(StorageError::new(
            "agent_user_message_not_found",
            "user message does not belong to the requested conversation and workspace",
        ));
    }
    let timestamp = timestamp_text(new_run.timestamp);
    transaction
        .execute(
            "INSERT INTO agent_runs
             (id, workspace_id, conversation_id, user_message_id, state,
              next_sequence, created_at, updated_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, 'created', 1, ?5, ?5, NULL)",
            params![
                new_run.id.to_string(),
                new_run.workspace_id,
                new_run.conversation_id.to_string(),
                new_run.user_message_id.to_string(),
                timestamp,
            ],
        )
        .map_err(run_insert)?;
    let event = events::append_in_transaction(
        transaction,
        &new_run.workspace_id,
        new_run.id,
        new_run.event_id,
        new_run.timestamp,
        AgentEventData::RunCreated(RunCreated {
            state: AgentRunState::Created,
            user_message_id: new_run.user_message_id,
        }),
    )?;
    let run = read(transaction, &new_run.workspace_id, new_run.id)?
        .ok_or_else(|| StorageError::new("agent_run_storage_failed", "created run disappeared"))?;
    Ok(RunWithEvent { run, event })
}

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Option<AgentRunRecord>, StorageError> {
    validate_workspace(workspace_id)?;
    read(connection, workspace_id, run_id)
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
    let changed = transaction
        .execute(
            "UPDATE agent_runs
             SET state = ?1, updated_at = ?2, completed_at = ?2
             WHERE workspace_id = ?3 AND id = ?4 AND state = 'completing'",
            params![
                state_text(target),
                timestamp_text(timestamp),
                workspace_id,
                run_id.to_string()
            ],
        )
        .map_err(storage)?;
    if changed != 1 {
        return Err(StorageError::new(
            "agent_run_not_completing",
            "agent run state changed before completion",
        ));
    }
    let event = events::append_in_transaction(
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

fn read(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Option<AgentRunRecord>, StorageError> {
    connection
        .query_row(
            "SELECT id, workspace_id, conversation_id, user_message_id, state,
                    next_sequence, created_at, updated_at, completed_at
             FROM agent_runs WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, run_id.to_string()],
            |row| {
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
            },
        )
        .optional()
        .map_err(storage)?
        .map(decode_run)
        .transpose()
}

struct RawRun {
    id: String,
    workspace_id: String,
    conversation_id: String,
    user_message_id: String,
    state: String,
    next_sequence: i64,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
}

fn decode_run(raw: RawRun) -> Result<AgentRunRecord, StorageError> {
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
            "agent_run_decode_failed",
            format!("unknown agent run state: {value}"),
        )),
    }
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(decode)
}

fn timestamp_text(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
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

fn run_insert(error: rusqlite::Error) -> StorageError {
    if matches!(
        &error,
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == SQLITE_CONSTRAINT_PRIMARYKEY
    ) {
        return StorageError::new("agent_run_duplicate", "agent run ID already exists");
    }
    StorageError::new("agent_run_storage_failed", error.to_string())
}

fn storage(error: rusqlite::Error) -> StorageError {
    StorageError::new("agent_run_storage_failed", error.to_string())
}

fn decode(error: impl std::fmt::Display) -> StorageError {
    StorageError::new("agent_run_decode_failed", error.to_string())
}
