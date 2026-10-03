pub(crate) mod health;
mod logic;
pub(crate) mod query;
mod types;

use crate::{
    db::{current_workspace_id, with_conn, DbState},
    storage::{
        repositories::{database_connections as repository, settings},
        secrets::SecretState,
    },
};
use logic::load_record;
use types::{parse_id, DatabaseConnectionInput, DatabaseConnectionSummary};

#[tauri::command]
pub(crate) fn list_database_connections(
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
) -> Result<Vec<DatabaseConnectionSummary>, String> {
    with_conn(&db, |connection| {
        repository::list(connection, current_workspace_id())?
            .into_iter()
            .map(|record| Ok(logic::summary(&record, secrets.store())))
            .collect()
    })
}

#[tauri::command]
pub(crate) async fn save_database_connection(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    input: DatabaseConnectionInput,
) -> Result<DatabaseConnectionSummary, String> {
    let existing = input
        .id
        .as_deref()
        .map(parse_id)
        .transpose()?
        .map(|id| load_record(&db, id))
        .transpose()?;
    let is_update = existing.is_some();
    let provided = input
        .password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (id, mut record) = logic::normalized(input, existing.as_ref().map(|record| record.id))?;
    if provided.is_none() && existing.is_none() {
        return Err("password is required for a new database connection".to_string());
    }
    if let Some(existing) = existing {
        record.last_checked_at = existing.last_checked_at;
        record.last_latency_ms = existing.last_latency_ms;
        record.last_version = existing.last_version;
        record.last_error = existing.last_error;
    }
    if let Some(value) = provided.as_deref() {
        logic::set_password(secrets.store(), id, value)?;
    }
    crate::db::with_conn_mut(&db, |connection| {
        repository::save(connection, current_workspace_id(), &record)?;
        settings::record_audit(
            connection,
            current_workspace_id(),
            if is_update {
                "database.update"
            } else {
                "database.create"
            },
            &format!("database:{}", id),
            "success",
        )
    })?;
    Ok(logic::summary(&record, secrets.store()))
}

#[tauri::command]
pub(crate) async fn delete_database_connection(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    load_record(&db, id)?;
    crate::db::with_conn_mut(&db, |connection| {
        repository::delete(connection, current_workspace_id(), id)?;
        settings::record_audit(
            connection,
            current_workspace_id(),
            "database.delete",
            &format!("database:{}", id),
            "success",
        )
    })?;
    logic::delete_password(secrets.store(), id)
}
