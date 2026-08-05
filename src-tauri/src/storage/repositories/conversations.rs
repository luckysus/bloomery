use crate::models::{Conversation, ConversationSnapshotMessage, HistoryHit, Message};
use crate::retrieval::{search, SearchDocument};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn title(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.chars().take(80).collect()
    }
}

fn normalize_role(role: &str) -> Result<&'static str, String> {
    match role.trim() {
        "user" => Ok("user"),
        "agent" | "assistant" => Ok("agent"),
        "system" => Ok("system"),
        _ => Err("invalid message role".to_string()),
    }
}

fn changed(result: usize, entity: &str) -> Result<(), String> {
    if result == 0 {
        Err(format!("{entity} not found"))
    } else {
        Ok(())
    }
}

pub fn list(
    conn: &Connection,
    workspace_id: &str,
    archived: bool,
) -> Result<Vec<Conversation>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, created_at, updated_at, pinned, archived
             FROM conversations
             WHERE workspace_id = ?1 AND archived = ?2
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

pub fn list_messages(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<Vec<Message>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, role, content, response_json, created_at
             FROM messages
             WHERE workspace_id = ?1 AND conversation_id = ?2
             ORDER BY created_at ASC, rowid ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![workspace_id, conversation_id], |row| {
            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                response_json: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn search_history(
    conn: &Connection,
    workspace_id: &str,
    query: &str,
    conversation_id: Option<&str>,
    exclude_current: bool,
    limit: usize,
) -> Result<Vec<HistoryHit>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.conversation_id, c.title, m.role, m.content, m.created_at
             FROM messages m
             LEFT JOIN conversations c
               ON c.workspace_id = m.workspace_id AND c.id = m.conversation_id
             WHERE m.workspace_id = ?1 AND TRIM(m.content) != ''
             ORDER BY m.created_at DESC
             LIMIT 400",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![workspace_id], |row| {
            Ok(HistoryHit {
                message_id: row.get(0)?,
                conversation_id: row.get(1)?,
                conversation_title: row
                    .get::<_, Option<String>>(2)?
                    .unwrap_or_else(|| "Local conversation".to_string()),
                role: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
                score: 0.0,
                snippet: String::new(),
            })
        })
        .map_err(|error| error.to_string())?;
    let mut hits = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if let Some(conversation_id) = conversation_id.filter(|value| !value.trim().is_empty()) {
        if exclude_current {
            hits.retain(|hit| hit.conversation_id != conversation_id);
        } else {
            hits.retain(|hit| hit.conversation_id == conversation_id);
        }
    }
    rank_history_hits(query, hits, limit.clamp(1, 20))
}

pub fn rank_history_hits(
    query: &str,
    hits: Vec<HistoryHit>,
    limit: usize,
) -> Result<Vec<HistoryHit>, String> {
    let documents = hits
        .iter()
        .enumerate()
        .map(|(index, hit)| SearchDocument {
            index,
            text: format!(
                "{}\n{}\n{}\n{}",
                hit.conversation_title, hit.role, hit.created_at, hit.content
            ),
        })
        .collect::<Vec<_>>();
    Ok(search(query, &documents, limit, 240)
        .into_iter()
        .filter_map(|ranked| {
            let mut hit = hits.get(ranked.index)?.clone();
            hit.score = ranked.score;
            hit.snippet = ranked.snippet;
            Some(hit)
        })
        .collect())
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

fn ensure_id_available(
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

pub fn append_message(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
    response_json: Option<String>,
) -> Result<Message, String> {
    if conversation_id.trim().is_empty() {
        return Err("conversation_id is required".to_string());
    }
    let id = Uuid::new_v4();
    let timestamp = now();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let message = append_message_in_transaction(
        &tx,
        workspace_id,
        conversation_id,
        id,
        role,
        content,
        response_json,
        &timestamp,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(message)
}

#[allow(clippy::too_many_arguments)]
pub fn append_message_in_transaction(
    tx: &Transaction<'_>,
    workspace_id: &str,
    conversation_id: &str,
    message_id: Uuid,
    role: &str,
    content: &str,
    response_json: Option<String>,
    timestamp: &str,
) -> Result<Message, String> {
    if conversation_id.trim().is_empty() {
        return Err("conversation_id is required".to_string());
    }
    let role = normalize_role(role)?;
    if !belongs_to_workspace(tx, workspace_id, conversation_id)? {
        ensure_id_available(tx, workspace_id, conversation_id)?;
        return Err("conversation not found".to_string());
    }
    let id = message_id.to_string();
    tx.execute(
        "INSERT INTO messages
         (id, workspace_id, conversation_id, role, content, response_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            workspace_id,
            conversation_id,
            role,
            content,
            response_json,
            timestamp
        ],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE workspace_id = ?2 AND id = ?3",
        params![timestamp, workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(Message {
        id,
        conversation_id: conversation_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        response_json,
        created_at: timestamp.to_string(),
    })
}

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
    if !belongs_to_workspace(&tx, workspace_id, conversation_id)? {
        return Err("conversation not found".to_string());
    }
    changed(
        tx.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2, archived = 0
             WHERE workspace_id = ?3 AND id = ?4",
            params![title, timestamp, workspace_id, conversation_id],
        )
        .map_err(|error| error.to_string())?,
        "conversation",
    )?;
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
        tx.execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                workspace_id,
                conversation_id,
                normalize_role(&message.role)?,
                message.content,
                message.response_json,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

fn message_anchor(
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
    tx.execute(
        "DELETE FROM agent_runs
         WHERE workspace_id = ?1 AND conversation_id = ?2 AND user_message_id = ?3",
        params![workspace_id, conversation_id, message_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM messages
         WHERE workspace_id = ?1 AND conversation_id = ?2
           AND (created_at > ?3 OR (created_at = ?3 AND rowid > ?4))",
        params![workspace_id, conversation_id, created_at, rowid],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM conversation_summaries WHERE workspace_id = ?1 AND conversation_id = ?2",
        params![workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE workspace_id = ?2 AND id = ?3",
        params![now(), workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())?;
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
        tx.execute(
            "DELETE FROM agent_runs
             WHERE workspace_id = ?1 AND conversation_id = ?2 AND user_message_id = ?3",
            params![workspace_id, conversation_id, message_id],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.execute(
        "DELETE FROM messages
         WHERE workspace_id = ?1 AND conversation_id = ?2
           AND (created_at > ?3 OR (created_at = ?3 AND rowid > ?4))",
        params![workspace_id, conversation_id, created_at, rowid],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM conversation_summaries WHERE workspace_id = ?1 AND conversation_id = ?2",
        params![workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE workspace_id = ?2 AND id = ?3",
        params![now(), workspace_id, conversation_id],
    )
    .map_err(|error| error.to_string())?;
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
    tx.execute(
        "INSERT INTO conversations
         (id, workspace_id, title, created_at, updated_at, pinned, archived)
         VALUES (?1, ?2, ?3, ?4, ?4, 0, 0)",
        params![id, workspace_id, title, timestamp],
    )
    .map_err(|error| error.to_string())?;
    let source_messages = {
        let mut stmt = tx
            .prepare(
                "SELECT role, content, response_json, created_at
                 FROM messages
                 WHERE workspace_id = ?1 AND conversation_id = ?2
                   AND (created_at < ?3 OR (created_at = ?3 AND rowid <= ?4))
                 ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(|error| error.to_string())?;
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
        tx.execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                workspace_id,
                id,
                role,
                content,
                response_json,
                created_at
            ],
        )
        .map_err(|error| error.to_string())?;
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

pub fn get_summary(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<String, String> {
    conn.query_row(
        "SELECT summary FROM conversation_summaries
         WHERE workspace_id = ?1 AND conversation_id = ?2
         ORDER BY updated_at DESC LIMIT 1",
        params![workspace_id, conversation_id],
        |row| row.get(0),
    )
    .optional()
    .map(|value| value.unwrap_or_default())
    .map_err(|error| error.to_string())
}

pub fn save_summary(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    summary: &str,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    if !belongs_to_workspace(&tx, workspace_id, conversation_id)? {
        ensure_id_available(&tx, workspace_id, conversation_id)?;
        return Err("conversation not found".to_string());
    }
    let source_message_ids = if let Some(covered_message_id) = covered_message_id.as_deref() {
        let anchor = tx
            .query_row(
                "SELECT created_at, rowid FROM messages
                 WHERE workspace_id = ?1 AND conversation_id = ?2 AND id = ?3",
                params![workspace_id, conversation_id, covered_message_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((created_at, rowid)) = anchor else {
            return Err("summary coverage message not found".to_string());
        };
        let mut statement = tx
            .prepare(
                "SELECT id FROM messages
                 WHERE workspace_id = ?1 AND conversation_id = ?2
                   AND (created_at < ?3 OR (created_at = ?3 AND rowid <= ?4))
                 ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(|error| error.to_string())?;
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
    tx.execute(
        "INSERT INTO conversation_summaries
         (id, workspace_id, conversation_id, summary, covered_message_id,
          source_message_ids_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            Uuid::new_v4().to_string(),
            workspace_id,
            conversation_id,
            summary,
            covered_message_id,
            source_message_ids_json,
            timestamp
        ],
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}

fn valid_draft_key(
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

pub fn get_draft(
    conn: &Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<String, String> {
    conn.query_row(
        "SELECT content FROM conversation_drafts
         WHERE workspace_id = ?1 AND conversation_id = ?2",
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
    conn.execute(
        "INSERT INTO conversation_drafts (workspace_id, conversation_id, content, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(workspace_id, conversation_id)
         DO UPDATE SET content = excluded.content, updated_at = excluded.updated_at",
        params![workspace_id, conversation_id, content, now()],
    )
    .map_err(|error| error.to_string())?;
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

pub fn fork_from_anchor(
    conn: &mut Connection,
    workspace_id: &str,
    message_id: &str,
) -> Result<Conversation, String> {
    let (source_conversation_id, source_title) = conn
        .query_row(
            "SELECT m.conversation_id, c.title
             FROM messages m
             JOIN conversations c
               ON c.workspace_id = m.workspace_id AND c.id = m.conversation_id
             WHERE m.workspace_id = ?1 AND m.id = ?2",
            params![workspace_id, message_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "message not found".to_string())?;
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
    conn.query_row(
        "SELECT summary, covered_message_id, source_message_ids_json
         FROM conversation_summaries
         WHERE workspace_id = ?1 AND conversation_id = ?2
         ORDER BY updated_at DESC
         LIMIT 1",
        params![workspace_id, conversation_id],
        |row| {
            let source_json = row.get::<_, String>(2)?;
            let source_message_ids = serde_json::from_str(&source_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(ConversationSummary {
                summary: row.get(0)?,
                covered_message_id: row.get(1)?,
                source_message_ids,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}
