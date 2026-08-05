use crate::agent::context::{
    normalize_memory_key, MemoryCandidate, MemoryStatus, AUTO_MEMORY_WRITE_SETTING,
};
use crate::models::Memory;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

pub(super) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(super) fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    let stored_status = row.get::<_, String>(14)?;
    let status = MemoryStatus::from_storage(&stored_status).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            14,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid memory status: {stored_status}"),
            )),
        )
    })?;
    Ok(Memory {
        id: row.get(0)?,
        scope: row.get(1)?,
        r#type: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        body: row.get(5)?,
        tags_json: row.get(6)?,
        enabled: row.get::<_, i64>(7)? != 0,
        archived_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        source_message_id: row.get(11)?,
        source_run_id: row.get(12)?,
        confidence: row.get(13)?,
        status,
        dedup_key: row.get(15)?,
    })
}

pub fn capture_candidate(
    connection: &mut Connection,
    workspace_id: &str,
    candidate: MemoryCandidate,
) -> Result<Memory, String> {
    if !candidate.confidence.is_finite() || !(0.0..=1.0).contains(&candidate.confidence) {
        return Err("memory confidence must be finite and between 0 and 1".to_string());
    }
    let body = candidate.body.trim();
    let title = candidate.title.trim();
    let dedup_key = normalize_memory_key(body);
    if title.is_empty() || body.is_empty() || dedup_key.is_empty() {
        return Err("memory title and body are required".to_string());
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    validate_source(
        &transaction,
        workspace_id,
        &candidate.source_message_id,
        candidate.source_run_id.as_deref(),
    )?;
    let duplicate_key = transaction
        .query_row(
            "SELECT 1 FROM memories WHERE workspace_id = ?1 AND dedup_key = ?2 LIMIT 1",
            params![workspace_id, dedup_key],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if duplicate_key || legacy_body_duplicate(&transaction, workspace_id, &dedup_key)? {
        return Err("memory duplicate".to_string());
    }

    let auto_confirm = transaction
        .query_row(
            "SELECT value_json FROM settings WHERE workspace_id = ?1 AND key = ?2",
            params![workspace_id, AUTO_MEMORY_WRITE_SETTING],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .as_deref()
        == Some("true");
    let status = if auto_confirm {
        MemoryStatus::Confirmed
    } else {
        MemoryStatus::Pending
    };
    let timestamp = now();
    let id = Uuid::new_v4().to_string();
    transaction
        .execute(
            "INSERT INTO memories
             (id, workspace_id, scope, type, title, description, body, tags_json,
              enabled, archived_at, created_at, updated_at, source_message_id,
              source_run_id, confidence, status, dedup_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?10,
                     ?11, ?12, ?13, ?14, ?15)",
            params![
                id,
                workspace_id,
                candidate.scope,
                candidate.memory_type,
                title,
                candidate.description,
                body,
                candidate.tags_json,
                i64::from(auto_confirm),
                timestamp,
                candidate.source_message_id,
                candidate.source_run_id,
                candidate.confidence,
                status.as_str(),
                dedup_key,
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    super::get(connection, workspace_id, &id)?.ok_or_else(|| "memory not found".to_string())
}

pub fn confirm_candidate(
    connection: &mut Connection,
    workspace_id: &str,
    id: &str,
) -> Result<(), String> {
    update_lifecycle(
        connection,
        "UPDATE memories SET status = 'confirmed', enabled = 1, updated_at = ?1
         WHERE workspace_id = ?2 AND id = ?3",
        workspace_id,
        id,
    )
}

pub fn reject_candidate(
    connection: &mut Connection,
    workspace_id: &str,
    id: &str,
) -> Result<(), String> {
    update_lifecycle(
        connection,
        "UPDATE memories SET status = 'rejected', enabled = 0, updated_at = ?1
         WHERE workspace_id = ?2 AND id = ?3",
        workspace_id,
        id,
    )
}

pub fn set_enabled(
    connection: &mut Connection,
    workspace_id: &str,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let current =
        super::get(connection, workspace_id, id)?.ok_or_else(|| "memory not found".to_string())?;

    if enabled
        && (current.status == MemoryStatus::Pending || current.status == MemoryStatus::Rejected)
    {
        return Err("memory must be confirmed before enabling".to_string());
    }

    let updated = connection
        .execute(
            "UPDATE memories SET enabled = ?1, updated_at = ?2
             WHERE workspace_id = ?3 AND id = ?4",
            params![i64::from(enabled), now(), workspace_id, id],
        )
        .map_err(|error| error.to_string())?;
    changed(updated)
}

pub fn delete(connection: &mut Connection, workspace_id: &str, id: &str) -> Result<(), String> {
    let deleted = connection
        .execute(
            "DELETE FROM memories WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id],
        )
        .map_err(|error| error.to_string())?;
    changed(deleted)
}

pub fn list_context(connection: &Connection, workspace_id: &str) -> Result<Vec<Memory>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, scope, type, title, description, body, tags_json, enabled,
                    archived_at, created_at, updated_at, source_message_id, source_run_id,
                    confidence, status, dedup_key
             FROM memories
             WHERE workspace_id = ?1 AND enabled = 1 AND archived_at IS NULL
               AND status = 'confirmed'
             ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], row_to_memory)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn validate_source(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    message_id: &str,
    run_id: Option<&str>,
) -> Result<(), String> {
    let message_exists = transaction
        .query_row(
            "SELECT 1 FROM messages WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, message_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if !message_exists {
        return Err("memory source message not found".to_string());
    }
    let Some(run_id) = run_id else {
        return Ok(());
    };
    let run_exists = transaction
        .query_row(
            "SELECT 1 FROM agent_runs
             WHERE workspace_id = ?1 AND id = ?2 AND user_message_id = ?3",
            params![workspace_id, run_id, message_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if run_exists {
        Ok(())
    } else {
        Err("memory source run not found".to_string())
    }
}

fn legacy_body_duplicate(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    dedup_key: &str,
) -> Result<bool, String> {
    let mut statement = transaction
        .prepare(
            "SELECT body FROM memories
             WHERE workspace_id = ?1 AND dedup_key LIKE 'legacy:%'",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    for body in rows {
        if normalize_memory_key(&body.map_err(|error| error.to_string())?) == dedup_key {
            return Ok(true);
        }
    }
    Ok(false)
}

fn update_lifecycle(
    connection: &mut Connection,
    statement: &str,
    workspace_id: &str,
    id: &str,
) -> Result<(), String> {
    let updated = connection
        .execute(statement, params![now(), workspace_id, id])
        .map_err(|error| error.to_string())?;
    changed(updated)
}

fn changed(count: usize) -> Result<(), String> {
    if count == 0 {
        Err("memory not found".to_string())
    } else {
        Ok(())
    }
}
