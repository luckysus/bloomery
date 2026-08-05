use crate::agent::session::service::SessionService;
use crate::db::{current_workspace_id, with_conn_mut, DbState};

pub(super) fn with_session<T>(
    db: &tauri::State<'_, DbState>,
    operation: impl FnOnce(&mut SessionService<'_>) -> Result<T, String>,
) -> Result<T, String> {
    with_conn_mut(db, |connection| {
        let mut session = SessionService::new(connection, current_workspace_id())?;
        operation(&mut session)
    })
}
