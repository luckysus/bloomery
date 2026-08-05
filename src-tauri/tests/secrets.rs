use bloomery::storage::secrets::{
    KeyringSecretStore, SecretError, SecretRef, SecretStatus, SecretStore, SecretValue,
};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
struct MemorySecretStore {
    values: Mutex<HashMap<String, SecretValue>>,
}

impl SecretStore for MemorySecretStore {
    fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::backend("memory secret store poisoned"))?
            .insert(reference.account(), value.clone());
        Ok(())
    }

    fn get(&self, reference: &SecretRef) -> Result<SecretValue, SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::backend("memory secret store poisoned"))?
            .get(&reference.account())
            .cloned()
            .ok_or_else(SecretError::not_found)
    }

    fn delete(&self, reference: &SecretRef) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::backend("memory secret store poisoned"))?
            .remove(&reference.account())
            .map(|_| ())
            .ok_or_else(SecretError::not_found)
    }
}

fn reference() -> SecretRef {
    SecretRef::new(Uuid::new_v4(), "api_key").expect("valid secret reference")
}

#[test]
fn secret_value_debug_is_redacted_and_store_round_trips() {
    let store = MemorySecretStore::default();
    let reference = reference();
    let value = SecretValue::new("sk-test-value").expect("non-empty secret");

    assert_eq!(format!("{value:?}"), "SecretValue([REDACTED])");
    store.set(&reference, &value).expect("set secret");
    assert_eq!(store.get(&reference).unwrap(), value);
    store.delete(&reference).expect("delete secret");
    assert_eq!(
        store.get(&reference).unwrap_err().code(),
        "secret_not_found"
    );
}

#[test]
fn secret_reference_and_status_are_safe_to_serialize() {
    let profile_id = Uuid::new_v4();
    let reference = SecretRef::new(profile_id, "mineru-token").unwrap();
    let status = SecretStatus { configured: true };

    assert_eq!(reference.account(), format!("{profile_id}/mineru-token"));
    assert_eq!(
        SecretRef::at_generation(profile_id, "mineru-token", 0)
            .unwrap()
            .account(),
        format!("{profile_id}/mineru-token")
    );
    assert_eq!(
        SecretRef::at_generation(profile_id, "mineru-token", 3)
            .unwrap()
            .account(),
        format!("{profile_id}/mineru-token/3")
    );
    assert!(SecretRef::new(profile_id, "../token").is_err());
    assert!(SecretValue::new("   ").is_err());
    assert_eq!(
        serde_json::to_value(status).unwrap(),
        serde_json::json!({"configured": true})
    );
}

#[test]
#[ignore = "writes one disposable entry to Windows Credential Manager"]
#[cfg(windows)]
fn windows_credential_manager_smoke() {
    struct Cleanup<'a> {
        store: &'a KeyringSecretStore,
        reference: &'a SecretRef,
    }

    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = self.store.delete(self.reference);
        }
    }

    let store = KeyringSecretStore;
    let reference = SecretRef::new(Uuid::new_v4(), "smoke").unwrap();
    let _cleanup = Cleanup {
        store: &store,
        reference: &reference,
    };
    let value = SecretValue::new(format!("bloomery-smoke-{}", Uuid::new_v4())).unwrap();

    store.set(&reference, &value).expect("set keyring secret");
    assert_eq!(store.get(&reference).unwrap(), value);
    store.delete(&reference).expect("delete keyring secret");
    assert_eq!(
        store.get(&reference).unwrap_err().code(),
        "secret_not_found"
    );
}
