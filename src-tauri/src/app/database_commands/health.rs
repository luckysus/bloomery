use super::{logic, types::parse_id};
use crate::{
    database,
    db::{current_workspace_id, DbState},
    storage::{
        repositories::{database_connections as repository, settings},
        secrets::SecretState,
    },
};
use std::time::Instant;

#[tauri::command]
pub(crate) async fn test_database_connection(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
) -> Result<String, String> {
    let id = parse_id(&id)?;
    let record = logic::load_record(&db, id)?;
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
                )?;
                settings::record_audit(
                    connection,
                    current_workspace_id(),
                    "database.test",
                    &format!("database:{}", id),
                    "success",
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
                )?;
                settings::record_audit(
                    connection,
                    current_workspace_id(),
                    "database.test",
                    &format!("database:{}", id),
                    "failure",
                )
            });
            Err(error.clone())
        }
    }
}
