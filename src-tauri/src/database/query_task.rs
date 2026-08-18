use crate::database::{catalog, query as query_guard, SqlClient};
use crate::storage::repositories::database_query_results::QueryResultRecord;
use crate::storage::repositories::{
    database_connections as connections_repository, database_query_results as results_repository,
};
use crate::storage::secrets::{KeyringSecretStore, SecretRef, SecretStore};
use crate::tasks::scheduler::{
    HandlerContext, HandlerError, HandlerFuture, HandlerOutcome, TaskHandler,
};
use crate::tasks::TaskRecord;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const DATABASE_QUERY_KIND: &str = "database_query";
pub const PASSWORD_CREDENTIAL: &str = "password";
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Deserialize)]
struct QueryTaskPayload {
    connection_id: String,
    database: Option<String>,
    sql: String,
    row_limit: Option<u64>,
}

fn parse_payload(payload_json: &str) -> Result<QueryTaskPayload, HandlerError> {
    let payload: QueryTaskPayload =
        serde_json::from_str(payload_json).map_err(|_| HandlerError::permanent("invalid_payload"))?;
    if payload.connection_id.trim().is_empty() || payload.sql.trim().is_empty() {
        return Err(HandlerError::permanent("invalid_payload"));
    }
    Ok(payload)
}

fn csv_line(cells: &[Option<String>]) -> String {
    cells
        .iter()
        .map(|cell| catalog::csv_cell(cell.as_deref()))
        .collect::<Vec<_>>()
        .join(",")
}

fn build_csv_document(columns: &[String], rows: &[Vec<Option<String>>]) -> String {
    let mut document = String::new();
    document.push_str(
        &columns
            .iter()
            .map(|column| catalog::csv_cell(Some(column)))
            .collect::<Vec<_>>()
            .join(","),
    );
    document.push('\n');
    for row in rows {
        document.push_str(&csv_line(row));
        document.push('\n');
    }
    document
}

async fn cancellation_watch(context: HandlerContext) {
    loop {
        if context.shutdown_requested() || context.cancellation_requested().unwrap_or(true) {
            return;
        }
        tokio::time::sleep(CANCEL_POLL_INTERVAL).await;
    }
}

async fn run_query(
    mut client: SqlClient,
    database: Option<&str>,
    wrapped: String,
    timeout: Duration,
) -> Result<catalog::QueryRows, HandlerError> {
    if let Some(name) = database {
        let statement = format!(
            "USE {}",
            catalog::escape_identifier(name).map_err(HandlerError::permanent)?
        );
        client
            .simple_query(statement)
            .await
            .map_err(|error| HandlerError::permanent(format!("cannot switch database: {error}")))?;
    }
    match tokio::time::timeout(timeout, catalog::execute_read(&mut client, &wrapped)).await {
        Ok(result) => result.map_err(|error| HandlerError::permanent(format!("query_failed: {error}"))),
        Err(_) => Err(HandlerError::permanent("query_timeout")),
    }
}

pub struct DatabaseQueryTaskHandler {
    db_path: PathBuf,
}

impl DatabaseQueryTaskHandler {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl TaskHandler for DatabaseQueryTaskHandler {
    fn kind(&self) -> &str {
        DATABASE_QUERY_KIND
    }

    fn resumable(&self) -> bool {
        false
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let db_path = self.db_path.clone();
        Box::pin(async move { execute(task, context, db_path).await })
    }
}

async fn execute(
    task: TaskRecord,
    context: HandlerContext,
    db_path: PathBuf,
) -> Result<HandlerOutcome, HandlerError> {
    let payload = parse_payload(&task.payload_json)?;
    let connection_id = Uuid::parse_str(payload.connection_id.trim())
        .map_err(|_| HandlerError::permanent("invalid_payload"))?;
    let row_limit = query_guard::clamp_row_limit(payload.row_limit);
    let normalized = query_guard::normalize_query(&payload.sql)
        .map_err(|reason| HandlerError::permanent(format!("query_guard_rejected: {reason}")))?;

    let workspace_id = task.workspace_id.clone();
    let connection = rusqlite::Connection::open(&db_path)
        .map_err(|_| HandlerError::retryable("storage_unavailable"))?;
    let record = connections_repository::get(&connection, &workspace_id, connection_id)
        .map_err(|_| HandlerError::retryable("storage_unavailable"))?
        .ok_or_else(|| HandlerError::permanent("connection_not_found"))?;
    if !record.enabled {
        return Err(HandlerError::permanent("connection_disabled"));
    }
    let secret_reference = SecretRef::new(connection_id, PASSWORD_CREDENTIAL)
        .map_err(|_| HandlerError::permanent("password_not_configured"))?;
    let password = KeyringSecretStore
        .get(&secret_reference)
        .map_err(|_| HandlerError::permanent("password_not_configured"))?
        .expose()
        .to_string();

    context
        .checkpoint(Some(r#"{"stage":"running"}"#), 10, None)
        .map_err(|_| HandlerError::retryable("checkpoint_failed"))?;

    let started = Instant::now();
    let wrapped = query_guard::wrap_query(&normalized, row_limit);
    let timeout = Duration::from_millis(record.timeout_ms);
    let query = crate::database::connect(&record, &password);

    let rows = tokio::select! {
        joined = async {
            match query.await {
                Ok(client) => run_query(client, payload.database.as_deref(), wrapped, timeout).await,
                Err(error) => Err(HandlerError::permanent(format!("connection_failed: {error}"))),
            }
        } => joined?,
        _ = cancellation_watch(context.clone()) => return Ok(HandlerOutcome::Cancelled),
    };
    let duration_ms = started.elapsed().as_millis() as i64;

    let row_count = rows.rows.len() as i64;
    let truncated = row_limit > 0 && row_count == row_limit as i64;
    let csv_directory = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("query-cache");
    std::fs::create_dir_all(&csv_directory)
        .map_err(|_| HandlerError::permanent("result_write_failed"))?;
    let csv_path = csv_directory.join(format!("{}.csv", task.id));
    std::fs::write(&csv_path, build_csv_document(&rows.columns, &rows.rows))
        .map_err(|_| HandlerError::permanent("result_write_failed"))?;

    let result = QueryResultRecord {
        task_id: task.id,
        connection_id,
        database_name: payload.database.clone().unwrap_or_default(),
        query_text: normalized,
        row_count,
        truncated,
        duration_ms,
        csv_path: csv_path.to_string_lossy().to_string(),
        columns: rows.columns,
        rows: rows.rows,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    results_repository::insert(&connection, &workspace_id, &result)
        .map_err(|_| HandlerError::permanent("result_write_failed"))?;

    context
        .checkpoint(Some(r#"{"stage":"completed"}"#), 100, None)
        .map_err(|_| HandlerError::retryable("checkpoint_failed"))?;
    Ok(HandlerOutcome::Completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_payload_reads_fields() {
        let payload = parse_payload(
            r#"{"connection_id":"11111111-1111-1111-1111-111111111111","database":"SteelWorks","sql":"SELECT 1","row_limit":100}"#,
        )
        .expect("parse");
        assert_eq!(payload.sql, "SELECT 1");
        assert_eq!(payload.database.as_deref(), Some("SteelWorks"));
        assert_eq!(payload.row_limit, Some(100));
    }

    #[test]
    fn parse_payload_rejects_invalid_json() {
        assert!(parse_payload("not json").is_err());
        assert!(parse_payload("{}").is_err(), "缺 connection_id/sql 应拒绝");
    }

    #[test]
    fn csv_document_contains_header_and_rows() {
        let document = build_csv_document(
            &["heat_id".to_string(), "carbon_pct".to_string()],
            &vec![
                vec![Some("H1".to_string()), Some("0.18".to_string())],
                vec![Some("H,2".to_string()), None],
            ],
        );
        let lines: Vec<&str> = document.lines().collect();
        assert_eq!(lines[0], "heat_id,carbon_pct");
        assert_eq!(lines[1], "H1,0.18");
        assert_eq!(lines[2], "\"H,2\",");
    }
}
