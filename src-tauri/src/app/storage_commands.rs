use crate::agent::session::service::SessionService;
use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::models::{
    Conversation, ConversationSnapshotMessage, HistoryHit, Memory, MemoryInput, MemorySuggestion,
    Message,
};
use crate::storage::repositories::{memories, settings};

fn with_session<T>(
    db: &tauri::State<'_, DbState>,
    operation: impl FnOnce(&mut SessionService<'_>) -> Result<T, String>,
) -> Result<T, String> {
    with_conn_mut(db, |connection| {
        let mut session = SessionService::new(connection, current_workspace_id())?;
        operation(&mut session)
    })
}

#[tauri::command]
pub fn list_conversations(db: tauri::State<DbState>) -> Result<Vec<Conversation>, String> {
    with_session(&db, |session| session.list_conversations(false))
}

#[tauri::command]
pub fn list_archived_conversations(db: tauri::State<DbState>) -> Result<Vec<Conversation>, String> {
    with_session(&db, |session| session.list_conversations(true))
}

#[tauri::command]
pub fn create_conversation(
    db: tauri::State<DbState>,
    title: String,
) -> Result<Conversation, String> {
    with_session(&db, |session| session.create_conversation(&title))
}

#[tauri::command]
pub fn update_conversation_title(
    db: tauri::State<DbState>,
    conversation_id: String,
    title: String,
) -> Result<(), String> {
    with_session(&db, |session| {
        session.rename_conversation(&conversation_id, &title)
    })
}

#[tauri::command]
pub fn update_conversation_pinned(
    db: tauri::State<DbState>,
    conversation_id: String,
    pinned: bool,
) -> Result<(), String> {
    with_session(&db, |session| {
        session.set_conversation_pinned(&conversation_id, pinned)
    })
}

#[tauri::command]
pub fn archive_conversation(
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<(), String> {
    with_session(&db, |session| {
        session.archive_conversation(&conversation_id)
    })
}

#[tauri::command]
pub fn restore_conversation(
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<(), String> {
    with_session(&db, |session| {
        session.restore_conversation(&conversation_id)
    })
}

#[tauri::command]
pub fn delete_conversation_local(
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<(), String> {
    with_session(&db, |session| session.delete_conversation(&conversation_id))
}

#[tauri::command]
pub fn list_messages(
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<Vec<Message>, String> {
    with_session(&db, |session| session.list_messages(&conversation_id))
}

#[tauri::command]
pub fn search_history(
    db: tauri::State<DbState>,
    query: String,
    conversation_id: Option<String>,
    exclude_current: Option<bool>,
    limit: Option<usize>,
) -> Result<Vec<HistoryHit>, String> {
    with_session(&db, |session| {
        session.search_history(
            &query,
            conversation_id.as_deref(),
            exclude_current.unwrap_or(false),
            limit.unwrap_or(8),
        )
    })
}

#[tauri::command]
pub fn append_message(
    db: tauri::State<DbState>,
    conversation_id: String,
    role: String,
    content: String,
    response_json: Option<String>,
) -> Result<Message, String> {
    with_session(&db, |session| {
        session.append_message(&conversation_id, &role, &content, response_json)
    })
}

#[tauri::command]
pub fn save_conversation_snapshot(
    db: tauri::State<DbState>,
    conversation_id: String,
    title: String,
    messages: Vec<ConversationSnapshotMessage>,
) -> Result<(), String> {
    with_session(&db, |session| {
        session.import_snapshot(&conversation_id, &title, messages)
    })
}

#[tauri::command]
pub fn replace_message_after_edit(
    db: tauri::State<DbState>,
    message_id: String,
    content: String,
) -> Result<(), String> {
    with_session(&db, |session| {
        session.edit_message_and_truncate(&message_id, &content)
    })
}

#[tauri::command]
pub fn truncate_conversation_after_message(
    db: tauri::State<DbState>,
    message_id: String,
) -> Result<(), String> {
    with_session(&db, |session| session.truncate_after_message(&message_id))
}

#[tauri::command]
pub fn fork_conversation_from_message(
    db: tauri::State<DbState>,
    message_id: String,
) -> Result<Conversation, String> {
    with_session(&db, |session| {
        session.fork_conversation_from_message(&message_id)
    })
}

#[tauri::command]
pub fn list_memories(db: tauri::State<DbState>) -> Result<Vec<Memory>, String> {
    with_conn(&db, |conn| {
        memories::list(conn, current_workspace_id(), false, "")
    })
}

#[tauri::command]
pub fn list_archived_memories(db: tauri::State<DbState>) -> Result<Vec<Memory>, String> {
    with_conn(&db, |conn| {
        memories::list(conn, current_workspace_id(), true, "")
    })
}

#[tauri::command]
pub fn search_memories(db: tauri::State<DbState>, query: String) -> Result<Vec<Memory>, String> {
    with_conn(&db, |conn| {
        memories::list(conn, current_workspace_id(), false, &query)
    })
}

#[tauri::command]
pub fn suggest_memories(
    db: tauri::State<DbState>,
    limit: Option<usize>,
) -> Result<Vec<MemorySuggestion>, String> {
    with_conn(&db, |conn| {
        memories::suggest(conn, current_workspace_id(), limit.unwrap_or(6))
    })
}

#[tauri::command]
pub fn get_memory(db: tauri::State<DbState>, id: String) -> Result<Option<Memory>, String> {
    with_conn(&db, |conn| memories::get(conn, current_workspace_id(), &id))
}

#[tauri::command]
pub fn save_memory(db: tauri::State<DbState>, memory: MemoryInput) -> Result<Memory, String> {
    with_conn_mut(&db, |conn| {
        memories::save(conn, current_workspace_id(), memory)
    })
}

#[tauri::command]
pub fn archive_memory(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    with_conn_mut(&db, |conn| {
        memories::archive(conn, current_workspace_id(), &id)
    })
}

#[tauri::command]
pub fn restore_memory(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    with_conn_mut(&db, |conn| {
        memories::restore(conn, current_workspace_id(), &id)
    })
}

#[tauri::command]
pub fn confirm_memory_candidate(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    with_conn_mut(&db, |conn| {
        memories::confirm_candidate(conn, current_workspace_id(), &id)
    })
}

#[tauri::command]
pub fn reject_memory_candidate(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    with_conn_mut(&db, |conn| {
        memories::reject_candidate(conn, current_workspace_id(), &id)
    })
}

#[tauri::command]
pub fn set_memory_enabled(
    db: tauri::State<DbState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    with_conn_mut(&db, |conn| {
        memories::set_enabled(conn, current_workspace_id(), &id, enabled)
    })
}

#[tauri::command]
pub fn delete_memory(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    with_conn_mut(&db, |conn| {
        memories::delete(conn, current_workspace_id(), &id)
    })
}

#[tauri::command]
pub fn get_conversation_summary(
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<String, String> {
    with_session(&db, |session| session.load_summary(&conversation_id))
}

#[tauri::command]
pub fn save_conversation_summary(
    db: tauri::State<DbState>,
    conversation_id: String,
    summary: String,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    with_session(&db, |session| {
        session.save_summary(&conversation_id, &summary, covered_message_id)
    })
}

#[tauri::command]
pub fn get_conversation_draft(
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<String, String> {
    with_session(&db, |session| session.load_draft(&conversation_id))
}

#[tauri::command]
pub fn save_conversation_draft(
    db: tauri::State<DbState>,
    conversation_id: String,
    content: String,
) -> Result<(), String> {
    if conversation_id.trim().is_empty() {
        return Err("conversation_id is required".to_string());
    }
    with_session(&db, |session| {
        session.save_draft(&conversation_id, &content)
    })
}

#[tauri::command]
pub fn clear_conversation_draft(
    db: tauri::State<DbState>,
    conversation_id: String,
) -> Result<(), String> {
    with_session(&db, |session| session.clear_draft(&conversation_id))
}

#[tauri::command]
pub fn get_setting(db: tauri::State<DbState>, key: String) -> Result<Option<String>, String> {
    with_conn(&db, |conn| {
        settings::get(conn, current_workspace_id(), &key)
    })
}

#[tauri::command]
pub fn set_setting(
    db: tauri::State<DbState>,
    key: String,
    value_json: String,
) -> Result<(), String> {
    with_conn_mut(&db, |conn| {
        settings::set(conn, current_workspace_id(), &key, &value_json)
    })
}
