use super::{decode_run, read, row_to_raw_run, storage, validate_workspace, AgentRunRecord};
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use uuid::Uuid;

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Option<AgentRunRecord>, StorageError> {
    validate_workspace(workspace_id)?;
    read(connection, workspace_id, run_id)
}

pub fn list_nonterminal(
    connection: &Connection,
    workspace_id: &str,
    updated_before: Option<DateTime<Utc>>,
) -> Result<Vec<AgentRunRecord>, StorageError> {
    validate_workspace(workspace_id)?;
    let updated_before = updated_before.map(super::timestamp_text);
    let mut statement = connection.prepare("SELECT id, workspace_id, conversation_id, user_message_id, state, next_sequence, created_at, updated_at, completed_at FROM agent_runs WHERE workspace_id = ?1 AND state NOT IN ('completed', 'cancelled', 'failed', 'interrupted') AND (?2 IS NULL OR updated_at <= ?2) ORDER BY updated_at ASC, id ASC").map_err(storage)?;
    let rows = statement
        .query_map(params![workspace_id, updated_before], row_to_raw_run)
        .map_err(storage)?;
    rows.map(|row| row.map_err(storage).and_then(decode_run))
        .collect()
}
