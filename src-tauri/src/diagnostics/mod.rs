pub mod redaction;

use crate::db::{current_workspace_id, database_path, with_conn, DbState};
use crate::rag::index::rebuild::IndexRebuildRequest;
use crate::rag::index::repair::{inspect_index_health, IndexHealthReport};
use crate::storage::migrations::latest_version;
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct StorageHealth {
    pub database_ok: bool,
    pub current_migration_version: u32,
    pub latest_migration_version: u32,
    pub database_size_bytes: u64,
    pub reclaimable_bytes: u64,
    pub available_disk_bytes: Option<u64>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::migrations::{latest_version, migrate};
    use rusqlite::Connection;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn storage_health_reports_safe_database_and_disk_metadata() {
        let path = std::env::temp_dir().join(format!("bloomery-health-{}.sqlite3", Uuid::new_v4()));
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
