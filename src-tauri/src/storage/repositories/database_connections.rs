use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseConnectionRecord {
    pub id: Uuid,
    pub display_name: String,
    pub host: String,
    pub port: u16,
    pub database_name: String,
    pub username: String,
    pub timeout_ms: u64,
    pub enabled: bool,
    pub last_checked_at: Option<String>,
    pub last_latency_ms: Option<i64>,
    pub last_version: Option<String>,
    pub last_error: Option<String>,
}

const SELECT_COLUMNS: &str =
    "id, display_name, host, port, database_name, username, timeout_ms, enabled, last_checked_at, last_latency_ms, last_version, last_error";

fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatabaseConnectionRecord> {
    Ok(DatabaseConnectionRecord {
        id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        display_name: row.get(1)?,
        host: row.get(2)?,
        port: row.get::<_, i64>(3)? as u16,
        database_name: row.get(4)?,
        username: row.get(5)?,
        timeout_ms: row.get::<_, i64>(6)? as u64,
        enabled: row.get::<_, i64>(7)? == 1,
        last_checked_at: row.get(8)?,
        last_latency_ms: row.get(9)?,
        last_version: row.get(10)?,
        last_error: row.get(11)?,
    })
}

pub fn list(
    conn: &Connection,
    workspace_id: &str,
) -> Result<Vec<DatabaseConnectionRecord>, String> {
    let mut statement = conn
        .prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM database_connections
             WHERE workspace_id = ?1
             ORDER BY display_name ASC, id ASC"
        ))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], decode)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn get(
    conn: &Connection,
    workspace_id: &str,
    id: Uuid,
) -> Result<Option<DatabaseConnectionRecord>, String> {
    conn.query_row(
        &format!(
            "SELECT {SELECT_COLUMNS} FROM database_connections
             WHERE workspace_id = ?1 AND id = ?2"
        ),
        params![workspace_id, id.to_string()],
        decode,
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(|value| Ok(value))
    .transpose()
}

pub fn save(
    conn: &mut Connection,
    workspace_id: &str,
    record: &DatabaseConnectionRecord,
) -> Result<(), String> {
    let owner = conn
        .query_row(
            "SELECT workspace_id FROM database_connections WHERE id = ?1",
            params![record.id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if owner.as_deref().is_some_and(|value| value != workspace_id) {
        return Err("database connection belongs to another workspace".to_string());
    }
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO database_connections
          (id, workspace_id, display_name, host, port, database_name, username,
           timeout_ms, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
         ON CONFLICT(id) DO UPDATE SET
           display_name = excluded.display_name,
           host = excluded.host,
           port = excluded.port,
           database_name = excluded.database_name,
           username = excluded.username,
           timeout_ms = excluded.timeout_ms,
           enabled = excluded.enabled,
           updated_at = excluded.updated_at",
        params![
            record.id.to_string(),
            workspace_id,
            record.display_name,
            record.host,
            record.port as i64,
            record.database_name,
            record.username,
            record.timeout_ms as i64,
            record.enabled as i64,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete(conn: &mut Connection, workspace_id: &str, id: Uuid) -> Result<(), String> {
    let deleted = conn
        .execute(
            "DELETE FROM database_connections WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if deleted == 0 {
        return Err("database connection not found".to_string());
    }
    Ok(())
}

pub fn record_health(
    conn: &Connection,
    workspace_id: &str,
    id: Uuid,
    checked_at: &str,
    latency_ms: Option<i64>,
    version: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    let updated = conn
        .execute(
            "UPDATE database_connections
             SET last_checked_at = ?3, last_latency_ms = ?4, last_version = ?5, last_error = ?6
             WHERE workspace_id = ?1 AND id = ?2",
            params![
                workspace_id,
                id.to_string(),
                checked_at,
                latency_ms,
                version,
                error
            ],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("database connection not found".to_string());
    }
    Ok(())
}
