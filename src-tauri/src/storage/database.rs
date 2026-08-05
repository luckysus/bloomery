use super::migrations::{migrate, MigrationReport};
use super::StorageError;
use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

pub fn open(path: impl AsRef<Path>) -> Result<(Connection, MigrationReport), StorageError> {
    let mut connection = Connection::open(path)
        .map_err(|error| StorageError::new("database_open_failed", error.to_string()))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| StorageError::new("database_config_failed", error.to_string()))?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| StorageError::new("database_config_failed", error.to_string()))?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|error| StorageError::new("database_config_failed", error.to_string()))?;
    let report = migrate(&mut connection)?;
    Ok((connection, report))
}
