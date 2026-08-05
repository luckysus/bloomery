use super::logic::{delete_profile_secret, profile_secret_status, set_profile_secret};
use crate::db::{current_workspace_id, with_conn, DbState};
use crate::storage::secrets::{SecretState, SecretStatus, SecretValue};
use uuid::Uuid;

#[tauri::command]
pub fn secret_set(
    db: tauri::State<DbState>,
    state: tauri::State<SecretState>,
    profile_id: String,
    credential_name: String,
    value: String,
) -> Result<SecretStatus, String> {
    let profile_id = Uuid::parse_str(&profile_id)
        .map_err(|error| format!("invalid provider profile ID: {error}"))?;
    let value = SecretValue::new(value).map_err(|error| error.to_string())?;
    with_conn(&db, |connection| {
        set_profile_secret(
            connection,
            current_workspace_id(),
            state.store(),
            profile_id,
            &credential_name,
            value,
        )
    })
}

#[tauri::command]
pub fn secret_status(
    db: tauri::State<DbState>,
    state: tauri::State<SecretState>,
    profile_id: String,
    credential_name: String,
) -> Result<SecretStatus, String> {
    let profile_id = Uuid::parse_str(&profile_id)
        .map_err(|error| format!("invalid provider profile ID: {error}"))?;
    with_conn(&db, |connection| {
        profile_secret_status(
            connection,
            current_workspace_id(),
            state.store(),
            profile_id,
            &credential_name,
        )
    })
}

#[tauri::command]
pub fn secret_delete(
    db: tauri::State<DbState>,
    state: tauri::State<SecretState>,
    profile_id: String,
    credential_name: String,
) -> Result<SecretStatus, String> {
    let profile_id = Uuid::parse_str(&profile_id)
        .map_err(|error| format!("invalid provider profile ID: {error}"))?;
    with_conn(&db, |connection| {
        delete_profile_secret(
            connection,
            current_workspace_id(),
            state.store(),
            profile_id,
            &credential_name,
        )
    })
}
