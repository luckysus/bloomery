use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::skills::{self, SkillCatalog};

#[tauri::command]
pub fn list_skills(db: tauri::State<DbState>) -> Result<SkillCatalog, String> {
    with_conn(&db, |connection| {
        skills::catalog(
            connection,
            current_workspace_id(),
            env!("CARGO_PKG_VERSION"),
        )
    })
}

#[tauri::command]
pub fn set_skill_enabled(
    db: tauri::State<DbState>,
    name: String,
    enabled: bool,
) -> Result<SkillCatalog, String> {
    with_conn_mut(&db, |connection| {
        skills::set_enabled(
            connection,
            current_workspace_id(),
            &name,
            enabled,
            env!("CARGO_PKG_VERSION"),
        )
    })
}
