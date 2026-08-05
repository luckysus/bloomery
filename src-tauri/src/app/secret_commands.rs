use crate::db::{current_workspace_id, with_conn, DbState};
use crate::providers::profiles::ProviderProfileRecord;
use crate::storage::repositories::provider_profiles;
use crate::storage::secrets::{
    status, SecretRef, SecretState, SecretStatus, SecretStore, SecretValue,
};
use rusqlite::Connection;
use uuid::Uuid;

fn profile_record(
    connection: &Connection,
    workspace_id: &str,
    profile_id: Uuid,
    credential_name: &str,
) -> Result<ProviderProfileRecord, String> {
    let record = provider_profiles::get_record(connection, workspace_id, profile_id)?
        .ok_or_else(|| "provider profile not found".to_string())?;
    if record.profile.secret_ref.as_deref() != Some(credential_name) {
        return Err("credential name does not match provider profile".to_string());
    }
    Ok(record)
}

fn profile_reference(record: &ProviderProfileRecord, generation: u64) -> Result<SecretRef, String> {
    SecretRef::at_generation(
        record.profile.id,
        record
            .profile
            .secret_ref
            .as_deref()
            .ok_or_else(|| "provider profile has no credential".to_string())?,
        generation,
    )
    .map_err(|error| error.to_string())
}

fn set_profile_secret(
    connection: &Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    profile_id: Uuid,
    credential_name: &str,
    value: SecretValue,
) -> Result<SecretStatus, String> {
    let record = profile_record(connection, workspace_id, profile_id, credential_name)?;
    let next_generation = record
        .secret_generation
        .checked_add(1)
        .ok_or_else(|| "provider secret generation is exhausted".to_string())?;
    let next = profile_reference(&record, next_generation)?;
    store
        .set(&next, &value)
        .map_err(|error| error.to_string())?;
    if let Err(error) = provider_profiles::activate_secret_generation(
        connection,
        workspace_id,
        profile_id,
        credential_name,
        record.secret_generation,
    ) {
        let _ = store.delete(&next);
        return Err(error);
    }
    let _ = store.delete(&profile_reference(&record, record.secret_generation)?);
    Ok(SecretStatus { configured: true })
}

fn profile_secret_status(
    connection: &Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    profile_id: Uuid,
    credential_name: &str,
) -> Result<SecretStatus, String> {
    let record = profile_record(connection, workspace_id, profile_id, credential_name)?;
    status(
        store,
        &profile_reference(&record, record.secret_generation)?,
    )
    .map_err(|error| error.to_string())
}

fn delete_profile_secret(
    connection: &Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    profile_id: Uuid,
    credential_name: &str,
) -> Result<SecretStatus, String> {
    let record = profile_record(connection, workspace_id, profile_id, credential_name)?;
    let next_generation = record
        .secret_generation
        .checked_add(1)
        .ok_or_else(|| "provider secret generation is exhausted".to_string())?;
    let next = profile_reference(&record, next_generation)?;
    match store.delete(&next) {
        Ok(()) => {}
        Err(error) if error.is_not_found() => {}
        Err(error) => return Err(error.to_string()),
    }
    provider_profiles::activate_secret_generation(
        connection,
        workspace_id,
        profile_id,
        credential_name,
        record.secret_generation,
    )?;
    let _ = store.delete(&profile_reference(&record, record.secret_generation)?);
    Ok(SecretStatus { configured: false })
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::profiles::{ProviderKind, ProviderProfile};
    use crate::storage::migrations::migrate;
    use crate::storage::repositories::provider_profiles;
    use crate::storage::secrets::{SecretError, SecretStore};
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemorySecretStore {
        values: Mutex<HashMap<String, SecretValue>>,
        fail_set: bool,
    }

    impl SecretStore for MemorySecretStore {
        fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError> {
            if self.fail_set {
                return Err(SecretError::backend("injected write failure"));
            }
            self.values
                .lock()
                .unwrap()
                .insert(reference.account(), value.clone());
            Ok(())
        }

        fn get(&self, reference: &SecretRef) -> Result<SecretValue, SecretError> {
            self.values
                .lock()
                .unwrap()
                .get(&reference.account())
                .cloned()
                .ok_or_else(SecretError::not_found)
        }

        fn delete(&self, reference: &SecretRef) -> Result<(), SecretError> {
            self.values
                .lock()
                .unwrap()
                .remove(&reference.account())
                .map(|_| ())
                .ok_or_else(SecretError::not_found)
        }
    }

    fn database() -> (Connection, Uuid) {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        let id = Uuid::new_v4();
        provider_profiles::save_record(
            &mut connection,
            "workspace-a",
            ProviderProfile {
                id,
                kind: ProviderKind::MinerU,
                display_name: "MinerU".to_string(),
                base_url: "https://mineru.example".to_string(),
                model_id: None,
                secret_ref: Some("api_key".to_string()),
                enabled: true,
            },
        )
        .unwrap();
        (connection, id)
    }

    #[test]
    fn secret_replacement_activates_new_generation_and_removes_old_value() {
        let (connection, id) = database();
        let store = MemorySecretStore::default();

        set_profile_secret(
            &connection,
            "workspace-a",
            &store,
            id,
            "api_key",
            SecretValue::new("first").unwrap(),
        )
        .unwrap();
        assert_eq!(
            provider_profiles::get_record(&connection, "workspace-a", id)
                .unwrap()
                .unwrap()
                .secret_generation,
            1
        );
        set_profile_secret(
            &connection,
            "workspace-a",
            &store,
            id,
            "api_key",
            SecretValue::new("second").unwrap(),
        )
        .unwrap();

        assert!(store
            .get(&SecretRef::at_generation(id, "api_key", 1).unwrap())
            .unwrap_err()
            .is_not_found());
        assert_eq!(
            store
                .get(&SecretRef::at_generation(id, "api_key", 2).unwrap())
                .unwrap(),
            SecretValue::new("second").unwrap()
        );
        delete_profile_secret(&connection, "workspace-a", &store, id, "api_key").unwrap();
        let record = provider_profiles::get_record(&connection, "workspace-a", id)
            .unwrap()
            .unwrap();
        assert_eq!(record.secret_generation, 3);
        assert!(store
            .get(&SecretRef::at_generation(id, "api_key", 3).unwrap())
            .unwrap_err()
            .is_not_found());
    }

    #[test]
    fn secret_write_failure_and_foreign_workspace_do_not_advance_generation() {
        let (connection, id) = database();
        let failing = MemorySecretStore {
            fail_set: true,
            ..Default::default()
        };

        assert!(set_profile_secret(
            &connection,
            "workspace-a",
            &failing,
            id,
            "api_key",
            SecretValue::new("value").unwrap(),
        )
        .is_err());
        assert!(set_profile_secret(
            &connection,
            "workspace-b",
            &MemorySecretStore::default(),
            id,
            "api_key",
            SecretValue::new("value").unwrap(),
        )
        .is_err());
        assert_eq!(
            provider_profiles::get_record(&connection, "workspace-a", id)
                .unwrap()
                .unwrap()
                .secret_generation,
            0
        );
    }
}
