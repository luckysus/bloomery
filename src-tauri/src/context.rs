use crate::db::{current_workspace_id, with_conn, DbState};
use crate::models::HistoryHit;
use crate::retrieval::{estimate_text_tokens, search, SearchDocument};
use crate::storage::repositories::conversations::{belongs_to_workspace, rank_history_hits};
use rusqlite::params;
use serde::Serialize;

const RECENT_MESSAGE_READ_LIMIT: i64 = 60;
const RECENT_MESSAGE_TOKEN_BUDGET: usize = 3200;
const RECENT_MESSAGE_CHAR_LIMIT: usize = 2500;
const CONVERSATION_SUMMARY_CHAR_LIMIT: usize = 4000;
const SELECTED_MEMORY_LIMIT: usize = 5;
const MEMORY_BODY_CHAR_LIMIT: usize = 1200;
const MEMORY_INDEX_LIMIT: usize = 80;
const MEMORY_INDEX_TITLE_CHAR_LIMIT: usize = 120;
const MEMORY_INDEX_DESCRIPTION_CHAR_LIMIT: usize = 240;
const MEMORY_INDEX_TAGS_CHAR_LIMIT: usize = 240;
const HISTORY_READ_LIMIT: i64 = 300;
const HISTORY_HIT_LIMIT: usize = 3;
const HISTORY_HIT_CHAR_LIMIT: usize = 900;

#[derive(Debug, Serialize)]
pub struct DesktopContextPacket {
    pub conversation_summary: String,
    pub recent_messages: Vec<serde_json::Value>,
    pub memory_index: Vec<serde_json::Value>,
    pub selected_memories: Vec<serde_json::Value>,
    pub history_hits: Vec<serde_json::Value>,
    pub desktop_meta: serde_json::Value,
}

#[derive(Clone)]
struct MemoryCandidate {
    id: String,
    scope: String,
    memory_type: String,
    title: String,
    description: String,
    body: String,
    tags_json: String,
    updated_at: String,
}

#[derive(Clone)]
struct RecentMessage {
    role: String,
    content: String,
    created_at: String,
}

#[tauri::command]
pub fn build_context_packet(
    db: tauri::State<DbState>,
    conversation_id: String,
    message: String,
) -> Result<DesktopContextPacket, String> {
    let workspace_id = current_workspace_id();
    let conversation_id = conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        return Err("conversation_id is required".to_string());
    }
    with_conn(&db, |conn| {
        build_context_packet_for_connection(conn, workspace_id, &conversation_id, &message)
    })
}

pub(crate) fn build_context_packet_for_connection(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    conversation_id: &str,
    message: &str,
) -> Result<DesktopContextPacket, String> {
    ensure_context_conversation_belongs_to_workspace(conn, workspace_id, conversation_id)?;
    let raw_conversation_summary = conn
        .query_row(
            "SELECT summary FROM conversation_summaries
                 WHERE workspace_id = ?1 AND conversation_id = ?2
                 ORDER BY updated_at DESC LIMIT 1",
            params![workspace_id, conversation_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default();
    let raw_conversation_summary_chars = raw_conversation_summary.chars().count();
    let conversation_summary = budget_conversation_summary(raw_conversation_summary.clone());
    let conversation_summary_truncated =
        raw_conversation_summary_chars > CONVERSATION_SUMMARY_CHAR_LIMIT;

    let recent_messages = load_recent_messages(conn, workspace_id, conversation_id)?;
    let (recent_messages, recent_tokens) = budget_recent_messages(recent_messages);

    let memories = load_enabled_memories(conn, workspace_id)?;
    let memory_index_total_count = memories.len();
    let memory_index = build_memory_index(&memories);
    let memory_index_tokens = memory_index
        .iter()
        .map(|value| estimate_text_tokens(&value.to_string()))
        .sum::<usize>();
    let selected_memories = select_memories(&memories, message);
    let selected_memory_tokens = selected_memories
        .iter()
        .map(|value| estimate_text_tokens(&value.to_string()))
        .sum::<usize>();

    let history_hits = load_history_hits(conn, workspace_id, conversation_id, message)?;
    let history_tokens = history_hits
        .iter()
        .map(|value| estimate_text_tokens(&value.to_string()))
        .sum::<usize>();
    let summary_tokens = estimate_text_tokens(&conversation_summary);
    let estimated_context_tokens = recent_tokens
        + selected_memory_tokens
        + memory_index_tokens
        + history_tokens
        + summary_tokens;
    let recent_message_count = recent_messages.len();
    let selected_memory_count = selected_memories.len();
    let memory_index_count = memory_index.len();
    let history_hit_count = history_hits.len();

    Ok(DesktopContextPacket {
        conversation_summary,
        recent_messages,
        memory_index,
        selected_memories,
        history_hits,
        desktop_meta: serde_json::json!({
            "client": "tauri",
            "context_version": 2,
            "conversation_id": conversation_id,
            "query_length": message.chars().count(),
            "budget_meta": {
                "recent_message_token_budget": RECENT_MESSAGE_TOKEN_BUDGET,
                "recent_message_count": recent_message_count,
                "selected_memory_count": selected_memory_count,
                "memory_index_count": memory_index_count,
                "memory_index_total_count": memory_index_total_count,
                "memory_index_truncated": memory_index_total_count > memory_index_count,
                "history_hit_count": history_hit_count,
                "estimated_context_tokens": estimated_context_tokens,
                "summary_tokens": summary_tokens,
                "conversation_summary_char_limit": CONVERSATION_SUMMARY_CHAR_LIMIT,
                "conversation_summary_truncated": conversation_summary_truncated,
                "memory_index_tokens": memory_index_tokens,
            }
        }),
    })
}

fn ensure_context_conversation_belongs_to_workspace(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    if !belongs_to_workspace(conn, workspace_id, conversation_id)? {
        return Err("conversation not found".to_string());
    }
    Ok(())
}

fn budget_conversation_summary(summary: String) -> String {
    truncate_chars(&summary, CONVERSATION_SUMMARY_CHAR_LIMIT)
}

fn load_recent_messages(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<Vec<RecentMessage>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT role, content, created_at FROM messages
             WHERE workspace_id = ?1 AND conversation_id = ?2
             ORDER BY created_at DESC LIMIT ?3",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(
            params![workspace_id, conversation_id, RECENT_MESSAGE_READ_LIMIT],
            |row| {
                Ok(RecentMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    created_at: row.get(2)?,
                })
            },
        )
        .map_err(|err| err.to_string())?;
    let mut messages = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    messages.reverse();
    Ok(messages)
}

fn budget_recent_messages(messages: Vec<RecentMessage>) -> (Vec<serde_json::Value>, usize) {
    let mut kept = Vec::new();
    let mut tokens = 0usize;
    for message in messages.into_iter().rev() {
        let content = truncate_chars(&message.content, RECENT_MESSAGE_CHAR_LIMIT);
        let cost = estimate_text_tokens(&content) + estimate_text_tokens(&message.role) + 4;
        if !kept.is_empty() && tokens + cost > RECENT_MESSAGE_TOKEN_BUDGET {
            break;
        }
        tokens += cost;
        kept.push(serde_json::json!({
            "role": message.role,
            "content": content,
            "created_at": message.created_at,
        }));
    }
    kept.reverse();
    (kept, tokens)
}

fn load_enabled_memories(
    conn: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<Vec<MemoryCandidate>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scope, type, title, description, body, tags_json, updated_at
             FROM memories
             WHERE workspace_id = ?1 AND enabled = 1 AND archived_at IS NULL
               AND status = 'confirmed'
             ORDER BY updated_at DESC",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![workspace_id], |row| {
            Ok(MemoryCandidate {
                id: row.get(0)?,
                scope: row.get(1)?,
                memory_type: row.get(2)?,
                title: row.get(3)?,
                description: row.get(4)?,
                body: row.get(5)?,
                tags_json: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|err| err.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

fn build_memory_index(memories: &[MemoryCandidate]) -> Vec<serde_json::Value> {
    memories
        .iter()
        .take(MEMORY_INDEX_LIMIT)
        .map(|memory| {
            serde_json::json!({
                "id": memory.id,
                "scope": memory.scope,
                "type": memory.memory_type,
                "title": truncate_chars(&memory.title, MEMORY_INDEX_TITLE_CHAR_LIMIT),
                "description": truncate_chars(&memory.description, MEMORY_INDEX_DESCRIPTION_CHAR_LIMIT),
                "tags_json": truncate_chars(&memory.tags_json, MEMORY_INDEX_TAGS_CHAR_LIMIT),
                "updated_at": memory.updated_at,
            })
        })
        .collect()
}

fn select_memories(memories: &[MemoryCandidate], query: &str) -> Vec<serde_json::Value> {
    let docs = memories
        .iter()
        .enumerate()
        .map(|(index, memory)| SearchDocument {
            index,
            text: format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n{}",
                memory.id,
                memory.scope,
                memory.memory_type,
                memory.title,
                memory.description,
                memory.tags_json,
                memory.body
            ),
        })
        .collect::<Vec<_>>();
    search(query, &docs, SELECTED_MEMORY_LIMIT, 260)
        .into_iter()
        .filter_map(|hit| {
            let memory = memories.get(hit.index)?;
            Some(serde_json::json!({
                "id": memory.id,
                "scope": memory.scope,
                "type": memory.memory_type,
                "title": truncate_chars(&memory.title, MEMORY_INDEX_TITLE_CHAR_LIMIT),
                "description": truncate_chars(&memory.description, MEMORY_INDEX_DESCRIPTION_CHAR_LIMIT),
                "body": truncate_chars(&memory.body, MEMORY_BODY_CHAR_LIMIT),
                "tags_json": truncate_chars(&memory.tags_json, MEMORY_INDEX_TAGS_CHAR_LIMIT),
                "score": hit.score,
                "snippet": hit.snippet,
            }))
        })
        .collect()
}

fn load_history_hits(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    conversation_id: &str,
    query: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.conversation_id, c.title, m.role, m.content, m.created_at
             FROM messages m
             LEFT JOIN conversations c ON c.workspace_id = m.workspace_id AND c.id = m.conversation_id
             WHERE m.workspace_id = ?1 AND m.conversation_id != ?2 AND TRIM(m.content) != ''
             ORDER BY m.created_at DESC
             LIMIT ?3",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(
            params![workspace_id, conversation_id, HISTORY_READ_LIMIT],
            |row| {
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
            },
        )
        .map_err(|err| err.to_string())?;
    let hits = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    Ok(rank_history_hits(query, hits, HISTORY_HIT_LIMIT)?
        .into_iter()
        .map(|hit| {
            serde_json::json!({
                "conversation_id": hit.conversation_id,
                "conversation_title": hit.conversation_title,
                "message_id": hit.message_id,
                "role": hit.role,
                "content": truncate_chars(&hit.content, HISTORY_HIT_CHAR_LIMIT),
                "created_at": hit.created_at,
                "score": hit.score,
                "snippet": hit.snippet,
            })
        })
        .collect())
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut result: String = value.chars().take(limit).collect();
    if value.chars().count() > limit {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn memory_conn() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open memory sqlite");
        crate::storage::migrations::migrate(&mut conn).expect("migrate schema");
        conn
    }

    #[test]
    fn context_packet_requires_user_owned_conversation() {
        let conn = memory_conn();
        conn.execute(
            "INSERT INTO conversations (id, workspace_id, title, created_at, updated_at)
             VALUES ('c1', 'user-1', 'title', 't1', 't1')",
            [],
        )
        .expect("insert conversation");

        ensure_context_conversation_belongs_to_workspace(&conn, "user-1", "c1")
            .expect("owned conversation");
        let err = ensure_context_conversation_belongs_to_workspace(&conn, "user-2", "c1")
            .expect_err("cross-user context should fail");
        assert!(err.contains("conversation not found"));
        assert!(
            ensure_context_conversation_belongs_to_workspace(&conn, "user-1", "missing").is_err()
        );
    }

    #[test]
    fn memory_index_is_bounded_and_does_not_include_bodies() {
        let long_text = "x".repeat(500);
        let memories = (0..100)
            .map(|index| MemoryCandidate {
                id: format!("memory-{index}"),
                scope: "global".to_string(),
                memory_type: "user".to_string(),
                title: long_text.clone(),
                description: long_text.clone(),
                body: "secret body must stay out of memory index".to_string(),
                tags_json: long_text.clone(),
                updated_at: format!("t{index}"),
            })
            .collect::<Vec<_>>();

        let index = build_memory_index(&memories);

        assert_eq!(index.len(), MEMORY_INDEX_LIMIT);
        assert_eq!(index[0]["id"], "memory-0");
        assert_eq!(index[MEMORY_INDEX_LIMIT - 1]["id"], "memory-79");
        assert!(index[0].get("body").is_none());
        assert!(
            index[0]["title"].as_str().expect("title").chars().count()
                <= MEMORY_INDEX_TITLE_CHAR_LIMIT + 1
        );
        assert!(
            index[0]["description"]
                .as_str()
                .expect("description")
                .chars()
                .count()
                <= MEMORY_INDEX_DESCRIPTION_CHAR_LIMIT + 1
        );
    }

    #[test]
    fn selected_memories_bound_all_text_fields() {
        let long_text = "Q355B ".repeat(400);
        let memories = vec![MemoryCandidate {
            id: "memory-1".to_string(),
            scope: "global".to_string(),
            memory_type: "domain".to_string(),
            title: long_text.clone(),
            description: long_text.clone(),
            body: long_text.clone(),
            tags_json: long_text,
            updated_at: "t1".to_string(),
        }];

        let selected = select_memories(&memories, "Q355B");

        assert_eq!(selected.len(), 1);
        assert!(
            selected[0]["title"]
                .as_str()
                .expect("title")
                .chars()
                .count()
                <= MEMORY_INDEX_TITLE_CHAR_LIMIT + 1
        );
        assert!(
            selected[0]["description"]
                .as_str()
                .expect("description")
                .chars()
                .count()
                <= MEMORY_INDEX_DESCRIPTION_CHAR_LIMIT + 1
        );
        assert!(
            selected[0]["tags_json"]
                .as_str()
                .expect("tags")
                .chars()
                .count()
                <= MEMORY_INDEX_TAGS_CHAR_LIMIT + 1
        );
        assert!(
            selected[0]["body"].as_str().expect("body").chars().count()
                <= MEMORY_BODY_CHAR_LIMIT + 1
        );
    }

    #[test]
    fn conversation_summary_is_bounded_before_context_injection() {
        let summary = "Q355B 热轧屈服波动。".repeat(1200);

        let budgeted = budget_conversation_summary(summary);

        assert!(budgeted.chars().count() <= CONVERSATION_SUMMARY_CHAR_LIMIT + 1);
        assert!(budgeted.contains('…'));
    }
}
