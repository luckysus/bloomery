use super::bundled_domain;
use super::domain_commands::DomainInstallResult;
use crate::db::DbState;

#[tauri::command]
pub fn install_bundled_steel_package(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
) -> Result<DomainInstallResult, String> {
    bundled_domain::install_steel_package(&app, &db)
}
