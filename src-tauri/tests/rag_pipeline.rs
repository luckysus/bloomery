use bloomery::providers::capabilities::EmbeddingResponse;
use bloomery::rag::index::{EmbeddingRemote, EmbeddingRemoteFactory, EmbeddingRemoteFuture};
use bloomery::rag::model::{
    DocumentVersionId, NewDocumentVersion, NewSourceDocument, SourceDocumentId, SourceLocation,
};
use bloomery::rag::parse::{DocumentBlock, ParsedDocument};
use bloomery::rag::tasks::{
    LocalRagPostprocessor, MinerUPostprocessor, MinerUTaskPayload, StoredObjectRef,
    TaskFinalization, MINERU_TASK_KIND,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::knowledge;
use bloomery::tasks::{repository as task_repository, NewTask, TaskState};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

const WORKSPACE: &str = "workspace-a";
const PROFILE: &str = "11111111-1111-4111-8111-111111111111";
const MODEL: &str = "BAAI/bge-m3";

#[test]
fn local_postprocessor_persists_chunks_embeddings_index_and_activation() {
    let fixture = PipelineFixture::new();
    let parsed = ParsedDocument {
        blocks: vec![
            DocumentBlock::Heading {
                level: 1,
                text: "Steel standard".to_string(),
                location: SourceLocation::PdfPage {
                    page: 1,
                    bbox: None,
                },
            },
            DocumentBlock::Paragraph {
                text: "Q355B yield strength requirement".to_string(),
                location: SourceLocation::PdfPage {
                    page: 2,
                    bbox: None,
                },
            },
        ],
        assets: Vec::new(),
        warnings: Vec::new(),
    };
    let ast = fixture.store(&serde_json::to_vec(&parsed).unwrap());
    let (document_id, version_id) = fixture.create_version(1);
    let payload = fixture.payload(document_id, version_id);
    let processor = LocalRagPostprocessor::new(
        fixture.database.clone(),
        fixture.root.clone(),
        Arc::new(FakeEmbeddingFactory),
    );

    let chunk_manifest = tauri::async_runtime::block_on(processor.chunk(
        WORKSPACE.to_string(),
        payload.clone(),
        ast,
    ))
    .expect("persist chunks");
    let embedding_manifest = tauri::async_runtime::block_on(processor.embed(
        WORKSPACE.to_string(),
        payload.clone(),
        Arc::new(|| false),
    ))
    .expect("persist embeddings");
    let index_manifest =
        tauri::async_runtime::block_on(processor.index(WORKSPACE.to_string(), payload.clone()))
            .expect("persist indexes");
    let mut connection = Connection::open(&fixture.database).unwrap();
    let task = task_repository::create(
        &mut connection,
        NewTask {
            workspace_id: WORKSPACE.to_string(),
            kind: MINERU_TASK_KIND.to_string(),
            payload_json: serde_json::to_string(&payload).unwrap(),
            checkpoint_json: None,
            next_run_at: None,
            progress: 95,
        },
    )
    .unwrap();
    let claimed = task_repository::claim_next(&mut connection, WORKSPACE, "2099-01-01T00:00:00Z")
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, task.id);
    drop(connection);
    let checkpoint_json = serde_json::json!({
        "stage": "activated",
        "activated_version_id": version_id,
    })
    .to_string();
    let activated = tauri::async_runtime::block_on(processor.activate(
        WORKSPACE.to_string(),
        payload,
        TaskFinalization::new(claimed.id, claimed.attempt, checkpoint_json.clone()),
    ))
    .expect("activate version and complete task");

    assert_eq!(chunk_manifest.len(), 64);
    assert_eq!(embedding_manifest.len(), 64);
    assert_eq!(index_manifest.len(), 64);
    assert_eq!(activated, version_id);
    let connection = Connection::open(&fixture.database).unwrap();
    assert_eq!(count(&connection, "knowledge_chunks"), 1);
    assert_eq!(count(&connection, "knowledge_chunk_embeddings"), 1);
    assert_eq!(count(&connection, "knowledge_chunks_fts"), 1);
    assert_eq!(count(&connection, "knowledge_vector_watermarks"), 1);
    let completed = task_repository::get(&connection, WORKSPACE, claimed.id)
        .unwrap()
        .unwrap();
    assert_eq!(completed.state, TaskState::Completed);
    assert_eq!(completed.progress, 100);
    assert_eq!(
        completed.checkpoint_json.as_deref(),
        Some(checkpoint_json.as_str())
    );
    assert_eq!(
        knowledge::get_source_document(&connection, WORKSPACE, document_id)
            .unwrap()
            .unwrap()
            .active_version_id,
        Some(version_id)
    );
}

struct FakeEmbeddingFactory;

impl EmbeddingRemoteFactory for FakeEmbeddingFactory {
    fn load(
        &self,
        workspace_id: &str,
        _profile_id: Uuid,
        _expected_revision: u64,
        _expected_secret_generation: u64,
    ) -> Result<Arc<dyn EmbeddingRemote>, bloomery::rag::index::EmbeddingError> {
        assert_eq!(workspace_id, WORKSPACE);
        Ok(Arc::new(FakeEmbeddingRemote))
    }
}

struct FakeEmbeddingRemote;

impl EmbeddingRemote for FakeEmbeddingRemote {
    fn model_id(&self) -> &str {
        MODEL
    }

    fn max_batch_size(&self) -> usize {
        64
    }

    fn embed(&self, inputs: Vec<String>) -> EmbeddingRemoteFuture {
        Box::pin(async move {
            Ok(EmbeddingResponse {
                model_id: MODEL.to_string(),
                vectors: inputs.into_iter().map(|_| vec![1.0, 2.0]).collect(),
            })
        })
    }
}

struct PipelineFixture {
    root: PathBuf,
    database: PathBuf,
}

impl PipelineFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("bloomery-pipeline-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let database = root.join("bloomery.sqlite3");
        let mut connection = Connection::open(&database).unwrap();
        migrate(&mut connection).unwrap();
        Self { root, database }
    }

    fn create_version(&self, expected_chunks: u32) -> (SourceDocumentId, DocumentVersionId) {
        let mut connection = Connection::open(&self.database).unwrap();
        let base = knowledge::create_knowledge_base(&mut connection, WORKSPACE, "Steel").unwrap();
        let document = knowledge::create_source_document(
            &mut connection,
            WORKSPACE,
            NewSourceDocument {
                knowledge_base_id: base.id,
                display_name: "Standard".to_string(),
                source_kind: "pdf".to_string(),
            },
        )
        .unwrap();
        let version = knowledge::create_document_version(
            &mut connection,
            WORKSPACE,
            NewDocumentVersion {
                document_id: document.id,
                content_sha256: digest(b"source"),
                mime_type: "application/pdf".to_string(),
                parser: "mineru".to_string(),
                parser_version: "v4".to_string(),
                chunk_policy_version: "steel-v1".to_string(),
                embedding_profile_id: PROFILE.to_string(),
                embedding_model_id: MODEL.to_string(),
                embedding_dimension: 2,
                expected_asset_count: 0,
                expected_chunk_count: expected_chunks,
            },
        )
        .unwrap();
        (document.id, version.id)
    }

    fn store(&self, bytes: &[u8]) -> StoredObjectRef {
        let hash = digest(bytes);
        let object =
            StoredObjectRef::new(&hash, format!("objects/sha256/{}/{}", &hash[..2], hash)).unwrap();
        let path = self.root.join(Path::new(object.storage_key()));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        object
    }

    fn payload(
        &self,
        document_id: SourceDocumentId,
        version_id: DocumentVersionId,
    ) -> MinerUTaskPayload {
        MinerUTaskPayload {
            document_id,
            version_id,
            provider_profile_id: "22222222-2222-4222-8222-222222222222".to_string(),
            provider_profile_revision: 1,
            provider_secret_generation: 0,
            embedding_profile_revision: 1,
            embedding_secret_generation: 0,
            source: self.store(b"source"),
            file_name: "standard.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
        }
    }
}

impl Drop for PipelineFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
