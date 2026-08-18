mod logic;
pub(crate) mod query;
mod types;

use crate::{
    database,
    db::{current_workspace_id, with_conn, DbState},
    storage::{repositories::database_connections as repository, secrets::SecretState},
};
use logic::load_record;
use std::time::Instant;
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
        repository::save(connection, current_workspace_id(), &record)
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
        repository::delete(connection, current_workspace_id(), id)
    })?;
    logic::delete_password(secrets.store(), id)
}

#[tauri::command]
pub(crate) async fn test_database_connection(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
) -> Result<String, String> {
    let id = parse_id(&id)?;
    let record = load_record(&db, id)?;
    let secret = logic::password(secrets.store(), id)?;
    let started = Instant::now();
    let outcome = async {
        let mut client = database::connect(&record, &secret).await?;
        database::server_version(&mut client).await
    }
    .await;
    let checked_at = chrono::Utc::now().to_rfc3339();
    match &outcome {
        Ok(version) => {
            crate::db::with_conn(&db, |connection| {
                repository::record_health(
                    connection,
                    current_workspace_id(),
                    id,
                    &checked_at,
                    Some(started.elapsed().as_millis() as i64),
                    Some(version),
                    None,
                )
            })?;
            Ok(version.clone())
        }
        Err(error) => {
            let _ = crate::db::with_conn(&db, |connection| {
                repository::record_health(
                    connection,
                    current_workspace_id(),
                    id,
                    &checked_at,
                    None,
                    None,
                    Some(error),
                )
            });
            Err(error.clone())
        }
    }
}
