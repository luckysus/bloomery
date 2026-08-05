use super::{changed, now, title};
use crate::models::Conversation;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn list(
    conn: &Connection,
    workspace_id: &str,
    archived: bool,
) -> Result<Vec<Conversation>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, created_at, updated_at, pinned, archived
             FROM conversations WHERE workspace_id = ?1 AND archived = ?2
             ORDER BY pinned DESC, updated_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![workspace_id, i64::from(archived)], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                pinned: row.get::<_, i64>(4)? != 0,
                archived: row.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn get(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<Option<Conversation>, String> {
    conn.query_row(
        "SELECT id, title, created_at, updated_at, pinned, archived
         FROM conversations WHERE workspace_id = ?1 AND id = ?2",
        params![workspace_id, conversation_id],
        |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                pinned: row.get::<_, i64>(4)? != 0,
                archived: row.get::<_, i64>(5)? != 0,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn create(
    conn: &mut Connection,
    workspace_id: &str,
    requested_title: &str,
) -> Result<Conversation, String> {
    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    let title = title(requested_title, "New conversation");
    conn.execute(
        "INSERT INTO conversations
         (id, workspace_id, title, created_at, updated_at, pinned, archived)
         VALUES (?1, ?2, ?3, ?4, ?4, 0, 0)",
        params![id, workspace_id, title, timestamp],
    )
    .map_err(|error| error.to_string())?;
    Ok(Conversation {
        id,
        title,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        pinned: false,
        archived: false,
    })
}

pub fn update_title(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    requested_title: &str,
) -> Result<(), String> {
    changed(
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2
             WHERE workspace_id = ?3 AND id = ?4",
            params![
                title(requested_title, "New conversation"),
                now(),
                workspace_id,
                conversation_id
            ],
        )
        .map_err(|error| error.to_string())?,
        "conversation",
    )
}

pub fn set_pinned(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    pinned: bool,
) -> Result<(), String> {
    changed(
        conn.execute(
            "UPDATE conversations SET pinned = ?1, updated_at = ?2
             WHERE workspace_id = ?3 AND id = ?4",
            params![i64::from(pinned), now(), workspace_id, conversation_id],
        )
        .map_err(|error| error.to_string())?,
        "conversation",
    )
}

pub fn set_archived(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    archived: bool,
) -> Result<(), String> {
    changed(
        conn.execute(
            "UPDATE conversations SET archived = ?1, updated_at = ?2
             WHERE workspace_id = ?3 AND id = ?4",
            params![i64::from(archived), now(), workspace_id, conversation_id],
        )
        .map_err(|error| error.to_string())?,
        "conversation",
    )
}

pub fn delete(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    for table in ["messages", "conversation_summaries", "conversation_drafts"] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE workspace_id = ?1 AND conversation_id = ?2"),
            params![workspace_id, conversation_id],
        )
        .map_err(|error| error.to_string())?;
    }
    changed(
        tx.execute(
            "DELETE FROM conversations WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, conversation_id],
        )
        .map_err(|error| error.to_string())?,
        "conversation",
    )?;
    tx.commit().map_err(|error| error.to_string())
}

pub fn belongs_to_workspace(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM conversations WHERE workspace_id = ?1 AND id = ?2 LIMIT 1",
        params![workspace_id, conversation_id],
        |_| Ok(()),
    )
    .optional()
    .map(|value| value.is_some())
    .map_err(|error| error.to_string())
}
