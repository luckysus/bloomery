use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::domains::{self, official_trust_store, DomainTrustStore};
use crate::storage::repositories::domains::{
    self as domain_repository, DomainPackageImpact, DomainPackageRecord,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct DomainInstallResult {
    pub package: DomainPackageRecord,
    pub replaced_active_version: Option<String>,
}

fn domains_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data dir failed: {error}"))?
        .join("domains");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("create domain package directory failed: {error}"))?;
    Ok(directory)
}

fn trust_store() -> DomainTrustStore {
    // Official keys are embedded at build time; unsigned community packages remain usable.
    official_trust_store()
}

#[tauri::command]
pub fn list_domain_packages(db: tauri::State<DbState>) -> Result<Vec<DomainPackageRecord>, String> {
    with_conn(&db, |connection| {
        domain_repository::list(connection, current_workspace_id())
    })
}

#[tauri::command]
pub fn install_domain_package(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    source_path: String,
) -> Result<DomainInstallResult, String> {
    let source = Path::new(&source_path);
    let root = domains_root(&app)?;
    let installed =
        domains::install_package(source, &root, env!("CARGO_PKG_VERSION"), &trust_store())
            .map_err(|error| error.to_string())?;
    let package = match with_conn_mut(&db, |connection| {
        domain_repository::upsert(connection, current_workspace_id(), &installed)
    }) {
        Ok(package) => package,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&installed.path);
            return Err(error);
        }
    };
    Ok(DomainInstallResult {
        package,
        replaced_active_version: None,
    })
}

#[tauri::command]
pub fn activate_domain_package(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    package_id: String,
    version: String,
) -> Result<DomainPackageRecord, String> {
    let root = domains_root(&app)?;
    domains::activate_package(&root, &package_id, &version, env!("CARGO_PKG_VERSION"))
        .map_err(|error| error.to_string())?;
    with_conn_mut(&db, |connection| {
        domain_repository::activate(connection, current_workspace_id(), &package_id, &version)
    })
}

#[tauri::command]
pub fn preview_remove_domain_package(
    db: tauri::State<DbState>,
    package_id: String,
    version: String,
) -> Result<DomainPackageImpact, String> {
    with_conn(&db, |connection| {
        domain_repository::impact(connection, current_workspace_id(), &package_id, &version)
    })
}

#[tauri::command]
pub fn remove_domain_package(
    db: tauri::State<DbState>,
    package_id: String,
    version: String,
) -> Result<(), String> {
    let record = with_conn(&db, |connection| {
        domain_repository::get(connection, current_workspace_id(), &package_id, &version)
    })?
    .ok_or_else(|| "domain package version is not installed".to_string())?;
    if record.active {
        return Err("active domain package cannot be removed".to_string());
    }
    std::fs::remove_dir_all(&record.path)
        .map_err(|error| format!("remove domain package files failed: {error}"))?;
    with_conn_mut(&db, |connection| {
        domain_repository::remove(connection, current_workspace_id(), &package_id, &version)
    })
}
