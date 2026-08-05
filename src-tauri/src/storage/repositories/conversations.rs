mod basic;
mod edits;
mod messages;
mod summary;

pub use basic::{
    belongs_to_workspace, create, delete, get, list, set_archived, set_pinned, update_title,
};
pub use edits::{
    fork_from_anchor, fork_from_message, replace_after_edit, save_snapshot, truncate_after_message,
};
pub use messages::{
    append_message, append_message_in_transaction, list_messages, rank_history_hits, search_history,
};
pub use summary::{
    clear_draft, get_draft, get_summary, latest_summary, save_draft, save_summary,
    ConversationSummary,
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};

pub(super) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(super) fn title(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.chars().take(80).collect()
    }
}

pub(super) fn normalize_role(role: &str) -> Result<&'static str, String> {
    match role.trim() {
        "user" => Ok("user"),
        "agent" | "assistant" => Ok("agent"),
        "system" => Ok("system"),
        _ => Err("invalid message role".to_string()),
    }
}

pub(super) fn changed(result: usize, entity: &str) -> Result<(), String> {
    if result == 0 {
        Err(format!("{entity} not found"))
    } else {
        Ok(())
    }
}

pub(super) fn ensure_id_available(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    let owner = conn
        .query_row(
            "SELECT workspace_id FROM conversations WHERE id = ?1 LIMIT 1",
            params![conversation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match owner {
        Some(owner) if owner != workspace_id => Err("conversation not found".to_string()),
        _ => Ok(()),
    }
}

pub(super) fn message_anchor(
    tx: &Transaction<'_>,
    workspace_id: &str,
    message_id: &str,
) -> Result<(String, String, i64, String), String> {
    tx.query_row(
        "SELECT conversation_id, created_at, rowid, role FROM messages
         WHERE workspace_id = ?1 AND id = ?2",
        params![workspace_id, message_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "message not found".to_string())
}

pub(super) fn valid_draft_key(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<bool, String> {
    if matches!(conversation_id, "__new__" | "__agent_new__") {
        Ok(true)
    } else {
        belongs_to_workspace(conn, workspace_id, conversation_id)
    }
}
