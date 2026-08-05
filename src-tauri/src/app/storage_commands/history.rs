use super::logic::with_session;
use crate::db::DbState;
use crate::models::{Conversation, ConversationSnapshotMessage};

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
