use bloomery::providers::capabilities::EmbeddingResponse;
use bloomery::providers::http::{ProviderError, ProviderErrorCode};
use bloomery::rag::index::{
    embed_version, EmbeddingIndexRequest, EmbeddingRemote, EmbeddingRemoteFuture,
};
use bloomery::rag::model::{
    ChunkId, DocumentVersionId, NewChunk, NewDocumentVersion, NewSourceDocument, SourceLocation,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::knowledge;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const WORKSPACE: &str = "workspace-a";
const PROFILE: &str = "11111111-1111-4111-8111-111111111111";
const MODEL: &str = "BAAI/bge-m3";
const POLICY: &str = "steel-v1";

#[test]
fn embedding_batches_preserve_chunk_order_provider_limit_and_dimensions() {
    let (mut connection, version) = database_with_chunks(&["third", "first", "second"]);
    let remote = FakeEmbeddingRemote::new(2, vec![]);

    let manifest = tauri::async_runtime::block_on(embed_version(
        &mut connection,
        request(version),
        &remote,
        &|| false,
    ))
    .expect("embed version");

    assert_eq!(
        remote.inputs(),
        vec![vec!["third", "first"], vec!["second"]]
    );
    assert_eq!(manifest.chunk_count, 3);
    assert_eq!(manifest.sha256.len(), 64);
    assert_eq!(table_count(&connection, "knowledge_vectors"), 3);
    assert_eq!(table_count(&connection, "knowledge_chunk_embeddings"), 3);
}

#[test]
fn embedding_restart_reuses_committed_batches_after_partial_failure() {
    let (mut connection, version) = database_with_chunks(&["one", "two", "three"]);
    let first = FakeEmbeddingRemote::new(
        2,
        vec![
            Ok(None),
            Err(ProviderError::new(
                ProviderErrorCode::Network,
                None,
                "temporary network failure",
            )),
        ],
    );

    let error = tauri::async_runtime::block_on(embed_version(
        &mut connection,
        request(version),
        &first,
        &|| false,
    ))
    .unwrap_err();
    assert_eq!(error.code(), "embedding_network");
    assert!(error.retryable());
    assert_eq!(table_count(&connection, "knowledge_chunk_embeddings"), 2);

    let resumed = FakeEmbeddingRemote::new(2, vec![]);
    let manifest = tauri::async_runtime::block_on(embed_version(
        &mut connection,
        request(version),
        &resumed,
        &|| false,
    ))
    .expect("resume embedding");

    assert_eq!(resumed.inputs(), vec![vec!["three"]]);
    assert_eq!(manifest.chunk_count, 3);
    assert_eq!(table_count(&connection, "knowledge_vectors"), 3);
}

#[test]
fn duplicate_normalized_text_uses_one_vector_and_links_every_chunk() {
    let (mut connection, version) = database_with_chunks(&["same   text", "same text"]);
    let remote = FakeEmbeddingRemote::new(8, vec![]);

    tauri::async_runtime::block_on(embed_version(
        &mut connection,
        request(version),
        &remote,
        &|| false,
    ))
    .unwrap();

    assert_eq!(remote.inputs(), vec![vec!["same text"]]);
    assert_eq!(table_count(&connection, "knowledge_vectors"), 1);
    assert_eq!(table_count(&connection, "knowledge_chunk_embeddings"), 2);
}

#[test]
fn cancellation_keeps_completed_batches_and_never_marks_index_complete() {
    let (mut connection, version) = database_with_chunks(&["one", "two", "three"]);
    let cancelled = Arc::new(AtomicBool::new(false));
    let remote = FakeEmbeddingRemote::new(2, vec![]).cancel_after_first(cancelled.clone());

    let error = tauri::async_runtime::block_on(embed_version(
        &mut connection,
        request(version),
        &remote,
        &|| cancelled.load(Ordering::SeqCst),
    ))
    .unwrap_err();

    assert_eq!(error.code(), "embedding_cancelled");
    assert_eq!(table_count(&connection, "knowledge_chunk_embeddings"), 2);
    assert_eq!(table_count(&connection, "knowledge_vector_watermarks"), 0);
    let document_id = source_document_id(&connection, version);
    assert!(
        knowledge::activate_document_version(&mut connection, WORKSPACE, document_id, version,)
            .is_err()
    );
}

#[test]
fn embedding_rejects_model_count_dimension_and_non_finite_responses() {
    for (response, expected) in [
        (
            EmbeddingResponse {
                model_id: "wrong-model".to_string(),
                vectors: vec![vec![1.0, 2.0]],
            },
            "embedding_model_mismatch",
        ),
        (
            EmbeddingResponse {
                model_id: MODEL.to_string(),
                vectors: Vec::new(),
            },
            "embedding_count_mismatch",
        ),
        (
            EmbeddingResponse {
                model_id: MODEL.to_string(),
                vectors: vec![vec![1.0]],
            },
            "embedding_dimension_mismatch",
        ),
        (
            EmbeddingResponse {
                model_id: MODEL.to_string(),
                vectors: vec![vec![f32::NAN, 2.0]],
            },
            "embedding_value_invalid",
        ),
    ] {
        let (mut connection, version) = database_with_chunks(&["one"]);
        let remote = FakeEmbeddingRemote::new(8, vec![Ok(Some(response))]);
        let error = tauri::async_runtime::block_on(embed_version(
            &mut connection,
            request(version),
            &remote,
            &|| false,
        ))
        .unwrap_err();
        assert_eq!(error.code(), expected);
        assert_eq!(table_count(&connection, "knowledge_vectors"), 0);
    }
}

type FakeResponse = Result<Option<EmbeddingResponse>, ProviderError>;

struct FakeEmbeddingRemote {
    max_batch_size: usize,
    responses: Mutex<VecDeque<FakeResponse>>,
    inputs: Mutex<Vec<Vec<String>>>,
    cancel_after_first: Option<Arc<AtomicBool>>,
}

impl FakeEmbeddingRemote {
    fn new(max_batch_size: usize, responses: Vec<FakeResponse>) -> Self {
        Self {
            max_batch_size,
            responses: Mutex::new(responses.into()),
            inputs: Mutex::new(Vec::new()),
            cancel_after_first: None,
        }
    }

    fn cancel_after_first(mut self, cancelled: Arc<AtomicBool>) -> Self {
        self.cancel_after_first = Some(cancelled);
        self
    }

    fn inputs(&self) -> Vec<Vec<String>> {
        self.inputs.lock().unwrap().clone()
    }
}

impl EmbeddingRemote for FakeEmbeddingRemote {
    fn model_id(&self) -> &str {
        MODEL
    }

    fn max_batch_size(&self) -> usize {
        self.max_batch_size
    }

    fn embed(&self, inputs: Vec<String>) -> EmbeddingRemoteFuture {
        self.inputs.lock().unwrap().push(inputs.clone());
        let response = self.responses.lock().unwrap().pop_front();
        if self.inputs.lock().unwrap().len() == 1 {
            if let Some(cancelled) = &self.cancel_after_first {
                cancelled.store(true, Ordering::SeqCst);
            }
        }
        Box::pin(async move {
            match response {
                Some(Ok(Some(response))) => Ok(response),
                Some(Err(error)) => Err(error),
                _ => Ok(EmbeddingResponse {
                    model_id: MODEL.to_string(),
                    vectors: inputs
                        .iter()
                        .map(|input| vec![input.len() as f32, 1.0])
                        .collect(),
                }),
            }
        })
    }
}

fn database_with_chunks(texts: &[&str]) -> (Connection, DocumentVersionId) {
    let mut connection = Connection::open_in_memory().expect("open database");
    migrate(&mut connection).expect("migrate database");
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
            chunk_policy_version: POLICY.to_string(),
            embedding_profile_id: PROFILE.to_string(),
            embedding_model_id: MODEL.to_string(),
            embedding_dimension: 2,
            expected_asset_count: 0,
            expected_chunk_count: texts.len() as u32,
        },
    )
    .unwrap();
    for (ordinal, text) in texts.iter().enumerate() {
        knowledge::add_chunk(
            &mut connection,
            WORKSPACE,
            NewChunk {
                id: ChunkId::new(format!("chunk-{ordinal}")).unwrap(),
                version_id: version.id,
                ordinal: ordinal as u32,
                text: (*text).to_string(),
                source_location: SourceLocation::TextOffsets {
                    start: ordinal as u64,
                    end: ordinal as u64 + 1,
                },
                content_sha256: digest(text.as_bytes()),
                policy_version: POLICY.to_string(),
            },
        )
        .unwrap();
    }
    (connection, version.id)
}

fn request(version_id: DocumentVersionId) -> EmbeddingIndexRequest {
    EmbeddingIndexRequest {
        workspace_id: WORKSPACE.to_string(),
        version_id,
        provider_profile_id: PROFILE.to_string(),
        model_id: MODEL.to_string(),
        dimension: 2,
        policy_version: POLICY.to_string(),
    }
}

fn table_count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn source_document_id(
    connection: &Connection,
    version: DocumentVersionId,
) -> bloomery::rag::model::SourceDocumentId {
    let value: String = connection
        .query_row(
            "SELECT document_id FROM knowledge_document_versions WHERE id = ?1",
            [version.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    value.parse().unwrap()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
