// Secret lifecycle domain logic.
use crate::providers::profiles::ProviderProfileRecord;
use crate::storage::repositories::provider_profiles;
use crate::storage::secrets::{
    status, SecretRef, SecretStatus, SecretStore, SecretValue, MAX_SECRET_GENERATION,
};
use rusqlite::Connection;
use uuid::Uuid;

pub(crate) fn profile_record(
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

pub(crate) fn profile_reference(
    record: &ProviderProfileRecord,
    generation: u64,
) -> Result<SecretRef, String> {
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

pub(crate) fn set_profile_secret(
    connection: &Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    profile_id: Uuid,
    credential_name: &str,
    value: SecretValue,
) -> Result<SecretStatus, String> {
    let record = profile_record(connection, workspace_id, profile_id, credential_name)?;
    if record.secret_generation >= MAX_SECRET_GENERATION {
        return Err("provider secret generation is exhausted".to_string());
    }
    let next_generation = record
        .secret_generation
        .checked_add(1)
        .ok_or_else(|| "provider secret generation is exhausted".to_string())?;
    let current = profile_reference(&record, record.secret_generation)?;
    let next = profile_reference(&record, next_generation)?;
    let previous = optional_secret(store, &current)?;
    store
        .set(&next, &value)
        .map_err(|error| error.to_string())?;
    if let Err(error) = delete_optional_secret(store, &current) {
        return Err(rollback_secret_transition(
            store,
            &next,
            None,
            &current,
            previous.as_ref(),
            error,
        ));
    }
    if let Err(error) = provider_profiles::activate_secret_generation(
        connection,
        workspace_id,
        profile_id,
        credential_name,
        record.secret_generation,
    ) {
        return Err(rollback_secret_transition(
            store,
            &next,
            None,
            &current,
            previous.as_ref(),
            error,
        ));
    }
    Ok(SecretStatus { configured: true })
}

pub(crate) fn profile_secret_status(
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

pub(crate) fn delete_profile_secret(
    connection: &Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    profile_id: Uuid,
    credential_name: &str,
) -> Result<SecretStatus, String> {
    let record = profile_record(connection, workspace_id, profile_id, credential_name)?;
    if record.secret_generation >= MAX_SECRET_GENERATION {
        return Err("provider secret generation is exhausted".to_string());
    }
    let next_generation = record
        .secret_generation
        .checked_add(1)
        .ok_or_else(|| "provider secret generation is exhausted".to_string())?;
    let current = profile_reference(&record, record.secret_generation)?;
    let next = profile_reference(&record, next_generation)?;
    let stale_next = optional_secret(store, &next)?;
    if let Err(error) = delete_optional_secret(store, &next) {
        return Err(rollback_secret_transition(
            store,
            &next,
            stale_next.as_ref(),
            &current,
            None,
            error,
        ));
    }
    let previous = optional_secret(store, &current)?;
    if let Err(error) = delete_optional_secret(store, &current) {
        return Err(rollback_secret_transition(
            store,
            &next,
            stale_next.as_ref(),
            &current,
            previous.as_ref(),
            error,
        ));
    }
    if let Err(error) = provider_profiles::activate_secret_generation(
        connection,
        workspace_id,
        profile_id,
        credential_name,
        record.secret_generation,
    ) {
        return Err(rollback_secret_transition(
            store,
            &next,
            stale_next.as_ref(),
            &current,
            previous.as_ref(),
            error,
        ));
    }
    Ok(SecretStatus { configured: false })
}

fn optional_secret(
    store: &dyn SecretStore,
    reference: &SecretRef,
) -> Result<Option<SecretValue>, String> {
    match store.get(reference) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn delete_optional_secret(store: &dyn SecretStore, reference: &SecretRef) -> Result<(), String> {
    match store.delete(reference) {
        Ok(()) => Ok(()),
        Err(error) if error.is_not_found() => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn rollback_secret_transition(
    store: &dyn SecretStore,
    next: &SecretRef,
    next_value: Option<&SecretValue>,
    current: &SecretRef,
    current_value: Option<&SecretValue>,
    primary_error: String,
) -> String {
    let mut rollback_errors = Vec::new();
    if let Err(error) = delete_optional_secret(store, next) {
        rollback_errors.push(error);
    }
    if let Some(value) = next_value {
        if let Err(error) = store.set(next, value) {
            rollback_errors.push(error.to_string());
        }
    }
    if let Some(value) = current_value {
        if let Err(error) = store.set(current, value) {
            rollback_errors.push(error.to_string());
        }
    }
    if rollback_errors.is_empty() {
        primary_error
    } else {
        format!(
            "{primary_error}; credential rollback failed: {}",
            rollback_errors.join("; ")
        )
    }
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
        fail_delete: bool,
        fail_delete_account: Option<String>,
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
            if self.fail_delete
                || self
                    .fail_delete_account
                    .as_deref()
                    .is_some_and(|account| account == reference.account())
            {
                return Err(SecretError::backend("injected delete failure"));
            }
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

    #[test]
    fn secret_replacement_rejects_exhausted_generation_before_writing() {
        let (connection, id) = database();
        connection
            .execute(
                "UPDATE provider_profiles SET secret_generation = ?1 WHERE id = ?2",
                rusqlite::params![4096_i64, id.to_string()],
            )
            .unwrap();
        let store = MemorySecretStore::default();

        let error = set_profile_secret(
            &connection,
            "workspace-a",
            &store,
            id,
            "api_key",
            SecretValue::new("value").unwrap(),
        )
        .expect_err("exhausted secret generation must fail before keyring writes");

        assert!(error.contains("provider secret generation is exhausted"));
        assert!(store
            .get(&SecretRef::at_generation(id, "api_key", 4097).unwrap())
            .unwrap_err()
            .is_not_found());
        let generation: i64 = connection
            .query_row(
                "SELECT secret_generation FROM provider_profiles WHERE id = ?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(generation, 4096);
    }

    #[test]
    fn secret_replacement_does_not_advance_when_old_secret_cleanup_fails() {
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
        let failing = MemorySecretStore {
            values: store.values,
            fail_delete_account: Some(
                SecretRef::at_generation(id, "api_key", 1)
                    .unwrap()
                    .account(),
            ),
            ..Default::default()
        };

        let error = set_profile_secret(
            &connection,
            "workspace-a",
            &failing,
            id,
            "api_key",
            SecretValue::new("second").unwrap(),
        )
        .expect_err("old secret cleanup failure must fail replacement");

        assert!(error.contains("injected delete failure"));
        assert_eq!(
            provider_profiles::get_record(&connection, "workspace-a", id)
                .unwrap()
                .unwrap()
                .secret_generation,
            1
        );
        assert_eq!(
            failing
                .get(&SecretRef::at_generation(id, "api_key", 1).unwrap())
                .unwrap(),
            SecretValue::new("first").unwrap()
        );
        assert!(failing
            .get(&SecretRef::at_generation(id, "api_key", 2).unwrap())
            .unwrap_err()
            .is_not_found());
    }

    #[test]
    fn secret_deletion_does_not_advance_when_old_secret_cleanup_fails() {
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
        let failing = MemorySecretStore {
            values: store.values,
            fail_delete: true,
            ..Default::default()
        };

        let error = delete_profile_secret(&connection, "workspace-a", &failing, id, "api_key")
            .expect_err("old secret cleanup failure must fail deletion");

        assert!(error.contains("injected delete failure"));
        assert_eq!(
            provider_profiles::get_record(&connection, "workspace-a", id)
                .unwrap()
                .unwrap()
                .secret_generation,
            1
        );
        assert_eq!(
            failing
                .get(&SecretRef::at_generation(id, "api_key", 1).unwrap())
                .unwrap(),
            SecretValue::new("first").unwrap()
        );
    }
}
