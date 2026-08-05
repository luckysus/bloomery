use super::{changed, message_anchor, normalize_role, now, title};
use crate::models::{Conversation, ConversationSnapshotMessage};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

pub fn save_snapshot(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    requested_title: &str,
    messages: Vec<ConversationSnapshotMessage>,
) -> Result<(), String> {
    if conversation_id.trim().is_empty() {
        return Err("conversation_id is required".to_string());
    }
    let timestamp = now();
    let title = title(requested_title, "New conversation");
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    if !super::basic::belongs_to_workspace(&tx, workspace_id, conversation_id)? {
        return Err("conversation not found".to_string());
    }
    changed(tx.execute("UPDATE conversations SET title = ?1, updated_at = ?2, archived = 0 WHERE workspace_id = ?3 AND id = ?4", params![title, timestamp, workspace_id, conversation_id]).map_err(|error| error.to_string())?, "conversation")?;
    let existing = tx
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE workspace_id = ?1 AND conversation_id = ?2",
            params![workspace_id, conversation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())? as usize;
    for message in messages.into_iter().skip(existing) {
        if message.content.trim().is_empty() {
            continue;
        }
        tx.execute("INSERT INTO messages (id, workspace_id, conversation_id, role, content, response_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![Uuid::new_v4().to_string(), workspace_id, conversation_id, normalize_role(&message.role)?, message.content, message.response_json, now()]).map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

pub fn replace_after_edit(
    conn: &mut Connection,
    workspace_id: &str,
    message_id: &str,
    content: &str,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let (conversation_id, created_at, rowid, role) = message_anchor(&tx, workspace_id, message_id)?;
    if role != "user" {
        return Err("only user messages can be edited".to_string());
    }
    tx.execute(
        "UPDATE messages SET content = ?1 WHERE workspace_id = ?2 AND id = ?3",
        params![content, workspace_id, message_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM agent_runs WHERE workspace_id = ?1 AND conversation_id = ?2 AND user_message_id = ?3", params![workspace_id, conversation_id, message_id]).map_err(|error| error.to_string())?;
    delete_after(&tx, workspace_id, &conversation_id, &created_at, rowid)?;
    tx.execute(
        "DELETE FROM conversation_summaries WHERE workspace_id = ?1 AND conversation_id = ?2",
        params![workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())?;
    touch(&tx, workspace_id, &conversation_id)?;
    tx.commit().map_err(|error| error.to_string())
}

pub fn truncate_after_message(
    conn: &mut Connection,
    workspace_id: &str,
    message_id: &str,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let (conversation_id, created_at, rowid, role) = message_anchor(&tx, workspace_id, message_id)?;
    if role == "user" {
        tx.execute("DELETE FROM agent_runs WHERE workspace_id = ?1 AND conversation_id = ?2 AND user_message_id = ?3", params![workspace_id, conversation_id, message_id]).map_err(|error| error.to_string())?;
    }
    delete_after(&tx, workspace_id, &conversation_id, &created_at, rowid)?;
    tx.execute(
        "DELETE FROM conversation_summaries WHERE workspace_id = ?1 AND conversation_id = ?2",
        params![workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())?;
    touch(&tx, workspace_id, &conversation_id)?;
    tx.commit().map_err(|error| error.to_string())
}

pub fn fork_from_message(
    conn: &mut Connection,
    workspace_id: &str,
    source_conversation_id: &str,
    message_id: &str,
    requested_title: &str,
) -> Result<Conversation, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let (anchor_conversation_id, anchor_created_at, anchor_rowid, _) =
        message_anchor(&tx, workspace_id, message_id)?;
    if anchor_conversation_id != source_conversation_id {
        return Err("message does not belong to conversation".to_string());
    }
    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    let title = title(requested_title, "Fork");
    tx.execute("INSERT INTO conversations (id, workspace_id, title, created_at, updated_at, pinned, archived) VALUES (?1, ?2, ?3, ?4, ?4, 0, 0)", params![id, workspace_id, title, timestamp]).map_err(|error| error.to_string())?;
    let source_messages = {
        let mut stmt = tx.prepare("SELECT role, content, response_json, created_at FROM messages WHERE workspace_id = ?1 AND conversation_id = ?2 AND (created_at < ?3 OR (created_at = ?3 AND rowid <= ?4)) ORDER BY created_at ASC, rowid ASC").map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(
                params![
                    workspace_id,
                    source_conversation_id,
                    anchor_created_at,
                    anchor_rowid
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    for (role, content, response_json, created_at) in source_messages {
        tx.execute("INSERT INTO messages (id, workspace_id, conversation_id, role, content, response_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![Uuid::new_v4().to_string(), workspace_id, id, role, content, response_json, created_at]).map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(Conversation {
        id,
        title,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        pinned: false,
        archived: false,
    })
}

pub fn fork_from_anchor(
    conn: &mut Connection,
    workspace_id: &str,
    message_id: &str,
) -> Result<Conversation, String> {
    let (source_conversation_id, source_title) = conn.query_row("SELECT m.conversation_id, c.title FROM messages m JOIN conversations c ON c.workspace_id = m.workspace_id AND c.id = m.conversation_id WHERE m.workspace_id = ?1 AND m.id = ?2", params![workspace_id, message_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).optional().map_err(|error| error.to_string())?.ok_or_else(|| "message not found".to_string())?;
    let fork_title = format!(
        "Fork - {}",
        source_title.trim().chars().take(70).collect::<String>()
    );
    fork_from_message(
        conn,
        workspace_id,
        &source_conversation_id,
        message_id,
        &fork_title,
    )
}

fn delete_after(
    tx: &Transaction<'_>,
    workspace_id: &str,
    conversation_id: &str,
    created_at: &str,
    rowid: i64,
) -> Result<(), String> {
    tx.execute("DELETE FROM messages WHERE workspace_id = ?1 AND conversation_id = ?2 AND (created_at > ?3 OR (created_at = ?3 AND rowid > ?4))", params![workspace_id, conversation_id, created_at, rowid]).map_err(|error| error.to_string()).map(|_| ())
}

fn touch(tx: &Transaction<'_>, workspace_id: &str, conversation_id: &str) -> Result<(), String> {
    tx.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE workspace_id = ?2 AND id = ?3",
        params![now(), workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())
    .map(|_| ())
}
