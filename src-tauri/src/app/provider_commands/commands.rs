use super::logic::{
    delete_profile, list_profile_responses, probe_provider, profile_credential, save_profile,
    set_default_profile, ProviderProbeResponse, ProviderProfileInput, ProviderProfileResponse,
};
use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::providers::profiles::ProviderCapability;
use crate::storage::repositories::provider_profiles;
use crate::storage::secrets::SecretState;
use uuid::Uuid;

#[tauri::command]
pub fn list_provider_profiles(
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
) -> Result<Vec<ProviderProfileResponse>, String> {
    with_conn(&db, |connection| {
        list_profile_responses(connection, current_workspace_id(), secrets.store())
    })
}

#[tauri::command]
pub fn save_provider_profile(
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
    profile: ProviderProfileInput,
) -> Result<ProviderProfileResponse, String> {
    with_conn_mut(&db, |connection| {
        save_profile(connection, current_workspace_id(), secrets.store(), profile)
    })
}

#[tauri::command]
pub fn set_default_provider_profile(
    db: tauri::State<DbState>,
    capability: ProviderCapability,
    profile_id: Option<String>,
) -> Result<(), String> {
    let profile_id = profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|error| format!("invalid provider profile ID: {error}"))?;
    with_conn_mut(&db, |connection| {
        set_default_profile(connection, current_workspace_id(), capability, profile_id)
    })
}

#[tauri::command]
pub async fn test_provider_profile(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
    capability: Option<ProviderCapability>,
) -> Result<ProviderProbeResponse, String> {
    let id =
        Uuid::parse_str(&id).map_err(|error| format!("invalid provider profile ID: {error}"))?;
    let record = with_conn(&db, |connection| {
        provider_profiles::get_record(connection, current_workspace_id(), id)?
            .ok_or_else(|| "provider profile not found".to_string())
    })?;
    let credential =
        profile_credential(&record.profile, record.secret_generation, secrets.store())?;
    Ok(probe_provider(record.profile, credential, capability).await)
}

#[tauri::command]
pub fn delete_provider_profile(
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
    id: String,
) -> Result<(), String> {
    let id =
        Uuid::parse_str(&id).map_err(|error| format!("invalid provider profile ID: {error}"))?;
    with_conn_mut(&db, |connection| {
        delete_profile(connection, current_workspace_id(), secrets.store(), id)
    })
}
