use crate::storage::StorageError;
use rusqlite::{params, Connection};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronOutboxEvent {
    pub event_id: Uuid,
    pub job_id: Uuid,
    pub slot_at_utc: String,
    pub identity: String,
    pub prompt: String,
}

pub fn pending_events(
    conn: &Connection,
    workspace_id: &str,
) -> Result<Vec<CronOutboxEvent>, StorageError> {
    let mut statement = conn.prepare("SELECT event_id, job_id, slot_at_utc, identity, prompt FROM cron_outbox WHERE workspace_id = ?1 AND acknowledged_at IS NULL ORDER BY slot_at_utc, event_id").map_err(storage)?;
    let rows = statement
        .query_map(params![workspace_id], |row| {
            Ok(CronOutboxEvent {
                event_id: Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                job_id: Uuid::parse_str(&row.get::<_, String>(1)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                slot_at_utc: row.get(2)?,
                identity: row.get(3)?,
                prompt: row.get(4)?,
            })
        })
        .map_err(storage)?;
    rows.map(|row| row.map_err(storage)).collect()
}

pub fn acknowledge(
    conn: &Connection,
    workspace_id: &str,
    event_id: Uuid,
) -> Result<bool, StorageError> {
    Ok(conn.execute("UPDATE cron_outbox SET acknowledged_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE workspace_id = ?1 AND event_id = ?2 AND acknowledged_at IS NULL", params![workspace_id, event_id.to_string()]).map_err(storage)? == 1)
}

fn storage(error: rusqlite::Error) -> StorageError {
    StorageError::new("cron_storage_failed", error.to_string())
}
