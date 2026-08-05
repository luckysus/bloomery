use super::logic::with_session;
use crate::db::DbState;

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
