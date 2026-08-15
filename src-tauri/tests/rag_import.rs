use bloomery::providers::profiles::{ProviderKind, ProviderProfile};
use bloomery::rag::ingest::{queue_document_import, DocumentImportRequest, KnowledgeBaseTarget};
use bloomery::rag::tasks::{MinerUTaskPayload, MINERU_TASK_KIND};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::{knowledge, provider_profiles};
use bloomery::storage::secrets::{SecretError, SecretRef, SecretStore, SecretValue};
use bloomery::tasks::{repository as task_repository, TaskState};
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

const WORKSPACE: &str = "workspace-a";

#[test]
fn import_request_accepts_a_missing_optional_mineru_profile() {
    let request: DocumentImportRequest = serde_json::from_value(serde_json::json!({
        "source_path": "F:\\docs\\GB 50632.pdf",
        "knowledge_base": {
            "mode": "create",
            "name": "Steel standards"
        },
        "mineru_profile_id": null,
        "embedding_profile_id": Uuid::new_v4(),
        "embedding_dimension": 1024
    }))
    .expect("MinerU should be optional");

    assert_eq!(request.mineru_profile_id, None);
}

#[test]
fn import_queues_one_atomic_durable_graph_with_provider_pins() {
    let mut fixture = Fixture::new();
    let response = fixture.queue();

    assert!(!response.duplicate_content);
    let task = task_repository::get(&fixture.connection, WORKSPACE, response.task_id)
        .unwrap()
        .unwrap();
    assert_eq!(task.kind, MINERU_TASK_KIND);
    assert_eq!(task.state, TaskState::Queued);
    let payload: MinerUTaskPayload = serde_json::from_str(&task.payload_json).unwrap();
    assert_eq!(payload.document_id, response.document_id);
    assert_eq!(payload.version_id, response.version_id);
    assert_eq!(payload.provider_profile_revision, 1);
    assert_eq!(payload.provider_secret_generation, 0);
    assert_eq!(payload.embedding_profile_revision, 1);
    assert_eq!(payload.embedding_secret_generation, 0);
    let attempt =
        knowledge::get_ingest_attempt(&fixture.connection, WORKSPACE, response.ingest_attempt_id)
            .unwrap()
            .unwrap();
    assert_eq!(attempt.document_id, response.document_id);
    assert_eq!(attempt.version_id, Some(response.version_id));
    assert_eq!(attempt.task_id, Some(task.id.to_string()));
}

#[test]
fn import_queues_local_parser_without_a_mineru_profile() {
    let mut fixture = Fixture::new();
    let mut request = fixture.request();
    request.mineru_profile_id = None;

    let response = fixture.queue_request(request);
    let task = task_repository::get(&fixture.connection, WORKSPACE, response.task_id)
        .unwrap()
        .unwrap();
    let payload: MinerUTaskPayload = serde_json::from_str(&task.payload_json).unwrap();
    let version =
        knowledge::get_document_version(&fixture.connection, WORKSPACE, response.version_id)
            .unwrap()
            .unwrap();

    assert_eq!(payload.provider_profile_id, None);
    assert_eq!(version.parser, "local");
    assert_eq!(version.parser_version, "v1");
}

#[test]
fn import_database_failure_rolls_back_graph_but_keeps_shared_object() {
    let mut fixture = Fixture::new();
    fixture
        .connection
        .execute_batch(
            "CREATE TEMP TRIGGER fail_import_attempt
             BEFORE INSERT ON knowledge_ingest_attempts
             BEGIN SELECT RAISE(ABORT, 'injected attempt failure'); END;",
        )
        .unwrap();

    assert!(fixture.try_queue().is_err());
    for table in [
        "knowledge_bases",
        "knowledge_source_documents",
        "knowledge_document_versions",
        "knowledge_ingest_attempts",
        "background_tasks",
    ] {
        let count: i64 = fixture
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} was not rolled back");
    }
    assert_eq!(object_count(&fixture.content_root), 1);
}

#[test]
fn import_can_target_an_existing_knowledge_base() {
    let mut fixture = Fixture::new();
    let knowledge_base =
        knowledge::create_knowledge_base(&fixture.connection, WORKSPACE, "Existing base").unwrap();
    let mut request = fixture.request();
    request.knowledge_base = KnowledgeBaseTarget::Existing {
        id: knowledge_base.id,
    };

    let response = fixture.queue_request(request);

    assert_eq!(response.knowledge_base_id, knowledge_base.id);
    assert_eq!(
        knowledge::list_knowledge_bases(&fixture.connection, WORKSPACE)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn duplicate_bytes_share_one_object_but_create_independent_import_records() {
    let mut fixture = Fixture::new();
    let knowledge_base =
        knowledge::create_knowledge_base(&fixture.connection, WORKSPACE, "Shared base").unwrap();

    let first = fixture.queue_into(knowledge_base.id);
    let second = fixture.queue_into(knowledge_base.id);

    assert!(!first.duplicate_content);
    assert!(second.duplicate_content);
    assert_ne!(first.document_id, second.document_id);
    assert_ne!(first.version_id, second.version_id);
    assert_ne!(first.ingest_attempt_id, second.ingest_attempt_id);
    assert_ne!(first.task_id, second.task_id);
    assert_eq!(object_count(&fixture.content_root), 1);
    assert_eq!(
        workspace_count(&fixture.connection, "knowledge_source_documents"),
        2
    );
    assert_eq!(workspace_count(&fixture.connection, "background_tasks"), 2);
}

#[test]
fn cross_workspace_targets_are_rejected_without_logical_imports() {
    let mut fixture = Fixture::new();
    let foreign_base =
        knowledge::create_knowledge_base(&fixture.connection, "workspace-b", "Foreign").unwrap();
    let mut request = fixture.request();
    request.knowledge_base = KnowledgeBaseTarget::Existing {
        id: foreign_base.id,
    };
    assert!(fixture
        .try_queue_request(request)
        .unwrap_err()
        .contains("knowledge base not found"));
    assert_no_import_graph(&fixture.connection);

    let foreign_provider = save_profile_for(
        &mut fixture.connection,
        "workspace-b",
        ProviderKind::MinerU,
        None,
        true,
        Some("api_key"),
    );
    fixture
        .secrets
        .set(
            &SecretRef::new(foreign_provider, "api_key").unwrap(),
            &SecretValue::new("foreign-secret").unwrap(),
        )
        .unwrap();
    let mut request = fixture.request();
    request.mineru_profile_id = Some(foreign_provider);
    assert!(fixture
        .try_queue_request(request)
        .unwrap_err()
        .contains("provider profile not found"));
    assert_no_import_graph(&fixture.connection);
}

#[test]
fn import_rejects_missing_disabled_wrong_kind_and_uncredentialed_providers() {
    let mut fixture = Fixture::new();

    let mut request = fixture.request();
    request.mineru_profile_id = Some(Uuid::new_v4());
    assert!(fixture
        .try_queue_request(request)
        .unwrap_err()
        .contains("provider profile not found"));

    let mut request = fixture.request();
    request.mineru_profile_id = Some(fixture.embedding_id);
    assert!(fixture
        .try_queue_request(request)
        .unwrap_err()
        .contains("provider profile kind mismatch"));

    fixture
        .connection
        .execute(
            "UPDATE provider_profiles SET enabled = 0 WHERE id = ?1",
            [fixture.mineru_id.to_string()],
        )
        .unwrap();
    assert!(fixture
        .try_queue()
        .unwrap_err()
        .contains("provider profile is disabled"));
    fixture
        .connection
        .execute(
            "UPDATE provider_profiles SET enabled = 1 WHERE id = ?1",
            [fixture.mineru_id.to_string()],
        )
        .unwrap();

    fixture
        .secrets
        .delete(&SecretRef::new(fixture.mineru_id, "api_key").unwrap())
        .unwrap();
    assert!(fixture
        .try_queue()
        .unwrap_err()
        .contains("provider credential is not configured"));
    assert_no_import_graph(&fixture.connection);
}

#[test]
fn import_requires_an_embedding_model_and_positive_dimension() {
    let mut fixture = Fixture::new();
    let mut request = fixture.request();
    request.embedding_dimension = 0;
    assert!(fixture
        .try_queue_request(request)
        .unwrap_err()
        .contains("embedding dimension must be positive"));
    assert_eq!(object_count_if_present(&fixture.content_root), 0);

    let no_model = save_profile_for(
        &mut fixture.connection,
        WORKSPACE,
        ProviderKind::SiliconFlow,
        None,
        true,
        Some("api_key"),
    );
    fixture
        .secrets
        .set(
            &SecretRef::new(no_model, "api_key").unwrap(),
            &SecretValue::new("test-secret").unwrap(),
        )
        .unwrap();
    let mut request = fixture.request();
    request.embedding_profile_id = no_model;
    assert!(fixture
        .try_queue_request(request)
        .unwrap_err()
        .contains("embedding provider model is required"));
    assert_no_import_graph(&fixture.connection);
}

#[test]
fn import_preserves_a_chinese_display_name_without_using_it_as_a_storage_path() {
    let mut fixture = Fixture::new();
    let chinese_source = fixture.root.join("高炉炼铁标准.pdf");
    fs::rename(&fixture.source, &chinese_source).unwrap();
    fixture.source = chinese_source;

    let response = fixture.queue();

    let document =
        knowledge::get_source_document(&fixture.connection, WORKSPACE, response.document_id)
            .unwrap()
            .unwrap();
    assert_eq!(document.display_name, "高炉炼铁标准.pdf");
    let task = task_repository::get(&fixture.connection, WORKSPACE, response.task_id)
        .unwrap()
        .unwrap();
    let payload: MinerUTaskPayload = serde_json::from_str(&task.payload_json).unwrap();
    assert_eq!(payload.file_name, "高炉炼铁标准.pdf");
    assert!(!payload.source.storage_key().contains("高炉"));
}

#[derive(Default)]
struct MemorySecretStore {
    values: Mutex<HashMap<String, SecretValue>>,
}

impl SecretStore for MemorySecretStore {
    fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError> {
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

struct Fixture {
    root: PathBuf,
    content_root: PathBuf,
    source: PathBuf,
    connection: Connection,
    secrets: MemorySecretStore,
    mineru_id: Uuid,
    embedding_id: Uuid,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("bloomery-import-{}", Uuid::new_v4()));
        let content_root = root.join("content");
        fs::create_dir_all(&content_root).unwrap();
        let source = root.join("steel-standard.pdf");
        fs::write(&source, b"%PDF-1.7\nimport fixture").unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        let secrets = MemorySecretStore::default();
        let mineru_id = save_profile(&mut connection, ProviderKind::MinerU, None);
        let embedding_id = save_profile(
            &mut connection,
            ProviderKind::SiliconFlow,
            Some("BAAI/bge-m3"),
        );
        for id in [mineru_id, embedding_id] {
            secrets
                .set(
                    &SecretRef::new(id, "api_key").unwrap(),
                    &SecretValue::new("test-secret").unwrap(),
                )
                .unwrap();
        }
        Self {
            root,
            content_root,
            source,
            connection,
            secrets,
            mineru_id,
            embedding_id,
        }
    }

    fn request(&self) -> DocumentImportRequest {
        DocumentImportRequest {
            source_path: self.source.clone(),
            knowledge_base: KnowledgeBaseTarget::Create {
                name: "Steel standards".to_string(),
            },
            mineru_profile_id: Some(self.mineru_id),
            embedding_profile_id: self.embedding_id,
            embedding_dimension: 1024,
        }
    }

    fn queue(&mut self) -> bloomery::rag::ingest::DocumentImportResponse {
        self.try_queue().unwrap()
    }

    fn try_queue(&mut self) -> Result<bloomery::rag::ingest::DocumentImportResponse, String> {
        let request = self.request();
        self.try_queue_request(request)
    }

    fn queue_request(
        &mut self,
        request: DocumentImportRequest,
    ) -> bloomery::rag::ingest::DocumentImportResponse {
        self.try_queue_request(request).unwrap()
    }

    fn try_queue_request(
        &mut self,
        request: DocumentImportRequest,
    ) -> Result<bloomery::rag::ingest::DocumentImportResponse, String> {
        queue_document_import(
            &mut self.connection,
            WORKSPACE,
            &self.secrets,
            &self.content_root,
            request,
        )
    }

    fn queue_into(
        &mut self,
        knowledge_base_id: bloomery::rag::model::KnowledgeBaseId,
    ) -> bloomery::rag::ingest::DocumentImportResponse {
        let mut request = self.request();
        request.knowledge_base = KnowledgeBaseTarget::Existing {
            id: knowledge_base_id,
        };
        self.queue_request(request)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn save_profile(connection: &mut Connection, kind: ProviderKind, model_id: Option<&str>) -> Uuid {
    save_profile_for(connection, WORKSPACE, kind, model_id, true, Some("api_key"))
}

fn save_profile_for(
    connection: &mut Connection,
    workspace_id: &str,
    kind: ProviderKind,
    model_id: Option<&str>,
    enabled: bool,
    secret_ref: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();
    provider_profiles::save_record(
        connection,
        workspace_id,
        ProviderProfile {
            id,
            kind,
            display_name: format!("{kind:?}"),
            base_url: "http://127.0.0.1:9/v1".to_string(),
            model_id: model_id.map(str::to_string),
            secret_ref: secret_ref.map(str::to_string),
            enabled,
        },
    )
    .unwrap();
    id
}

fn object_count(root: &PathBuf) -> usize {
    fs::read_dir(root.join("objects/sha256"))
        .unwrap()
        .flat_map(|prefix| fs::read_dir(prefix.unwrap().path()).unwrap())
        .count()
}

fn object_count_if_present(root: &PathBuf) -> usize {
    if root.join("objects/sha256").is_dir() {
        object_count(root)
    } else {
        0
    }
}

fn workspace_count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE workspace_id = ?1"),
            [WORKSPACE],
            |row| row.get(0),
        )
        .unwrap()
}

fn assert_no_import_graph(connection: &Connection) {
    for table in [
        "knowledge_source_documents",
        "knowledge_document_versions",
        "knowledge_ingest_attempts",
        "background_tasks",
    ] {
        assert_eq!(
            workspace_count(connection, table),
            0,
            "unexpected row in {table}"
        );
    }
}
