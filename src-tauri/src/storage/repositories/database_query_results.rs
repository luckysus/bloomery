use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct QueryResultRecord {
    pub task_id: Uuid,
    pub connection_id: Uuid,
    pub database_name: String,
    pub query_text: String,
    pub row_count: i64,
    pub truncated: bool,
    pub duration_ms: i64,
    pub csv_path: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryResultSummary {
    pub task_id: Uuid,
    pub database_name: String,
    pub query_text: String,
    pub row_count: i64,
    pub truncated: bool,
    pub duration_ms: i64,
    pub created_at: String,
}

fn parse_uuid(value: String, column_index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column_index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn decode_json<T: serde::de::DeserializeOwned>(
    value: String,
    column_index: usize,
) -> rusqlite::Result<T> {
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column_index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

pub fn insert(
    conn: &Connection,
    workspace_id: &str,
    record: &QueryResultRecord,
) -> Result<(), String> {
    let columns_json = serde_json::to_string(&record.columns).map_err(|error| error.to_string())?;
    let rows_json = serde_json::to_string(&record.rows).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO database_query_results
          (task_id, workspace_id, connection_id, database_name, query_text,
           row_count, truncated, duration_ms, csv_path, columns_json, rows_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            record.task_id.to_string(),
            workspace_id,
            record.connection_id.to_string(),
            record.database_name,
            record.query_text,
            record.row_count,
            record.truncated as i64,
            record.duration_ms,
            record.csv_path,
            columns_json,
            rows_json,
            record.created_at
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get(
    conn: &Connection,
    workspace_id: &str,
    task_id: Uuid,
) -> Result<Option<QueryResultRecord>, String> {
    conn.query_row(
        "SELECT connection_id, database_name, query_text, row_count, truncated, duration_ms,
                csv_path, columns_json, rows_json, created_at
         FROM database_query_results WHERE workspace_id = ?1 AND task_id = ?2",
        params![workspace_id, task_id.to_string()],
        |row| {
            Ok(QueryResultRecord {
                task_id,
                connection_id: parse_uuid(row.get(0)?, 0)?,
                database_name: row.get(1)?,
                query_text: row.get(2)?,
                row_count: row.get(3)?,
                truncated: row.get::<_, i64>(4)? == 1,
                duration_ms: row.get(5)?,
                csv_path: row.get(6)?,
                columns: decode_json(row.get(7)?, 7)?,
                rows: decode_json(row.get(8)?, 8)?,
                created_at: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(Ok)
    .transpose()
}

pub fn list_recent(
    conn: &Connection,
    workspace_id: &str,
    limit: u32,
) -> Result<Vec<QueryResultSummary>, String> {
    let mut statement = conn
        .prepare(
            "SELECT task_id, database_name, query_text, row_count, truncated, duration_ms, created_at
             FROM database_query_results
             WHERE workspace_id = ?1
             ORDER BY created_at DESC, task_id DESC
             LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id, limit], |row| {
            Ok(QueryResultSummary {
                task_id: parse_uuid(row.get(0)?, 0)?,
                database_name: row.get(1)?,
                query_text: row.get(2)?,
                row_count: row.get(3)?,
                truncated: row.get::<_, i64>(4)? == 1,
                duration_ms: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
