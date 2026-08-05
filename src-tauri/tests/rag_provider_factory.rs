use bloomery::providers::profiles::{ProviderKind, ProviderProfile};
use bloomery::providers::siliconflow::DEFAULT_EMBEDDING_MODEL;
use bloomery::rag::index::EmbeddingRemoteFactory;
use bloomery::rag::tasks::{MinerURemoteFactory, RuntimeProviderFactory};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::provider_profiles;
use bloomery::storage::secrets::{SecretError, SecretRef, SecretStore, SecretValue};
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const WORKSPACE: &str = "workspace-a";

#[test]
fn runtime_factory_loads_workspace_scoped_mineru_and_siliconflow_profiles() {
    let fixture = FactoryFixture::new();
    let mineru_id = fixture.save_profile(ProviderKind::MinerU, None, true);
    let siliconflow_id = fixture.save_profile(
        ProviderKind::SiliconFlow,
        Some(DEFAULT_EMBEDDING_MODEL),
        true,
    );
    fixture.set_secret(mineru_id);
    fixture.set_secret(siliconflow_id);
    let factory = fixture.factory();

    MinerURemoteFactory::load(&factory, WORKSPACE, mineru_id, 1, 0).expect("load MinerU provider");
    let embedding = EmbeddingRemoteFactory::load(&factory, WORKSPACE, siliconflow_id, 1, 0)
        .expect("load SiliconFlow provider");

    assert_eq!(embedding.model_id(), DEFAULT_EMBEDDING_MODEL);
    assert_eq!(embedding.max_batch_size(), 64);
}

#[test]
fn runtime_factory_rejects_cross_workspace_disabled_kind_and_missing_secret() {
    let fixture = FactoryFixture::new();
    let mineru_id = fixture.save_profile(ProviderKind::MinerU, None, true);
    let disabled_id = fixture.save_profile(
        ProviderKind::SiliconFlow,
        Some(DEFAULT_EMBEDDING_MODEL),
        false,
    );
    fixture.set_secret(mineru_id);
    fixture.set_secret(disabled_id);
    let factory = fixture.factory();

    assert_eq!(
        MinerURemoteFactory::load(&factory, "workspace-b", mineru_id, 1, 0)
            .err()
            .expect("cross-workspace profile must fail")
            .code(),
        "mineru_profile_missing"
    );
    assert_eq!(
        MinerURemoteFactory::load(&factory, WORKSPACE, disabled_id, 1, 0)
            .err()
            .expect("wrong provider kind must fail")
            .code(),
        "mineru_profile_kind_mismatch"
    );
    assert_eq!(
        EmbeddingRemoteFactory::load(&factory, WORKSPACE, disabled_id, 1, 0)
            .err()
            .expect("disabled profile must fail")
            .code(),
        "embedding_profile_disabled"
    );

    let missing_secret = fixture.save_profile(
        ProviderKind::SiliconFlow,
        Some(DEFAULT_EMBEDDING_MODEL),
        true,
    );
    assert_eq!(
        EmbeddingRemoteFactory::load(&factory, WORKSPACE, missing_secret, 1, 0)
            .err()
            .expect("missing secret must fail")
            .code(),
        "embedding_authentication"
    );
}

#[test]
fn runtime_factory_marks_secret_backend_failures_retryable() {
    let fixture = FactoryFixture::new();
    let mineru_id = fixture.save_profile(ProviderKind::MinerU, None, true);
    let factory =
        RuntimeProviderFactory::new(fixture.database.clone(), Arc::new(FailingSecretStore));

    let mineru = MinerURemoteFactory::load(&factory, WORKSPACE, mineru_id, 1, 0)
        .err()
        .expect("secret backend failure");
    let embedding_id = fixture.save_profile(
        ProviderKind::SiliconFlow,
        Some(DEFAULT_EMBEDDING_MODEL),
        true,
    );
    let embedding = EmbeddingRemoteFactory::load(&factory, WORKSPACE, embedding_id, 1, 0)
        .err()
        .expect("secret backend failure");

    assert!(mineru.is_retryable());
    assert!(embedding.retryable());
}

#[test]
fn runtime_factory_rejects_revision_and_secret_generation_drift_before_secret_access() {
    let fixture = FactoryFixture::new();
    let mineru_id = fixture.save_profile(ProviderKind::MinerU, None, true);
    let embedding_id = fixture.save_profile(
        ProviderKind::SiliconFlow,
        Some(DEFAULT_EMBEDDING_MODEL),
        true,
    );
    let factory =
        RuntimeProviderFactory::new(fixture.database.clone(), Arc::new(FailingSecretStore));

    assert_eq!(
        MinerURemoteFactory::load(&factory, WORKSPACE, mineru_id, 2, 0)
            .err()
            .expect("revision drift")
            .code(),
        "mineru_profile_revision_mismatch"
    );
    assert_eq!(
        MinerURemoteFactory::load(&factory, WORKSPACE, mineru_id, 1, 1)
            .err()
            .expect("secret generation drift")
            .code(),
        "mineru_secret_generation_mismatch"
    );
    assert_eq!(
        EmbeddingRemoteFactory::load(&factory, WORKSPACE, embedding_id, 2, 0)
            .err()
            .expect("revision drift")
            .code(),
        "embedding_profile_revision_mismatch"
    );
    assert_eq!(
        EmbeddingRemoteFactory::load(&factory, WORKSPACE, embedding_id, 1, 1)
            .err()
            .expect("secret generation drift")
            .code(),
        "embedding_secret_generation_mismatch"
    );
}

#[derive(Default)]
struct MemorySecretStore {
    values: Mutex<HashMap<String, SecretValue>>,
}

struct FailingSecretStore;

impl SecretStore for FailingSecretStore {
    fn set(&self, _reference: &SecretRef, _value: &SecretValue) -> Result<(), SecretError> {
        Err(SecretError::backend("credential manager unavailable"))
    }

    fn get(&self, _reference: &SecretRef) -> Result<SecretValue, SecretError> {
        Err(SecretError::backend("credential manager unavailable"))
    }

    fn delete(&self, _reference: &SecretRef) -> Result<(), SecretError> {
        Err(SecretError::backend("credential manager unavailable"))
    }
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

struct FactoryFixture {
    root: PathBuf,
    database: PathBuf,
    secrets: Arc<MemorySecretStore>,
}

impl FactoryFixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("bloomery-provider-factory-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let database = root.join("bloomery.sqlite3");
        let mut connection = Connection::open(&database).unwrap();
        migrate(&mut connection).unwrap();
        Self {
            root,
            database,
            secrets: Arc::new(MemorySecretStore::default()),
        }
    }

    fn save_profile(&self, kind: ProviderKind, model_id: Option<&str>, enabled: bool) -> Uuid {
        let id = Uuid::new_v4();
        let mut connection = Connection::open(&self.database).unwrap();
        provider_profiles::save(
            &mut connection,
            WORKSPACE,
            ProviderProfile {
                id,
                kind,
                display_name: format!("{kind:?}"),
                base_url: "http://127.0.0.1:9/v1".to_string(),
                model_id: model_id.map(str::to_string),
                secret_ref: Some("api_key".to_string()),
                enabled,
            },
        )
        .unwrap();
        id
    }

    fn set_secret(&self, profile_id: Uuid) {
        self.secrets
            .set(
                &SecretRef::new(profile_id, "api_key").unwrap(),
                &SecretValue::new("test-secret").unwrap(),
            )
            .unwrap();
    }

    fn factory(&self) -> RuntimeProviderFactory {
        RuntimeProviderFactory::new(self.database.clone(), self.secrets.clone())
    }
}

impl Drop for FactoryFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
