use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

pub fn get(conn: &Connection, workspace_id: &str, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value_json FROM settings WHERE workspace_id = ?1 AND key = ?2",
        params![workspace_id, key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn set(
    conn: &mut Connection,
    workspace_id: &str,
    key: &str,
    value_json: &str,
) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("setting key is required".to_string());
    }
    conn.execute(
        "INSERT INTO settings (workspace_id, key, value_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(workspace_id, key)
         DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![workspace_id, key, value_json, Utc::now().to_rfc3339()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
