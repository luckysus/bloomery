use chrono::Utc;
use rusqlite::{params, Connection};
use std::time::Duration;
use std::{collections::HashSet, fs, path::{Path, PathBuf}, sync::Arc, sync::Mutex};
use tauri::Manager;

use crate::tasks::scheduler::SchedulerState;
use crate::tasks::scheduler::TaskHandler;

pub struct DbState {
    conn: Mutex<Option<Connection>>,
}

impl Default for DbState {
    fn default() -> Self {
        Self {
            conn: Mutex::new(None),
        }
    }
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn current_workspace_id() -> &'static str {
    crate::app::identity::LocalIdentity.workspace_id()
}

pub(crate) fn with_conn<T>(
    db: &tauri::State<'_, DbState>,
    operation: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let guard = db.conn.lock().map_err(|_| "db state poisoned")?;
    let conn = guard.as_ref().ok_or("database not initialized")?;
    operation(conn)
}

pub(crate) fn with_conn_mut<T>(
    db: &tauri::State<'_, DbState>,
    operation: impl FnOnce(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = db.conn.lock().map_err(|_| "db state poisoned")?;
    let conn = guard.as_mut().ok_or("database not initialized")?;
    operation(conn)
}

pub(crate) fn database_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data dir failed: {error}"))?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("create app data dir failed: {error}"))?;
    Ok(directory.join("bloomery.sqlite3"))
}

fn content_root_for(database: &PathBuf) -> Result<PathBuf, String> {
    database
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "resolve RAG content root failed".to_string())
}

#[tauri::command]
pub fn db_init(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    scheduler_state: tauri::State<SchedulerState>,
) -> Result<(), String> {
    let path = database_path(&app)?;
    let (mut connection, _migration_report) =
        crate::storage::database::open(&path).map_err(|error| error.to_string())?;
    let mut recovery =
        crate::agent::runtime::AgentRecoveryService::new(&mut connection, current_workspace_id())?;
    recovery
        .recover_active(&HashSet::new(), Utc::now())
        .map_err(|error| format!("recover agent runs failed: {error}"))?;
    *db.conn.lock().map_err(|_| "db state poisoned")? = Some(connection);
    // Start scheduler with Tauri event sink for real progress updates
    use crate::app::event_sink::TauriEventSink;
    use crate::tasks::scheduler::{Scheduler, SchedulerConfig, SystemClock};
    let sink = Arc::new(TauriEventSink::new(app.clone()));
    let content_root = content_root_for(&path)?;
    let scheduler = Scheduler::new(
        path.clone(),
        current_workspace_id().to_string(),
        SchedulerConfig::default(),
        Arc::new(SystemClock),
        rag_task_handlers(path.clone(), content_root),
        sink,
    )
    .map_err(|error| error.to_string())?;
    scheduler_state
        .start(scheduler)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn create_backup_archive(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    archive_path: String,
) -> Result<crate::storage::backup::BackupSummary, String> {
    let archive_path = PathBuf::from(archive_path.trim());
    if archive_path.as_os_str().is_empty() {
        return Err("backup archive path is required".to_string());
    }
    let database = database_path(&app)?;
    let content_root = content_root_for(&database)?;
    with_conn(&db, |connection| {
        crate::storage::backup::create_backup(connection, &database, &content_root, &archive_path)
    })
}

#[tauri::command]
pub fn preview_backup_archive(
    archive_path: String,
) -> Result<crate::storage::backup::BackupSummary, String> {
    let archive_path = PathBuf::from(archive_path.trim());
    if archive_path.as_os_str().is_empty() {
        return Err("backup archive path is required".to_string());
    }
    crate::storage::backup::preview_backup(&archive_path)
}

#[tauri::command]
pub fn restore_backup_archive(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    scheduler_state: tauri::State<SchedulerState>,
    archive_path: String,
) -> Result<crate::storage::backup::BackupSummary, String> {
    let archive_path = PathBuf::from(archive_path.trim());
    if archive_path.as_os_str().is_empty() {
        return Err("backup archive path is required".to_string());
    }
    if !scheduler_state.shutdown(Duration::from_secs(10)) {
        return Err("background scheduler did not stop before restore".to_string());
    }
    let connection = {
        let mut guard = db.conn.lock().map_err(|_| "db state poisoned")?;
        guard.take().ok_or("database not initialized")?
    };
    drop(connection);

    let database = database_path(&app)?;
    let content_root = content_root_for(&database)?;
    let result = crate::storage::backup::restore_backup(&archive_path, &database, &content_root);
    let reinitialized = db_init(app, db, scheduler_state);
    match (result, reinitialized) {
        (Ok(summary), Ok(())) => Ok(summary),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(format!(
            "backup restored but database restart failed: {error}"
        )),
        (Err(error), Err(restart_error)) => {
            Err(format!("{error}; database restart failed: {restart_error}"))
        }
    }
}

fn rag_task_handlers(database: PathBuf, content_root: PathBuf) -> Vec<Arc<dyn TaskHandler>> {
    use crate::rag::index::rebuild::IndexRebuildHandler;
    use crate::rag::tasks::{LocalRagPostprocessor, MinerUTaskHandler, RuntimeProviderFactory};
    use crate::storage::secrets::KeyringSecretStore;

    let providers = Arc::new(RuntimeProviderFactory::new(
        database.clone(),
        Arc::new(KeyringSecretStore),
    ));
    let embedding_factory: Arc<dyn crate::rag::index::EmbeddingRemoteFactory> = providers.clone();
    let remote_factory: Arc<dyn crate::rag::tasks::MinerURemoteFactory> = providers;
    let postprocessor = Arc::new(LocalRagPostprocessor::new(
        database.clone(),
        content_root.clone(),
        embedding_factory,
    ));
    vec![
        Arc::new(MinerUTaskHandler::new(
            content_root.clone(),
            remote_factory,
            postprocessor,
            std::time::Duration::from_secs(2),
        )),
        Arc::new(IndexRebuildHandler::new(database, content_root)),
    ]
}

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
    fn local_workspace_is_stable() {
        assert_eq!(current_workspace_id(), "local");
    }

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
        let root = std::env::temp_dir().join(format!("bloomery-diagnostics-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create diagnostics root");
        let output = root.join("diagnostics.json");
        let value = serde_json::json!({
            "privacy": { "contains_provider_secret": false },
            "counts": { "messages": 2 }
        });

        write_json_atomically(&output, &value).expect("write diagnostics JSON");

        let written = std::fs::read_to_string(&output).expect("read diagnostics JSON");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&written).unwrap(), value);
        assert!(!root.join(".diagnostics.json.tmp").exists());
        std::fs::remove_dir_all(root).expect("remove diagnostics root");
    }

    #[test]
    fn production_scheduler_registers_mineru_ingest_handler() {
        let root = std::env::temp_dir().join(format!("bloomery-handlers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create handler root");
        let handlers = rag_task_handlers(root.join("bloomery.sqlite3"), root.clone());

        assert_eq!(handlers.len(), 2);
        assert!(handlers
            .iter()
            .any(|handler| handler.kind() == crate::rag::tasks::MINERU_TASK_KIND));
        assert!(handlers
            .iter()
            .any(|handler| { handler.kind() == crate::rag::index::rebuild::INDEX_REBUILD_KIND }));

        let _ = std::fs::remove_dir_all(root);
    }
}
