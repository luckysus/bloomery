use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::storage::repositories::settings;

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
