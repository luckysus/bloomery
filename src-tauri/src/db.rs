use crate::auth::AuthState;
use crate::models::{
    CloudJob, CloudJobInput, Conversation, HistoryHit, Memory, MemoryInput, MemorySuggestion,
    Message,
};
use crate::retrieval::{compact_whitespace, search, SearchDocument};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::Manager;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ConversationSnapshotMessage {
    pub role: String,
    pub content: String,
    pub response_json: Option<String>,
}

pub struct DbState {
    conn: Mutex<Option<Connection>>,
}

impl Default for DbState {
    fn default() -> Self {
        Self {
            conn: Mutex::new(None),
        }
    }
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn current_user_id(auth: &tauri::State<AuthState>) -> Result<String, String> {
    auth.current_user_id()
}

fn snapshot_missing_start(existing_count: usize, incoming_count: usize) -> Option<usize> {
    if existing_count >= incoming_count {
        None
    } else {
        Some(existing_count)
    }
}

fn normalize_message_role(role: &str) -> Result<&'static str, String> {
    match role {
        "user" => Ok("user"),
        "agent" | "assistant" => Ok("agent"),
        "system" => Ok("system"),
        _ => Err("invalid message role".to_string()),
    }
}

pub(crate) fn with_conn<T>(
    db: &tauri::State<DbState>,
    f: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let guard = db.conn.lock().map_err(|_| "db state poisoned")?;
    let conn = guard.as_ref().ok_or("database not initialized")?;
    f(conn)
}

pub(crate) fn database_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("resolve app data dir failed: {err}"))?;
    fs::create_dir_all(&dir).map_err(|err| format!("create app data dir failed: {err}"))?;
    Ok(dir.join("bloomery.sqlite3"))
}

#[tauri::command]
pub fn db_init(app: tauri::AppHandle, db: tauri::State<DbState>) -> Result<(), String> {
    let path = database_path(&app)?;
    let conn = Connection::open(path).map_err(|err| format!("open sqlite failed: {err}"))?;
    conn.execute_batch(include_str!("schema.sql"))
        .map_err(|err| format!("initialize sqlite schema failed: {err}"))?;
    *db.conn.lock().map_err(|_| "db state poisoned")? = Some(conn);
    Ok(())
}

#[tauri::command]
pub fn list_conversations(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
) -> Result<Vec<Conversation>, String> {
    query_conversations(&auth, &db, false)
}

#[tauri::command]
pub fn list_archived_conversations(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
) -> Result<Vec<Conversation>, String> {
    query_conversations(&auth, &db, true)
}

fn query_conversations(
    auth: &tauri::State<AuthState>,
    db: &tauri::State<DbState>,
    archived: bool,
) -> Result<Vec<Conversation>, String> {
    let user_id = current_user_id(&auth)?;
    let archived_value = if archived { 1 } else { 0 };
    with_conn(&db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, created_at, updated_at, pinned, archived
                 FROM conversations
                 WHERE user_id = ?1 AND archived = ?2
                 ORDER BY pinned DESC, updated_at DESC",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params![user_id, archived_value], |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    pinned: row.get::<_, i64>(4)? != 0,
                    archived: row.get::<_, i64>(5)? != 0,
                })
            })
            .map_err(|err| err.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn create_conversation(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    title: String,
) -> Result<Conversation, String> {
    let user_id = current_user_id(&auth)?;
    let id = Uuid::new_v4().to_string();
    let ts = now();
    let title = if title.trim().is_empty() {
        "新对话".to_string()
    } else {
        title.trim().chars().take(80).collect()
    };
    with_conn(&db, |conn| {
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
            params![id, user_id, title, ts, ts],
        )
        .map_err(|err| err.to_string())?;
        Ok(Conversation {
            id,
            title,
            created_at: ts.clone(),
            updated_at: ts,
            pinned: false,
            archived: false,
        })
    })
}

#[tauri::command]
pub fn update_conversation_title(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
    title: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE user_id = ?3 AND id = ?4",
            params![title.trim(), ts, user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn update_conversation_pinned(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
    pinned: bool,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        update_conversation_pinned_for_user(conn, &user_id, &conversation_id, pinned)
    })
}

fn update_conversation_pinned_for_user(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
    pinned: bool,
) -> Result<(), String> {
    let ts = now();
    let pinned_value = if pinned { 1 } else { 0 };
    conn.execute(
        "UPDATE conversations SET pinned = ?1, updated_at = ?2 WHERE user_id = ?3 AND id = ?4",
        params![pinned_value, ts, user_id, conversation_id],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn archive_conversation(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        conn.execute(
            "UPDATE conversations SET archived = 1, updated_at = ?1 WHERE user_id = ?2 AND id = ?3",
            params![ts, user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn restore_conversation(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        conn.execute(
            "UPDATE conversations SET archived = 0, updated_at = ?1 WHERE user_id = ?2 AND id = ?3",
            params![ts, user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn delete_conversation_local(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        conn.execute(
            "DELETE FROM messages WHERE user_id = ?1 AND conversation_id = ?2",
            params![user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "DELETE FROM conversation_summaries WHERE user_id = ?1 AND conversation_id = ?2",
            params![user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "DELETE FROM conversation_drafts WHERE user_id = ?1 AND conversation_id = ?2",
            params![user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "DELETE FROM conversations WHERE user_id = ?1 AND id = ?2",
            params![user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn list_messages(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<Vec<Message>, String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, conversation_id, role, content, response_json, created_at
                 FROM messages
                 WHERE user_id = ?1 AND conversation_id = ?2
                 ORDER BY created_at ASC",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params![user_id, conversation_id], |row| {
                Ok(Message {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    response_json: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|err| err.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn search_history(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    query: String,
    conversation_id: Option<String>,
    exclude_current: Option<bool>,
    limit: Option<usize>,
) -> Result<Vec<HistoryHit>, String> {
    let user_id = current_user_id(&auth)?;
    let limit = limit.unwrap_or(8).clamp(1, 20);
    with_conn(&db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.conversation_id, c.title, m.role, m.content, m.created_at
                 FROM messages m
                 LEFT JOIN conversations c ON c.user_id = m.user_id AND c.id = m.conversation_id
                 WHERE m.user_id = ?1 AND TRIM(m.content) != ''
                 ORDER BY m.created_at DESC
                 LIMIT 400",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params![user_id], |row| {
                Ok(HistoryHit {
                    message_id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    conversation_title: row
                        .get::<_, Option<String>>(2)?
                        .unwrap_or_else(|| "本地对话".to_string()),
                    role: row.get(3)?,
                    content: row.get(4)?,
                    created_at: row.get(5)?,
                    score: 0.0,
                    snippet: String::new(),
                })
            })
            .map_err(|err| err.to_string())?;
        let mut hits = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())?;
        if let Some(conversation_id) = conversation_id.filter(|value| !value.trim().is_empty()) {
            if exclude_current.unwrap_or(false) {
                hits.retain(|hit| hit.conversation_id != conversation_id);
            } else {
                hits.retain(|hit| hit.conversation_id == conversation_id);
            }
        }
        rank_history_hits(&query, hits, limit)
    })
}

pub(crate) fn rank_history_hits(
    query: &str,
    hits: Vec<HistoryHit>,
    limit: usize,
) -> Result<Vec<HistoryHit>, String> {
    let docs = hits
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
    Ok(search(query, &docs, limit, 240)
        .into_iter()
        .filter_map(|ranked| {
            let mut hit = hits.get(ranked.index)?.clone();
            hit.score = ranked.score;
            hit.snippet = ranked.snippet;
            Some(hit)
        })
        .collect())
}

pub(crate) fn conversation_belongs_to_user(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM conversations WHERE user_id = ?1 AND id = ?2 LIMIT 1",
        params![user_id, conversation_id],
        |_| Ok(()),
    )
    .optional()
    .map(|value| value.is_some())
    .map_err(|err| err.to_string())
}

fn ensure_conversation_id_available_for_user(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    let owner: Option<String> = conn
        .query_row(
            "SELECT user_id FROM conversations WHERE id = ?1 LIMIT 1",
            params![conversation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    if let Some(owner) = owner {
        if owner != user_id {
            return Err("conversation does not belong to current user".to_string());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn append_message(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
    role: String,
    content: String,
    response_json: Option<String>,
) -> Result<Message, String> {
    let user_id = current_user_id(&auth)?;
    let conversation_id = conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        return Err("conversation_id is required".to_string());
    }
    if !matches!(role.as_str(), "user" | "assistant" | "agent" | "system") {
        return Err("invalid message role".to_string());
    }
    let id = Uuid::new_v4().to_string();
    let ts = now();
    with_conn(&db, |conn| {
        if !conversation_belongs_to_user(conn, &user_id, &conversation_id)? {
            return Err("conversation not found".to_string());
        }
        conn.execute(
            "INSERT INTO messages (id, user_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, user_id, conversation_id, role, content, response_json, ts],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE user_id = ?2 AND id = ?3",
            params![ts, user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(Message {
            id,
            conversation_id,
            role,
            content,
            response_json,
            created_at: ts,
        })
    })
}

#[tauri::command]
pub fn save_conversation_snapshot(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
    title: String,
    messages: Vec<ConversationSnapshotMessage>,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let conversation_id = conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        return Err("conversation_id is required".to_string());
    }
    let title = if title.trim().is_empty() {
        "New conversation".to_string()
    } else {
        title.trim().chars().take(80).collect()
    };
    let ts = now();
    with_conn(&db, |conn| {
        ensure_conversation_id_available_for_user(conn, &user_id, &conversation_id)?;
        conn.execute(
            "INSERT OR IGNORE INTO conversations (id, user_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
            params![conversation_id, user_id, title, ts, ts],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2, archived = 0 WHERE user_id = ?3 AND id = ?4",
            params![title, ts, user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        let existing_count = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE user_id = ?1 AND conversation_id = ?2",
                params![user_id, conversation_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| err.to_string())? as usize;
        let Some(start) = snapshot_missing_start(existing_count, messages.len()) else {
            return Ok(());
        };
        for message in messages.into_iter().skip(start) {
            if message.content.trim().is_empty() {
                continue;
            }
            let role = normalize_message_role(message.role.trim())?;
            conn.execute(
                "INSERT INTO messages (id, user_id, conversation_id, role, content, response_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    Uuid::new_v4().to_string(),
                    user_id,
                    conversation_id,
                    role,
                    message.content,
                    message.response_json,
                    now()
                ],
            )
            .map_err(|err| err.to_string())?;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn replace_message_after_edit(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    message_id: String,
    content: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        let anchor: Option<(String, String)> = conn
            .query_row(
                "SELECT conversation_id, created_at FROM messages WHERE user_id = ?1 AND id = ?2",
                params![user_id, message_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        let Some((conversation_id, created_at)) = anchor else {
            return Err("message not found".to_string());
        };
        conn.execute(
            "UPDATE messages SET content = ?1 WHERE user_id = ?2 AND id = ?3",
            params![content, user_id, message_id],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "DELETE FROM messages WHERE user_id = ?1 AND conversation_id = ?2 AND created_at > ?3",
            params![user_id, conversation_id, created_at],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE user_id = ?2 AND id = ?3",
            params![ts, user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn truncate_conversation_after_message(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    message_id: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        let anchor: Option<(String, String)> = conn
            .query_row(
                "SELECT conversation_id, created_at FROM messages WHERE user_id = ?1 AND id = ?2",
                params![user_id, message_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        let Some((conversation_id, created_at)) = anchor else {
            return Err("message not found".to_string());
        };
        conn.execute(
            "DELETE FROM messages WHERE user_id = ?1 AND conversation_id = ?2 AND created_at > ?3",
            params![user_id, conversation_id, created_at],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "DELETE FROM conversation_summaries WHERE user_id = ?1 AND conversation_id = ?2",
            params![user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE user_id = ?2 AND id = ?3",
            params![ts, user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn fork_conversation_from_message(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    message_id: String,
) -> Result<Conversation, String> {
    let user_id = current_user_id(&auth)?;
    let id = Uuid::new_v4().to_string();
    let ts = now();
    with_conn(&db, |conn| {
        let anchor: Option<(String, String, String)> = conn
            .query_row(
                "SELECT m.conversation_id, m.created_at, c.title
                 FROM messages m
                 LEFT JOIN conversations c ON c.user_id = m.user_id AND c.id = m.conversation_id
                 WHERE m.user_id = ?1 AND m.id = ?2",
                params![user_id, message_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    ))
                },
            )
            .optional()
            .map_err(|err| err.to_string())?;
        let Some((source_conversation_id, anchor_created_at, source_title)) = anchor else {
            return Err("message not found".to_string());
        };
        let title = format!("分叉 - {}", truncate_chars(source_title.trim(), 70));
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
            params![id, user_id, title, ts, ts],
        )
        .map_err(|err| err.to_string())?;

        let mut stmt = conn
            .prepare(
                "SELECT role, content, response_json, created_at
                 FROM messages
                 WHERE user_id = ?1 AND conversation_id = ?2 AND created_at <= ?3
                 ORDER BY created_at ASC",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(
                params![user_id, source_conversation_id, anchor_created_at],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .map_err(|err| err.to_string())?;
        for row in rows {
            let (role, content, response_json, created_at) = row.map_err(|err| err.to_string())?;
            conn.execute(
                "INSERT INTO messages (id, user_id, conversation_id, role, content, response_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![Uuid::new_v4().to_string(), user_id, id, role, content, response_json, created_at],
            )
            .map_err(|err| err.to_string())?;
        }

        Ok(Conversation {
            id,
            title,
            created_at: ts.clone(),
            updated_at: ts,
            pinned: false,
            archived: false,
        })
    })
}

#[tauri::command]
pub fn list_memories(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
) -> Result<Vec<Memory>, String> {
    query_memories(&auth, &db, None, false)
}

#[tauri::command]
pub fn list_archived_memories(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
) -> Result<Vec<Memory>, String> {
    query_memories(&auth, &db, None, true)
}

#[tauri::command]
pub fn search_memories(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    query: String,
) -> Result<Vec<Memory>, String> {
    query_memories(&auth, &db, Some(query), false)
}

#[tauri::command]
pub fn suggest_memories(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    limit: Option<usize>,
) -> Result<Vec<MemorySuggestion>, String> {
    let user_id = current_user_id(&auth)?;
    let limit = limit.unwrap_or(6).clamp(1, 12);
    with_conn(&db, |conn| {
        let existing = load_active_memory_text(conn, &user_id)?;
        let mut stmt = conn
            .prepare(
                "SELECT id, conversation_id, content, created_at
                 FROM messages
                 WHERE user_id = ?1 AND role = 'user' AND TRIM(content) != ''
                 ORDER BY created_at DESC
                 LIMIT 160",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params![user_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|err| err.to_string())?;

        let mut suggestions = Vec::new();
        let mut seen = Vec::<String>::new();
        for row in rows {
            let (message_id, conversation_id, content, created_at) =
                row.map_err(|err| err.to_string())?;
            let Some((statement, reason)) = extract_memory_statement(&content) else {
                continue;
            };
            let key = normalize_memory_suggestion_key(&statement);
            if key.is_empty()
                || seen.iter().any(|item| item == &key)
                || existing
                    .iter()
                    .any(|item| item.contains(&key) || key.contains(item))
            {
                continue;
            }
            seen.push(key);
            let memory_type = infer_memory_type(&statement);
            let scope = infer_memory_scope(&statement, memory_type);
            let title = truncate_chars(&statement, 64);
            suggestions.push(MemorySuggestion {
                id: format!("suggestion-{message_id}"),
                scope: scope.to_string(),
                r#type: memory_type.to_string(),
                title: title.clone(),
                description: compact_whitespace(&statement),
                body: format!(
                    "{}\n\n**Why:** Suggested from recent local desktop history ({reason}).\n**How to apply:** Treat this as durable guidance only after the user confirms it still applies.\n\nEvidence: conversation={conversation_id}, message={message_id}, created_at={created_at}",
                    statement.trim()
                ),
                tags_json: serde_json::json!(["suggested"]).to_string(),
                reason,
                evidence: format!("{conversation_id} / {created_at}"),
            });
            if suggestions.len() >= limit {
                break;
            }
        }
        Ok(suggestions)
    })
}

fn query_memories(
    auth: &tauri::State<AuthState>,
    db: &tauri::State<DbState>,
    query: Option<String>,
    archived: bool,
) -> Result<Vec<Memory>, String> {
    let user_id = current_user_id(auth)?;
    let needle = query.unwrap_or_default().to_lowercase();
    let archived_predicate = if archived {
        "archived_at IS NOT NULL"
    } else {
        "archived_at IS NULL"
    };
    with_conn(db, |conn| {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, scope, type, title, description, body, tags_json, enabled, archived_at, created_at, updated_at
                 FROM memories
                 WHERE user_id = ?1 AND {archived_predicate}
                 ORDER BY updated_at DESC"
            ))
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params![user_id], |row| {
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
                })
            })
            .map_err(|err| err.to_string())?;
        let mut memories = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())?;
        if !needle.is_empty() {
            let docs = memories
                .iter()
                .enumerate()
                .map(|(index, memory)| SearchDocument {
                    index,
                    text: [
                        memory.id.as_str(),
                        memory.scope.as_str(),
                        memory.r#type.as_str(),
                        memory.title.as_str(),
                        memory.description.as_str(),
                        memory.body.as_str(),
                        memory.tags_json.as_str(),
                    ]
                    .join("\n"),
                })
                .collect::<Vec<_>>();
            memories = search(&needle, &docs, 20, 240)
                .into_iter()
                .filter_map(|hit| memories.get(hit.index).cloned())
                .collect();
        }
        Ok(memories)
    })
}

fn load_active_memory_text(conn: &Connection, user_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT title, description, body, tags_json
             FROM memories
             WHERE user_id = ?1 AND archived_at IS NULL",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![user_id], |row| {
            Ok(normalize_memory_suggestion_key(
                &[
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ]
                .join(" "),
            ))
        })
        .map_err(|err| err.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

fn extract_memory_statement(content: &str) -> Option<(String, String)> {
    let text = compact_whitespace(content);
    let len = text.chars().count();
    if !(8..=420).contains(&len) {
        return None;
    }
    let lower = text.to_lowercase();
    let markers = [
        ("记住", "explicit remember request"),
        ("以后", "future-facing preference"),
        ("始终", "persistent working rule"),
        ("总是", "persistent working rule"),
        ("每次", "repeated workflow preference"),
        ("默认", "default behavior preference"),
        ("不要", "negative working preference"),
        ("偏好", "user preference"),
        ("规则", "durable rule"),
        ("约定", "project convention"),
        ("remember", "explicit remember request"),
        ("always", "persistent working rule"),
        ("never", "negative working preference"),
        ("prefer", "user preference"),
        ("preference", "user preference"),
        ("by default", "default behavior preference"),
    ];
    for (marker, reason) in markers {
        if let Some(index) = lower.find(marker) {
            let statement = text[index..]
                .trim_start_matches(marker)
                .trim_start_matches(|ch| matches!(ch, '：' | ':' | '-' | ' '))
                .trim()
                .to_string();
            if !statement.is_empty() {
                return Some((statement, reason.to_string()));
            }
        }
    }
    None
}

fn infer_memory_type(statement: &str) -> &'static str {
    let lower = statement.to_lowercase();
    if lower.contains("http://") || lower.contains("https://") || lower.contains("doi") {
        "reference"
    } else if has_any(
        &lower,
        &["不要", "回答", "回复", "always", "never", "始终", "总是"],
    ) {
        "feedback"
    } else if has_any(
        &lower,
        &[
            "项目",
            "课题",
            "repo",
            "仓库",
            "工艺",
            "钢",
            "合金",
            "热处理",
        ],
    ) {
        "project"
    } else {
        "user"
    }
}

fn infer_memory_scope(statement: &str, memory_type: &str) -> &'static str {
    let lower = statement.to_lowercase();
    if has_any(
        &lower,
        &[
            "钢",
            "合金",
            "热处理",
            "相变",
            "轧制",
            "淬火",
            "回火",
            "成分",
        ],
    ) {
        "domain"
    } else if memory_type == "project" {
        "project"
    } else {
        "global"
    }
}

fn has_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn normalize_memory_suggestion_key(value: &str) -> String {
    compact_whitespace(value).to_lowercase()
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut out = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        out.push('…');
    }
    out
}

#[tauri::command]
pub fn get_memory(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Memory>, String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        conn.query_row(
            "SELECT id, scope, type, title, description, body, tags_json, enabled, archived_at, created_at, updated_at
             FROM memories
             WHERE user_id = ?1 AND id = ?2",
            params![user_id, id],
            |row| {
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
                })
            },
        )
        .optional()
        .map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn save_memory(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    memory: MemoryInput,
) -> Result<Memory, String> {
    let user_id = current_user_id(&auth)?;
    if memory.title.trim().is_empty() || memory.body.trim().is_empty() {
        return Err("memory title and body are required".to_string());
    }
    let id = memory.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let ts = now();
    with_conn(&db, |conn| {
        let created_at: Option<String> = conn
            .query_row(
                "SELECT created_at FROM memories WHERE user_id = ?1 AND id = ?2",
                params![user_id, id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        let created_at = created_at.unwrap_or_else(|| ts.clone());
        if created_at == ts {
            conn.execute(
                "INSERT INTO memories (id, user_id, scope, type, title, description, body, tags_json, enabled, archived_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11)",
                params![
                    id,
                    user_id,
                    memory.scope,
                    memory.r#type,
                    memory.title.trim(),
                    memory.description,
                    memory.body.trim(),
                    memory.tags_json,
                    if memory.enabled { 1 } else { 0 },
                    created_at,
                    ts
                ],
            )
            .map_err(|err| err.to_string())?;
        } else {
            let changed = conn
                .execute(
                    "UPDATE memories
                     SET scope = ?1, type = ?2, title = ?3, description = ?4, body = ?5,
                         tags_json = ?6, enabled = ?7, archived_at = NULL, updated_at = ?8
                     WHERE user_id = ?9 AND id = ?10",
                    params![
                        memory.scope,
                        memory.r#type,
                        memory.title.trim(),
                        memory.description,
                        memory.body.trim(),
                        memory.tags_json,
                        if memory.enabled { 1 } else { 0 },
                        ts,
                        user_id,
                        id
                    ],
                )
                .map_err(|err| err.to_string())?;
            if changed == 0 {
                return Err("memory not found".to_string());
            }
        }
        Ok(Memory {
            id,
            scope: memory.scope,
            r#type: memory.r#type,
            title: memory.title.trim().to_string(),
            description: memory.description,
            body: memory.body.trim().to_string(),
            tags_json: memory.tags_json,
            enabled: memory.enabled,
            archived_at: None,
            created_at,
            updated_at: ts,
        })
    })
}

#[tauri::command]
pub fn archive_memory(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    id: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        conn.execute(
            "UPDATE memories SET archived_at = ?1, updated_at = ?1 WHERE user_id = ?2 AND id = ?3",
            params![ts, user_id, id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn restore_memory(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    id: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        conn.execute(
            "UPDATE memories SET archived_at = NULL, updated_at = ?1 WHERE user_id = ?2 AND id = ?3",
            params![ts, user_id, id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn get_conversation_summary(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<String, String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        conn.query_row(
            "SELECT summary FROM conversation_summaries
             WHERE user_id = ?1 AND conversation_id = ?2
             ORDER BY updated_at DESC LIMIT 1",
            params![user_id, conversation_id],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.unwrap_or_default())
        .map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn save_conversation_summary(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
    summary: String,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        save_conversation_summary_for_user(
            conn,
            &user_id,
            &conversation_id,
            &summary,
            covered_message_id,
        )
    })
}

fn save_conversation_summary_for_user(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
    summary: &str,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    if !conversation_belongs_to_user(conn, user_id, conversation_id)? {
        return Err("conversation not found".to_string());
    }
    let id = Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO conversation_summaries (id, user_id, conversation_id, summary, covered_message_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, user_id, conversation_id, summary, covered_message_id, ts, ts],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_conversation_draft(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<String, String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        conn.query_row(
            "SELECT content FROM conversation_drafts WHERE user_id = ?1 AND conversation_id = ?2",
            params![user_id, conversation_id],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.unwrap_or_default())
        .map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn save_conversation_draft(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
    content: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let conversation_id = conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        return Err("conversation_id is required".to_string());
    }
    if content.trim().is_empty() {
        return clear_conversation_draft(auth, db, conversation_id);
    }
    with_conn(&db, |conn| {
        save_conversation_draft_for_user(conn, &user_id, &conversation_id, &content)
    })
}

fn save_conversation_draft_for_user(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
    content: &str,
) -> Result<(), String> {
    if !is_new_conversation_draft_key(conversation_id)
        && !conversation_belongs_to_user(conn, user_id, conversation_id)?
    {
        return Err("conversation not found".to_string());
    }
    let ts = now();
    conn.execute(
        "INSERT INTO conversation_drafts (user_id, conversation_id, content, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id, conversation_id) DO UPDATE SET
           content = excluded.content,
           updated_at = excluded.updated_at",
        params![user_id, conversation_id, content, ts],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn is_new_conversation_draft_key(conversation_id: &str) -> bool {
    matches!(conversation_id, "__new__" | "__agent_new__")
}

#[tauri::command]
pub fn clear_conversation_draft(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        conn.execute(
            "DELETE FROM conversation_drafts WHERE user_id = ?1 AND conversation_id = ?2",
            params![user_id, conversation_id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn list_cloud_jobs(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
) -> Result<Vec<CloudJob>, String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, conversation_id, cloud_job_id, type, status, payload_json, result_json, created_at, updated_at
                 FROM cloud_jobs
                 WHERE user_id = ?1
                 ORDER BY updated_at DESC",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params![user_id], |row| {
                Ok(CloudJob {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    cloud_job_id: row.get(2)?,
                    r#type: row.get(3)?,
                    status: row.get(4)?,
                    payload_json: row.get(5)?,
                    result_json: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|err| err.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn save_cloud_job(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    job: CloudJobInput,
) -> Result<CloudJob, String> {
    let user_id = current_user_id(&auth)?;
    upsert_cloud_job_for_user(&db, &user_id, job)
}

pub(crate) fn upsert_cloud_job_for_user(
    db: &tauri::State<DbState>,
    user_id: &str,
    job: CloudJobInput,
) -> Result<CloudJob, String> {
    let ts = now();
    with_conn(&db, |conn| {
        let conversation_id =
            normalize_cloud_job_conversation_id(conn, user_id, job.conversation_id.clone())?;
        let id = match job.id.clone() {
            Some(id) => id,
            None => existing_cloud_job_id(conn, &user_id, &job)?
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
        };
        let existing: Option<(String, String, Option<String>)> = conn
            .query_row(
                "SELECT created_at, payload_json, result_json FROM cloud_jobs WHERE user_id = ?1 AND id = ?2",
                params![user_id, id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        let (created_at, existing_payload_json, existing_result_json) =
            existing.unwrap_or_else(|| (ts.clone(), "{}".to_string(), None));
        let payload_json = job.payload_json.clone().unwrap_or(existing_payload_json);
        let result_json = job.result_json.clone().or(existing_result_json);
        if created_at == ts {
            conn.execute(
                "INSERT INTO cloud_jobs (id, user_id, conversation_id, cloud_job_id, type, status, payload_json, result_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    user_id,
                    conversation_id.clone(),
                    job.cloud_job_id,
                    job.r#type,
                    job.status,
                    payload_json,
                    result_json,
                    created_at,
                    ts
                ],
            )
            .map_err(|err| err.to_string())?;
        } else {
            let changed = conn
                .execute(
                    "UPDATE cloud_jobs
                     SET conversation_id = ?1, cloud_job_id = ?2, type = ?3, status = ?4,
                         payload_json = ?5, result_json = ?6, updated_at = ?7
                     WHERE user_id = ?8 AND id = ?9",
                    params![
                        conversation_id.clone(),
                        job.cloud_job_id,
                        job.r#type,
                        job.status,
                        payload_json,
                        result_json,
                        ts,
                        user_id,
                        id
                    ],
                )
                .map_err(|err| err.to_string())?;
            if changed == 0 {
                return Err("cloud job not found".to_string());
            }
        }
        Ok(CloudJob {
            id,
            conversation_id,
            cloud_job_id: job.cloud_job_id,
            r#type: job.r#type,
            status: job.status,
            payload_json,
            result_json,
            created_at,
            updated_at: ts,
        })
    })
}

fn normalize_cloud_job_conversation_id(
    conn: &Connection,
    user_id: &str,
    conversation_id: Option<String>,
) -> Result<Option<String>, String> {
    let Some(conversation_id) = conversation_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if !conversation_belongs_to_user(conn, user_id, &conversation_id)? {
        return Err("conversation not found".to_string());
    }
    Ok(Some(conversation_id))
}

fn existing_cloud_job_id(
    conn: &Connection,
    user_id: &str,
    job: &CloudJobInput,
) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT id FROM cloud_jobs
         WHERE user_id = ?1 AND cloud_job_id = ?2 AND type = ?3
         ORDER BY updated_at DESC LIMIT 1",
        params![user_id, job.cloud_job_id, job.r#type],
        |row| row.get(0),
    )
    .optional()
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn update_cloud_job(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    id: String,
    status: String,
    result_json: Option<String>,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        conn.execute(
            "UPDATE cloud_jobs SET status = ?1, result_json = ?2, updated_at = ?3 WHERE user_id = ?4 AND id = ?5",
            params![status, result_json, ts, user_id, id],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn get_setting(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    key: String,
) -> Result<Option<String>, String> {
    let user_id = current_user_id(&auth)?;
    with_conn(&db, |conn| {
        conn.query_row(
            "SELECT value_json FROM settings WHERE user_id = ?1 AND key = ?2",
            params![user_id, key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn set_setting(
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    key: String,
    value_json: String,
) -> Result<(), String> {
    let user_id = current_user_id(&auth)?;
    let ts = now();
    with_conn(&db, |conn| {
        conn.execute(
            "INSERT INTO settings (user_id, key, value_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_id, key) DO UPDATE SET
               value_json = excluded.value_json,
               updated_at = excluded.updated_at",
            params![user_id, key, value_json, ts],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn export_diagnostics(
    app: tauri::AppHandle,
    auth: tauri::State<AuthState>,
    db: tauri::State<DbState>,
    last_error_kind: Option<String>,
) -> Result<serde_json::Value, String> {
    let user_id = auth.current_user_id().ok();
    let path = database_path(&app)?;
    let metadata = fs::metadata(&path).ok();
    let counts = if let Some(user_id) = user_id.as_deref() {
        with_conn(&db, |conn| diagnostic_table_counts(conn, user_id))?
    } else {
        serde_json::json!({})
    };
    let cloud_api_base_configured = if let Some(user_id) = user_id.as_deref() {
        with_conn(&db, |conn| {
            conn.query_row(
                "SELECT value_json FROM settings WHERE user_id = ?1 AND key = 'cloud_api_base'",
                params![user_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|value| value.is_some_and(|raw| raw.trim_matches('"').trim().len() > 0))
            .map_err(|err| err.to_string())
        })?
    } else {
        false
    };
    Ok(serde_json::json!({
        "app": {
            "name": app.package_info().name,
            "version": app.package_info().version.to_string(),
        },
        "runtime": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "debug": cfg!(debug_assertions),
        },
        "local_storage": {
            "sqlite_exists": path.exists(),
            "sqlite_size_bytes": metadata.map(|item| item.len()).unwrap_or(0),
        },
        "settings": {
            "cloud_api_base_configured": cloud_api_base_configured,
        },
        "auth": {
            "authenticated": user_id.is_some(),
        },
        "counts": counts,
        "last_error_kind": last_error_kind.map(|value| sanitize_diagnostic_label(&value)).filter(|value| !value.is_empty()),
        "privacy": {
            "contains_message_content": false,
            "contains_memory_body": false,
            "contains_api_base": false,
            "contains_auth_token": false,
        },
        "generated_at": now(),
    }))
}

fn diagnostic_table_counts(conn: &Connection, user_id: &str) -> Result<serde_json::Value, String> {
    let count = |sql: &str| -> Result<i64, String> {
        conn.query_row(sql, params![user_id], |row| row.get(0))
            .map_err(|err| err.to_string())
    };
    Ok(serde_json::json!({
        "conversations_active": count("SELECT COUNT(*) FROM conversations WHERE user_id = ?1 AND archived = 0")?,
        "conversations_archived": count("SELECT COUNT(*) FROM conversations WHERE user_id = ?1 AND archived = 1")?,
        "messages": count("SELECT COUNT(*) FROM messages WHERE user_id = ?1")?,
        "memories_active": count("SELECT COUNT(*) FROM memories WHERE user_id = ?1 AND archived_at IS NULL")?,
        "memories_archived": count("SELECT COUNT(*) FROM memories WHERE user_id = ?1 AND archived_at IS NOT NULL")?,
        "conversation_drafts": count("SELECT COUNT(*) FROM conversation_drafts WHERE user_id = ?1")?,
        "cloud_jobs": count("SELECT COUNT(*) FROM cloud_jobs WHERE user_id = ?1")?,
        "settings": count("SELECT COUNT(*) FROM settings WHERE user_id = ?1")?,
    }))
}

fn sanitize_diagnostic_label(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.' | ' '))
        .take(80)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open memory sqlite");
        conn.execute_batch(include_str!("schema.sql"))
            .expect("schema");
        conn
    }

    #[test]
    fn existing_cloud_job_id_reuses_same_user_type_and_cloud_id() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO cloud_jobs (id, user_id, cloud_job_id, type, status, payload_json, created_at, updated_at)
             VALUES ('local-1', 'user-1', 'job-1', 'training', 'running', '{}', 't1', 't1')",
            [],
        )
        .expect("insert job");
        let job = CloudJobInput {
            id: None,
            conversation_id: None,
            cloud_job_id: "job-1".into(),
            r#type: "training".into(),
            status: "completed".into(),
            payload_json: None,
            result_json: None,
        };

        assert_eq!(
            existing_cloud_job_id(&conn, "user-1", &job).expect("lookup"),
            Some("local-1".into())
        );
    }

    #[test]
    fn existing_cloud_job_id_keeps_job_types_separate() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO cloud_jobs (id, user_id, cloud_job_id, type, status, payload_json, created_at, updated_at)
             VALUES ('local-1', 'user-1', 'job-1', 'training', 'running', '{}', 't1', 't1')",
            [],
        )
        .expect("insert job");
        let job = CloudJobInput {
            id: None,
            conversation_id: None,
            cloud_job_id: "job-1".into(),
            r#type: "literature".into(),
            status: "running".into(),
            payload_json: None,
            result_json: None,
        };

        assert_eq!(
            existing_cloud_job_id(&conn, "user-1", &job).expect("lookup"),
            None
        );
    }

    #[test]
    fn cloud_job_conversation_id_requires_owned_conversation() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
             VALUES ('c1', 'user-1', 'title', 't1', 't1')",
            [],
        )
        .expect("insert conversation");

        assert_eq!(
            normalize_cloud_job_conversation_id(&conn, "user-1", Some(" c1 ".to_string()))
                .expect("owned conversation"),
            Some("c1".to_string())
        );
        assert_eq!(
            normalize_cloud_job_conversation_id(&conn, "user-1", None).expect("none"),
            None
        );
        assert_eq!(
            normalize_cloud_job_conversation_id(&conn, "user-1", Some(" ".to_string()))
                .expect("blank"),
            None
        );

        let err = normalize_cloud_job_conversation_id(&conn, "user-2", Some("c1".to_string()))
            .expect_err("cross-user cloud job conversation should fail");
        assert!(err.contains("conversation not found"));
        assert!(
            normalize_cloud_job_conversation_id(&conn, "user-1", Some("missing".to_string()))
                .is_err()
        );
    }

    #[test]
    fn snapshot_missing_start_only_appends_tail() {
        assert_eq!(snapshot_missing_start(0, 2), Some(0));
        assert_eq!(snapshot_missing_start(1, 2), Some(1));
        assert_eq!(snapshot_missing_start(2, 2), None);
        assert_eq!(snapshot_missing_start(3, 2), None);
    }

    #[test]
    fn normalize_message_role_accepts_agent_and_assistant() {
        assert_eq!(normalize_message_role("user").expect("role"), "user");
        assert_eq!(normalize_message_role("agent").expect("role"), "agent");
        assert_eq!(normalize_message_role("assistant").expect("role"), "agent");
        assert!(normalize_message_role("tool").is_err());
    }

    #[test]
    fn conversation_belongs_to_user_is_user_scoped() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
             VALUES ('c1', 'user-1', 'title', 't1', 't1')",
            [],
        )
        .expect("insert conversation");

        assert!(conversation_belongs_to_user(&conn, "user-1", "c1").expect("owner lookup"));
        assert!(!conversation_belongs_to_user(&conn, "user-2", "c1").expect("other lookup"));
        assert!(!conversation_belongs_to_user(&conn, "user-1", "missing").expect("missing lookup"));
    }

    #[test]
    fn conversation_snapshot_rejects_cross_user_conversation_id() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
             VALUES ('c1', 'user-1', 'title', 't1', 't1')",
            [],
        )
        .expect("insert conversation");

        let err = ensure_conversation_id_available_for_user(&conn, "user-2", "c1")
            .expect_err("cross-user conversation id should fail");
        assert!(err.contains("current user"));
        ensure_conversation_id_available_for_user(&conn, "user-1", "c1").expect("same owner");
        ensure_conversation_id_available_for_user(&conn, "user-2", "missing").expect("new id");
    }

    #[test]
    fn conversation_summary_requires_user_owned_conversation() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
             VALUES ('c1', 'user-1', 'title', 't1', 't1')",
            [],
        )
        .expect("insert conversation");

        save_conversation_summary_for_user(&conn, "user-1", "c1", "summary", None)
            .expect("same owner summary");
        let err = save_conversation_summary_for_user(&conn, "user-2", "c1", "summary", None)
            .expect_err("cross-user summary should fail");
        assert!(err.contains("conversation not found"));
        assert!(
            save_conversation_summary_for_user(&conn, "user-1", "missing", "summary", None)
                .is_err()
        );

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM conversation_summaries", [], |row| {
                row.get(0)
            })
            .expect("summary count");
        assert_eq!(count, 1);
    }

    #[test]
    fn conversation_draft_requires_owned_conversation_except_new_keys() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
             VALUES ('c1', 'user-1', 'title', 't1', 't1')",
            [],
        )
        .expect("insert conversation");

        save_conversation_draft_for_user(&conn, "user-1", "c1", "draft").expect("owned draft");
        save_conversation_draft_for_user(&conn, "user-2", "__new__", "new draft")
            .expect("desktop new draft key");
        save_conversation_draft_for_user(&conn, "user-2", "__agent_new__", "agent draft")
            .expect("agent new draft key");

        let err = save_conversation_draft_for_user(&conn, "user-2", "c1", "bad draft")
            .expect_err("cross-user draft should fail");
        assert!(err.contains("conversation not found"));
        assert!(save_conversation_draft_for_user(&conn, "user-1", "missing", "bad draft").is_err());

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM conversation_drafts", [], |row| {
                row.get(0)
            })
            .expect("draft count");
        assert_eq!(count, 3);
    }

    #[test]
    fn update_conversation_pinned_is_user_scoped() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at, pinned)
             VALUES ('c1', 'user-1', 'title', 't1', 't1', 0)",
            [],
        )
        .expect("insert user conversation");
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at, pinned)
             VALUES ('c1-other', 'user-2', 'title', 't1', 't1', 0)",
            [],
        )
        .expect("insert other conversation");

        update_conversation_pinned_for_user(&conn, "user-1", "c1", true).expect("pin");

        let user_pinned: i64 = conn
            .query_row(
                "SELECT pinned FROM conversations WHERE user_id = 'user-1' AND id = 'c1'",
                [],
                |row| row.get(0),
            )
            .expect("user pinned");
        let other_pinned: i64 = conn
            .query_row(
                "SELECT pinned FROM conversations WHERE user_id = 'user-2' AND id = 'c1-other'",
                [],
                |row| row.get(0),
            )
            .expect("other pinned");

        assert_eq!(user_pinned, 1);
        assert_eq!(other_pinned, 0);
    }

    #[test]
    fn diagnostic_table_counts_only_return_counts() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at, archived)
             VALUES ('c1', 'user-1', 'secret customer title', 't1', 't1', 0)",
            [],
        )
        .expect("insert conversation");
        conn.execute(
            "INSERT INTO messages (id, user_id, conversation_id, role, content, created_at)
             VALUES ('m1', 'user-1', 'c1', 'user', 'secret message body', 't1')",
            [],
        )
        .expect("insert message");
        conn.execute(
            "INSERT INTO conversation_drafts (user_id, conversation_id, content, updated_at)
             VALUES ('user-1', 'c1', 'secret draft body', 't1')",
            [],
        )
        .expect("insert draft");

        let counts = diagnostic_table_counts(&conn, "user-1").expect("counts");
        assert_eq!(counts["conversations_active"], 1);
        assert_eq!(counts["messages"], 1);
        assert_eq!(counts["conversation_drafts"], 1);
        assert!(!counts.to_string().contains("secret"));
    }
}
