use super::logic::with_session;
use crate::db::DbState;
use crate::models::{Conversation, HistoryHit, Message};

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
