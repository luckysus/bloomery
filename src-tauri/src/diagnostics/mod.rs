pub(crate) mod export;
pub mod observability;
pub mod redaction;

use crate::db::{app_data_directory, current_workspace_id, database_path, with_conn, DbState};
use crate::rag::index::rebuild::IndexRebuildRequest;
use crate::rag::index::repair::{inspect_index_health, IndexHealthReport};
use crate::storage::migrations::latest_version;
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

#[derive(Debug, Serialize)]
pub struct StorageHealth {
    pub database_ok: bool,
    pub current_migration_version: u32,
    pub latest_migration_version: u32,
    pub database_size_bytes: u64,
    pub reclaimable_bytes: u64,
    pub available_disk_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct StoragePaths {
    pub app_data: String,
    pub documents: String,
    pub local_data: String,
    pub sqlite_database: String,
    pub knowledge_content: String,
    pub cache: String,
    pub logs: String,
    pub temp: String,
}

fn storage_health(connection: &Connection, path: &Path) -> Result<StorageHealth, String> {
    let quick_check: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| format!("database quick check failed: {error}"))?;
    let current_migration_version = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| format!("database version check failed: {error}"))?;
    let page_size: u64 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(|error| format!("database page size check failed: {error}"))?;
    let free_pages: u64 = connection
        .pragma_query_value(None, "freelist_count", |row| row.get(0))
        .map_err(|error| format!("database free page check failed: {error}"))?;
    Ok(StorageHealth {
        database_ok: quick_check == "ok",
        current_migration_version,
        latest_migration_version: latest_version(),
        database_size_bytes: std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0),
        reclaimable_bytes: page_size.saturating_mul(free_pages),
        available_disk_bytes: available_disk_bytes(path.parent().unwrap_or(path))?,
    })
}

#[cfg(windows)]
fn available_disk_bytes(path: &Path) -> Result<Option<u64>, String> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory_name: *const u16,
            free_bytes_available: *mut u64,
            total_bytes: *mut u64,
            total_free_bytes: *mut u64,
        ) -> i32;
    }

    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0_u64;
    let result = unsafe {
        GetDiskFreeSpaceExW(
            wide_path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Err(format!(
            "disk space check failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(Some(available))
    }
}

#[cfg(not(windows))]
fn available_disk_bytes(_path: &Path) -> Result<Option<u64>, String> {
    Ok(None)
}

#[tauri::command]
pub fn get_storage_health(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
) -> Result<StorageHealth, String> {
    let path = database_path(&app)?;
    with_conn(&db, |connection| storage_health(connection, &path))
}

#[tauri::command]
pub fn get_storage_paths(app: tauri::AppHandle) -> Result<StoragePaths, String> {
    let root = app_data_directory(&app)?;
    let documents = app
        .path()
        .document_dir()
        .map_err(|error| format!("resolve documents dir failed: {error}"))?;
    let local_data = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("resolve local data dir failed: {error}"))?;
    Ok(StoragePaths {
        app_data: root.to_string_lossy().into_owned(),
        documents: documents.to_string_lossy().into_owned(),
        local_data: local_data.to_string_lossy().into_owned(),
        sqlite_database: root.join("suna.sqlite3").to_string_lossy().into_owned(),
        knowledge_content: root.join("content").to_string_lossy().into_owned(),
        cache: root.join("cache").to_string_lossy().into_owned(),
        logs: root.join("logs").to_string_lossy().into_owned(),
        temp: root.join("temp").to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn get_index_health(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    request: IndexRebuildRequest,
) -> Result<IndexHealthReport, String> {
    let path = database_path(&app)?;
    let content_root = path
        .parent()
        .ok_or_else(|| "resolve RAG content root failed".to_string())?;
    let available = available_disk_bytes(content_root)?;
    with_conn(&db, |connection| {
        inspect_index_health(
            connection,
            current_workspace_id(),
            content_root,
            &request,
            available,
        )
    })
}

fn storage_path(app: &tauri::AppHandle, kind: &str) -> Result<PathBuf, String> {
    let root = app_data_directory(app)?;
    let path = match kind {
        "app_data" => root,
        "documents" => app
            .path()
            .document_dir()
            .map_err(|error| format!("resolve documents dir failed: {error}"))?,
        "local_data" => app
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("resolve local data dir failed: {error}"))?,
        "sqlite_database" => root.join("suna.sqlite3"),
        "knowledge_content" => root.join("content"),
        "cache" => root.join("cache"),
        "logs" => root.join("logs"),
        "temp" => root.join("temp"),
        _ => return Err("storage path kind is invalid".to_string()),
    };
    Ok(path)
}

#[cfg(windows)]
fn is_link_or_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[tauri::command]
pub fn open_storage_path(app: tauri::AppHandle, kind: String) -> Result<(), String> {
    let path = storage_path(&app, &kind)?;
    if !path.exists()
        && matches!(
            kind.as_str(),
            "app_data"
                | "documents"
                | "local_data"
                | "cache"
                | "temp"
                | "logs"
                | "knowledge_content"
        )
    {
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create storage directory failed: {error}"))?;
    }
    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg(&path)
            .spawn()
            .map_err(|error| format!("open storage directory failed: {error}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|error| format!("open storage directory failed: {error}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|error| format!("open storage directory failed: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn clear_storage_cache(app: tauri::AppHandle, kind: String) -> Result<u64, String> {
    if !matches!(kind.as_str(), "cache" | "temp") {
        return Err("only cache and temp directories can be cleared".to_string());
    }
    let path = storage_path(&app, &kind)?;
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("read storage directory failed: {error}")),
    };
    if is_link_or_reparse_point(&metadata) {
        return Err("refusing to clear a redirected storage directory".to_string());
    }
    if !metadata.is_dir() {
        return Err("storage cache path is not a directory".to_string());
    }

    let entries = std::fs::read_dir(&path)
        .map_err(|error| format!("read storage directory failed: {error}"))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("read storage entry failed: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for entry_path in &entries {
        let metadata = std::fs::symlink_metadata(entry_path)
            .map_err(|error| format!("read storage entry failed: {error}"))?;
        if is_link_or_reparse_point(&metadata) {
            return Err("refusing to clear a redirected storage entry".to_string());
        }
    }

    let mut removed = 0_u64;
    for entry_path in entries {
        if entry_path.is_dir() {
            std::fs::remove_dir_all(&entry_path)
                .map_err(|error| format!("clear storage directory failed: {error}"))?;
        } else {
            std::fs::remove_file(&entry_path)
                .map_err(|error| format!("clear storage file failed: {error}"))?;
        }
        removed = removed.saturating_add(1);
    }
    Ok(removed)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::migrations::{latest_version, migrate};
    use rusqlite::Connection;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn storage_health_reports_safe_database_and_disk_metadata() {
        let path = std::env::temp_dir().join(format!("suna-health-{}.sqlite3", Uuid::new_v4()));
        let mut connection = Connection::open(&path).expect("open database");
        migrate(&mut connection).expect("migrate database");

        let health = storage_health(&connection, &path).expect("read storage health");

        assert!(health.database_ok);
        assert_eq!(health.current_migration_version, latest_version());
        assert_eq!(health.latest_migration_version, latest_version());
        assert!(health.database_size_bytes > 0);
        #[cfg(windows)]
        assert!(health.available_disk_bytes.is_some());
        #[cfg(not(windows))]
        assert!(health.available_disk_bytes.is_none());
        let json = serde_json::to_string(&health).expect("serialize storage health");
        assert!(!json.contains(&path.to_string_lossy().to_string()));
        drop(connection);
        fs::remove_file(path).expect("remove test database");
    }
}
