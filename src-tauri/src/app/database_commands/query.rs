use super::{
    logic,
    types::{
        parse_id, DatabaseQueryResultResponse, DatabaseQuerySubmitInput,
        DatabaseQuerySummaryResponse,
    },
};
use crate::{
    app::task_commands::tasks::{background_task_response, BackgroundTaskResponse},
    database,
    database::query_task::DATABASE_QUERY_KIND,
    db::{current_workspace_id, with_conn, DbState},
    storage::{
        repositories::{
            database_connections as connections_repository,
            database_query_results as results_repository,
        },
        secrets::SecretState,
    },
    tasks::{model::NewTask, repository as task_repository},
};

#[tauri::command]
pub(crate) async fn list_databases(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
) -> Result<Vec<String>, String> {
    let id = parse_id(&id)?;
    let record = super::logic::load_enabled_record(&db, secrets.store(), id)?;
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
    let record = super::logic::load_enabled_record(&db, secrets.store(), id)?;
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
        let record = connections_repository::get(
            connection,
            current_workspace_id(),
            prepared.connection_id,
        )?
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
    with_conn(&db, |connection| {
        Ok(
            results_repository::get(connection, current_workspace_id(), task_id)?.map(|record| {
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
            }),
        )
    })
}

#[tauri::command]
pub(crate) fn list_database_query_results(
    db: tauri::State<DbState>,
) -> Result<Vec<DatabaseQuerySummaryResponse>, String> {
    with_conn(&db, |connection| {
        Ok(
            results_repository::list_recent(connection, current_workspace_id(), 10)?
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
                .collect(),
        )
    })
}
