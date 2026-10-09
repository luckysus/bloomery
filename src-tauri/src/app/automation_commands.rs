use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::tasks::cron_repository::{self, CronJob, SaveAgentSchedule};
use uuid::Uuid;

#[tauri::command]
pub fn list_agent_schedules(db: tauri::State<DbState>) -> Result<Vec<CronJob>, String> {
    with_conn(&db, |connection| {
        cron_repository::list(connection, current_workspace_id())
    })
}

#[tauri::command]
pub fn save_agent_schedule(
    db: tauri::State<DbState>,
    request: SaveAgentSchedule,
) -> Result<CronJob, String> {
    with_conn_mut(&db, |connection| {
        cron_repository::save(
            connection,
            current_workspace_id(),
            request,
            chrono::Utc::now(),
        )
    })
}

#[tauri::command]
pub fn delete_agent_schedule(db: tauri::State<DbState>, id: Uuid) -> Result<(), String> {
    with_conn_mut(&db, |connection| {
        cron_repository::delete(connection, current_workspace_id(), id)
    })
}

#[tauri::command]
pub fn set_agent_schedule_enabled(
    db: tauri::State<DbState>,
    id: Uuid,
    enabled: bool,
) -> Result<(), String> {
    with_conn_mut(&db, |connection| {
        cron_repository::set_enabled(
            connection,
            current_workspace_id(),
            id,
            enabled,
            chrono::Utc::now(),
        )
    })
}
