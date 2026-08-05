mod lifecycle;

pub use lifecycle::{
    capture_candidate, confirm_candidate, delete, list_context, reject_candidate, set_enabled,
};

use crate::agent::context::{extract_memory_candidate, normalize_memory_key};
use crate::models::{Memory, MemoryInput, MemorySuggestion};
use crate::retrieval::{compact_whitespace, search, SearchDocument};
use lifecycle::{now, row_to_memory};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn list(
    conn: &Connection,
    workspace_id: &str,
    archived: bool,
    query: &str,
) -> Result<Vec<Memory>, String> {
    let archived_predicate = if archived {
        "archived_at IS NOT NULL"
    } else {
        "archived_at IS NULL"
    };
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, scope, type, title, description, body, tags_json, enabled,
                    archived_at, created_at, updated_at, source_message_id, source_run_id,
                    confidence, status, dedup_key
             FROM memories
             WHERE workspace_id = ?1 AND {archived_predicate}
             ORDER BY updated_at DESC"
        ))
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![workspace_id], row_to_memory)
        .map_err(|error| error.to_string())?;
    let memories = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if query.trim().is_empty() {
        return Ok(memories);
    }
    let documents = memories
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
    Ok(search(query, &documents, 20, 240)
        .into_iter()
        .filter_map(|hit| memories.get(hit.index).cloned())
        .collect())
}

pub fn get(conn: &Connection, workspace_id: &str, id: &str) -> Result<Option<Memory>, String> {
    conn.query_row(
        "SELECT id, scope, type, title, description, body, tags_json, enabled,
                archived_at, created_at, updated_at, source_message_id, source_run_id,
                confidence, status, dedup_key
         FROM memories
         WHERE workspace_id = ?1 AND id = ?2",
        params![workspace_id, id],
        row_to_memory,
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn save(
    conn: &mut Connection,
    workspace_id: &str,
    memory: MemoryInput,
) -> Result<Memory, String> {
    let MemoryInput {
        id,
        scope,
        r#type: memory_type,
        title,
        description,
        body,
        tags_json,
        enabled,
    } = memory;
    let title = title.trim().to_string();
    let body = body.trim().to_string();
    if title.is_empty() || body.is_empty() {
        return Err("memory title and body are required".to_string());
    }
    let dedup_key = normalize_memory_key(&body);
    let id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = now();
    let exists = conn
        .query_row(
            "SELECT 1 FROM memories WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if exists {
        conn.execute(
            "UPDATE memories
             SET scope = ?1, type = ?2, title = ?3, description = ?4, body = ?5,
                 tags_json = ?6, enabled = ?7, archived_at = NULL, updated_at = ?8,
                 status = 'confirmed', dedup_key = ?9
             WHERE workspace_id = ?10 AND id = ?11",
            params![
                scope,
                memory_type,
                title,
                description,
                body,
                tags_json,
                i64::from(enabled),
                timestamp,
                dedup_key,
                workspace_id,
                id,
            ],
        )
        .map_err(|error| error.to_string())?;
    } else {
        conn.execute(
            "INSERT INTO memories
             (id, workspace_id, scope, type, title, description, body, tags_json,
              enabled, archived_at, created_at, updated_at, source_message_id,
              source_run_id, confidence, status, dedup_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?10,
                     NULL, NULL, 1.0, 'confirmed', ?11)",
            params![
                id,
                workspace_id,
                scope,
                memory_type,
                title,
                description,
                body,
                tags_json,
                i64::from(enabled),
                timestamp,
                dedup_key,
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    get(conn, workspace_id, &id)?.ok_or_else(|| "memory not found".to_string())
}

pub fn archive(conn: &mut Connection, workspace_id: &str, id: &str) -> Result<(), String> {
    let updated = conn
        .execute(
            "UPDATE memories SET archived_at = ?1, updated_at = ?1
             WHERE workspace_id = ?2 AND id = ?3",
            params![now(), workspace_id, id],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        Err("memory not found".to_string())
    } else {
        Ok(())
    }
}

pub fn restore(conn: &mut Connection, workspace_id: &str, id: &str) -> Result<(), String> {
    let updated = conn
        .execute(
            "UPDATE memories SET archived_at = NULL, updated_at = ?1
             WHERE workspace_id = ?2 AND id = ?3",
            params![now(), workspace_id, id],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        Err("memory not found".to_string())
    } else {
        Ok(())
    }
}

pub fn suggest(
    conn: &Connection,
    workspace_id: &str,
    limit: usize,
) -> Result<Vec<MemorySuggestion>, String> {
    let existing = load_active_memory_text(conn, workspace_id)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, content, created_at
             FROM messages
             WHERE workspace_id = ?1 AND role = 'user' AND TRIM(content) != ''
             ORDER BY created_at DESC
             LIMIT 160",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![workspace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut suggestions = Vec::new();
    let mut seen = Vec::<String>::new();
    for row in rows {
        let (message_id, conversation_id, content, created_at) =
            row.map_err(|error| error.to_string())?;
        let text = compact_whitespace(&content);
        if !(8..=420).contains(&text.chars().count()) {
            continue;
        }
        let Some(candidate) = extract_memory_candidate(&text, &message_id, None, 0.5)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        let key = candidate.dedup_key.clone();
        if key.is_empty()
            || seen.iter().any(|item| item == &key)
            || existing
                .iter()
                .any(|item| item.contains(&key) || key.contains(item))
        {
            continue;
        }
        seen.push(key);
        suggestions.push(MemorySuggestion {
            id: format!("suggestion-{message_id}"),
            scope: candidate.scope,
            r#type: candidate.memory_type,
            title: candidate.title,
            description: candidate.description,
            body: format!(
                "{}

**Why:** Suggested from recent local desktop history ({}).
**How to apply:** Treat this as durable guidance only after the user confirms it still applies.

Evidence: conversation={conversation_id}, message={message_id}, created_at={created_at}",
                candidate.body, candidate.reason
            ),
            tags_json: candidate.tags_json,
            reason: candidate.reason,
            evidence: format!("{conversation_id} / {created_at}"),
        });
        if suggestions.len() >= limit.clamp(1, 12) {
            break;
        }
    }
    Ok(suggestions)
}

fn load_active_memory_text(conn: &Connection, workspace_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT title, description, body, tags_json
             FROM memories
             WHERE workspace_id = ?1 AND archived_at IS NULL",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![workspace_id], |row| {
            Ok(normalize_memory_key(
                &[
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ]
                .join(" "),
            ))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
