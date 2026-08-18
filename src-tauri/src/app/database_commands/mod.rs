mod logic;
mod types;

use crate::{
    app::task_commands::tasks::{background_task_response, BackgroundTaskResponse},
    database,
    database::query_task::DATABASE_QUERY_KIND,
    db::{current_workspace_id, with_conn, DbState},
    storage::{
        repositories::{
            database_connections as repository, database_query_results as results_repository,
        },
        secrets::SecretState,
    },
    tasks::{model::NewTask, repository as task_repository},
};
use logic::load_record;
use std::time::Instant;
use types::{
    parse_id, DatabaseConnectionInput, DatabaseConnectionSummary, DatabaseQueryResultResponse,
    DatabaseQuerySubmitInput, DatabaseQuerySummaryResponse,
};

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

#[tauri::command]
pub(crate) async fn list_databases(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
) -> Result<Vec<String>, String> {
    let id = parse_id(&id)?;
    let record = load_record(&db, id)?;
    if !record.enabled {
        return Err("database connection is disabled".to_string());
    }
    let secret = logic::password(secrets.store(), id)?;
    let mut client = database::connect(&record, &secret).await?;
    database::catalog::list_databases(&mut client).await
}

#[tauri::command]
pub(crate) async fn list_database_tables(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
    database_name: Option<String>,
) -> Result<Vec<String>, String> {
    let id = parse_id(&id)?;
    let record = load_record(&db, id)?;
    if !record.enabled {
        return Err("database connection is disabled".to_string());
    }
    let secret = logic::password(secrets.store(), id)?;
    let mut client = database::connect(&record, &secret).await?;
    let target = database_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    database::catalog::table_names(&mut client, target).await
}

#[tauri::command]
pub(crate) fn submit_database_query(
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
    input: DatabaseQuerySubmitInput,
) -> Result<BackgroundTaskResponse, String> {
    let prepared = logic::validate_submission(&input)?;
    let payload = serde_json::json!({
        "connection_id": prepared.connection_id.to_string(),
        "database": prepared.database,
        "sql": prepared.sql,
        "row_limit": prepared.row_limit,
    });
    crate::db::with_conn_mut(&db, |connection| {
        let record = repository::get(connection, current_workspace_id(), prepared.connection_id)?
            .ok_or_else(|| "database connection not found".to_string())?;
        if !record.enabled {
            return Err("database connection is disabled".to_string());
        }
        if !logic::secret_configured(secrets.store(), prepared.connection_id) {
            return Err("database password is not configured".to_string());
        }
        task_repository::create(
            connection,
            NewTask {
                workspace_id: current_workspace_id().to_string(),
                kind: DATABASE_QUERY_KIND.to_string(),
                payload_json: payload.to_string(),
                checkpoint_json: Some(r#"{"stage":"queued"}"#.to_string()),
                next_run_at: None,
                progress: 0,
            },
        )
        .map(background_task_response)
        .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn get_database_query_result(
    db: tauri::State<DbState>,
    task_id: String,
) -> Result<Option<DatabaseQueryResultResponse>, String> {
    let task_id = parse_id(&task_id)?;
    crate::db::with_conn(&db, |connection| {
        Ok(results_repository::get(connection, current_workspace_id(), task_id)?.map(|record| {
            DatabaseQueryResultResponse {
                task_id: record.task_id.to_string(),
                connection_id: record.connection_id.to_string(),
                database_name: record.database_name,
                query_text: record.query_text,
                row_count: record.row_count,
                truncated: record.truncated,
                duration_ms: record.duration_ms,
                csv_path: record.csv_path,
                columns: record.columns,
                rows: record.rows,
                created_at: record.created_at,
            }
        }))
    })
}

#[tauri::command]
pub(crate) fn list_database_query_results(
    db: tauri::State<DbState>,
) -> Result<Vec<DatabaseQuerySummaryResponse>, String> {
    crate::db::with_conn(&db, |connection| {
        Ok(results_repository::list_recent(connection, current_workspace_id(), 10)?
            .into_iter()
            .map(|summary| DatabaseQuerySummaryResponse {
                task_id: summary.task_id.to_string(),
                database_name: summary.database_name,
                query_text: summary.query_text,
                row_count: summary.row_count,
                truncated: summary.truncated,
                duration_ms: summary.duration_ms,
                created_at: summary.created_at,
            })
            .collect())
    })
}
