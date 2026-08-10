use crate::db::{current_workspace_id, database_path, now, with_conn, DbState};
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};

#[tauri::command]
pub fn export_diagnostics(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    last_error_kind: Option<String>,
) -> Result<serde_json::Value, String> {
    let workspace_id = current_workspace_id();
    let path = database_path(&app)?;
    let metadata = fs::metadata(&path).ok();
    let counts = with_conn(&db, |conn| diagnostic_table_counts(conn, workspace_id))?;
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
        "counts": counts,
        "last_error_kind": last_error_kind
            .map(|value| sanitize_diagnostic_label(&value))
            .filter(|value| !value.is_empty()),
        "privacy": {
            "contains_message_content": false,
            "contains_memory_body": false,
            "contains_provider_endpoint": false,
            "contains_provider_secret": false,
        },
        "generated_at": now(),
    }))
}

fn write_json_atomically(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "diagnostics output path is invalid".to_string())?;
    let temporary = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|error| format!("serialize diagnostics failed: {error}"))?;
        fs::write(&temporary, bytes)
            .map_err(|error| format!("write diagnostics temporary file failed: {error}"))?;
        if path.exists() {
            fs::remove_file(path)
                .map_err(|error| format!("replace diagnostics file failed: {error}"))?;
        }
        fs::rename(&temporary, path)
            .map_err(|error| format!("finalize diagnostics file failed: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[tauri::command]
pub fn write_diagnostics_export(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    output_path: String,
    last_error_kind: Option<String>,
) -> Result<(), String> {
    let output_path = PathBuf::from(output_path.trim());
    if output_path.as_os_str().is_empty() {
        return Err("diagnostics output path is required".to_string());
    }
    let diagnostics = export_diagnostics(app, db, last_error_kind)?;
    write_json_atomically(&output_path, &diagnostics)
}

fn diagnostic_table_counts(
    conn: &Connection,
    workspace_id: &str,
) -> Result<serde_json::Value, String> {
    let count = |sql: &str| -> Result<i64, String> {
        conn.query_row(sql, params![workspace_id], |row| row.get(0))
            .map_err(|error| error.to_string())
    };
    Ok(serde_json::json!({
        "conversations_active": count(
            "SELECT COUNT(*) FROM conversations WHERE workspace_id = ?1 AND archived = 0"
        )?,
        "conversations_archived": count(
            "SELECT COUNT(*) FROM conversations WHERE workspace_id = ?1 AND archived = 1"
        )?,
        "messages": count("SELECT COUNT(*) FROM messages WHERE workspace_id = ?1")?,
        "memories_active": count(
            "SELECT COUNT(*) FROM memories WHERE workspace_id = ?1 AND archived_at IS NULL"
        )?,
        "memories_archived": count(
            "SELECT COUNT(*) FROM memories WHERE workspace_id = ?1 AND archived_at IS NOT NULL"
        )?,
        "conversation_drafts": count(
            "SELECT COUNT(*) FROM conversation_drafts WHERE workspace_id = ?1"
        )?,
        "settings": count("SELECT COUNT(*) FROM settings WHERE workspace_id = ?1")?,
    }))
}

fn sanitize_diagnostic_label(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | ':' | '.' | ' ')
        })
        .take(80)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_counts_do_not_expose_content() {
        let mut conn = Connection::open_in_memory().expect("open memory sqlite");
        crate::storage::migrations::migrate(&mut conn).expect("migrate schema");
        conn.execute(
            "INSERT INTO conversations
             (id, workspace_id, title, created_at, updated_at, archived)
             VALUES ('c1', 'local', 'secret title', 't1', 't1', 0)",
            [],
        )
        .expect("insert conversation");
        conn.execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, created_at)
             VALUES ('m1', 'local', 'c1', 'user', 'secret content', 't1')",
            [],
        )
        .expect("insert message");

        let counts = diagnostic_table_counts(&conn, "local").expect("counts");

        assert_eq!(counts["conversations_active"], 1);
        assert_eq!(counts["messages"], 1);
        assert!(!counts.to_string().contains("secret"));
    }

    #[test]
    fn diagnostic_export_is_written_as_json_without_partial_target() {
        let root =
            std::env::temp_dir().join(format!("bloomery-diagnostics-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create diagnostics root");
        let output = root.join("diagnostics.json");
        let value = serde_json::json!({
            "privacy": { "contains_provider_secret": false },
            "counts": { "messages": 2 }
        });

        write_json_atomically(&output, &value).expect("write diagnostics JSON");

        let written = std::fs::read_to_string(&output).expect("read diagnostics JSON");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).unwrap(),
            value
        );
        assert!(!root.join(".diagnostics.json.tmp").exists());
        std::fs::remove_dir_all(root).expect("remove diagnostics root");
    }
}
