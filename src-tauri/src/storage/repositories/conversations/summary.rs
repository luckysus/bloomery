use super::{now, valid_draft_key};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn get_summary(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<String, String> {
    conn.query_row("SELECT summary FROM conversation_summaries WHERE workspace_id = ?1 AND conversation_id = ?2 ORDER BY updated_at DESC LIMIT 1", params![workspace_id, conversation_id], |row| row.get(0)).optional().map(|value| value.unwrap_or_default()).map_err(|error| error.to_string())
}

pub fn save_summary(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    summary: &str,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    if !super::basic::belongs_to_workspace(&tx, workspace_id, conversation_id)? {
        super::ensure_id_available(&tx, workspace_id, conversation_id)?;
        return Err("conversation not found".to_string());
    }
    let source_message_ids = if let Some(covered_message_id) = covered_message_id.as_deref() {
        let anchor = tx.query_row("SELECT created_at, rowid FROM messages WHERE workspace_id = ?1 AND conversation_id = ?2 AND id = ?3", params![workspace_id, conversation_id, covered_message_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))).optional().map_err(|error| error.to_string())?;
        let Some((created_at, rowid)) = anchor else {
            return Err("summary coverage message not found".to_string());
        };
        let mut statement = tx.prepare("SELECT id FROM messages WHERE workspace_id = ?1 AND conversation_id = ?2 AND (created_at < ?3 OR (created_at = ?3 AND rowid <= ?4)) ORDER BY created_at ASC, rowid ASC").map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params![workspace_id, conversation_id, created_at, rowid],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };
    let source_message_ids_json =
        serde_json::to_string(&source_message_ids).map_err(|error| error.to_string())?;
    let timestamp = now();
    tx.execute("INSERT INTO conversation_summaries (id, workspace_id, conversation_id, summary, covered_message_id, source_message_ids_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)", params![Uuid::new_v4().to_string(), workspace_id, conversation_id, summary, covered_message_id, source_message_ids_json, timestamp]).map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}

pub fn get_draft(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<String, String> {
    conn.query_row(
        "SELECT content FROM conversation_drafts WHERE workspace_id = ?1 AND conversation_id = ?2",
        params![workspace_id, conversation_id],
        |row| row.get(0),
    )
    .optional()
    .map(|value| value.unwrap_or_default())
    .map_err(|error| error.to_string())
}

pub fn save_draft(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    content: &str,
) -> Result<(), String> {
    if !valid_draft_key(conn, workspace_id, conversation_id)? {
        return Err("conversation not found".to_string());
    }
    conn.execute("INSERT INTO conversation_drafts (workspace_id, conversation_id, content, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(workspace_id, conversation_id) DO UPDATE SET content = excluded.content, updated_at = excluded.updated_at", params![workspace_id, conversation_id, content, now()]).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn clear_draft(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM conversation_drafts WHERE workspace_id = ?1 AND conversation_id = ?2",
        params![workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub summary: String,
    pub covered_message_id: Option<String>,
    pub source_message_ids: Vec<String>,
}

pub fn latest_summary(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<Option<ConversationSummary>, String> {
    conn.query_row("SELECT summary, covered_message_id, source_message_ids_json FROM conversation_summaries WHERE workspace_id = ?1 AND conversation_id = ?2 ORDER BY updated_at DESC LIMIT 1", params![workspace_id, conversation_id], |row| {
        let source_json = row.get::<_, String>(2)?;
        let source_message_ids = serde_json::from_str(&source_json).map_err(|error| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error)))?;
        Ok(ConversationSummary { summary: row.get(0)?, covered_message_id: row.get(1)?, source_message_ids })
    }).optional().map_err(|error| error.to_string())
}
