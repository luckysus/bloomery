use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::models::{Memory, MemoryInput, MemorySuggestion};
use crate::storage::repositories::memories;

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
