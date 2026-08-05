use bloomery::providers::capabilities::{RerankDocument, RerankResult};
use bloomery::providers::http::{ProviderError, ProviderErrorCode};
use bloomery::rag::index::vector::{
    CandidateFilter, IndexError, IndexWatermark, VectorHit, VectorIndex,
};
use bloomery::rag::model::{
    ChunkId, DocumentVersionId, KnowledgeBaseId, SourceDocumentId, SourceLocation,
};
use bloomery::rag::rerank::{
    rerank_candidates, RerankDegradationReason, RerankProviderState, RerankRemote,
    RerankRemoteFuture,
};
use bloomery::rag::retrieve::filter::active_versions;
use bloomery::rag::retrieve::rrf::{reciprocal_rank_fusion, FusedChunk, RankedChunk};
use bloomery::rag::retrieve::{retrieve, HybridSearchRequest, RetrievedChunk};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::knowledge;
use rusqlite::{params, Connection};
use std::str::FromStr;
use std::sync::Mutex;

const WORKSPACE: &str = "workspace-a";
const PROFILE: &str = "11111111-1111-4111-8111-111111111111";
const MODEL: &str = "BAAI/bge-m3";

#[test]
fn rrf_handles_missing_lists_duplicates_ties_and_candidate_limits() {
    let version = DocumentVersionId::from_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
    let lexical = vec![
        ranked(version, "a"),
        ranked(version, "a"),
        ranked(version, "b"),
    ];
    let dense = vec![
        ranked(version, "c"),
        ranked(version, "b"),
        ranked(version, "c"),
    ];

    let fused = reciprocal_rank_fusion(&lexical, &dense, 60, 10);
    assert_eq!(ids(&fused), vec!["b", "a", "c"]);
    assert_eq!(fused[0].lexical_rank, Some(2));
    assert_eq!(fused[0].dense_rank, Some(2));
    assert_eq!(fused[1].rrf_score, fused[2].rrf_score);

    let lexical_only = reciprocal_rank_fusion(&lexical, &[], 60, 1);
    assert_eq!(ids(&lexical_only), vec!["a"]);
    assert_eq!(lexical_only[0].dense_rank, None);
    let dense_only = reciprocal_rank_fusion(&[], &dense, 60, 2);
    assert_eq!(ids(&dense_only), vec!["c", "b"]);
    assert_eq!(dense_only[0].lexical_rank, None);
    assert!(reciprocal_rank_fusion(&[], &dense, 60, 0).is_empty());
}

#[test]
fn active_version_filter_restricts_workspace_bases_and_inactive_versions() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let selected = base("11111111-1111-4111-8111-111111111111");
    let other = base("22222222-2222-4222-8222-222222222222");
    seed_base(&connection, selected, "Selected");
    seed_base(&connection, other, "Other");
    let active = version("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    let inactive = version("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
    let other_active = version("cccccccc-cccc-4ccc-8ccc-cccccccccccc");
    seed_document(
        &connection,
        "33333333-3333-4333-8333-333333333333",
        selected,
        active,
    );
    seed_version(
        &connection,
        active,
        "33333333-3333-4333-8333-333333333333",
        'a',
    );
    seed_version(
        &connection,
        inactive,
        "33333333-3333-4333-8333-333333333333",
        'b',
    );
    seed_document(
        &connection,
        "44444444-4444-4444-8444-444444444444",
        other,
        other_active,
    );
    seed_version(
        &connection,
        other_active,
        "44444444-4444-4444-8444-444444444444",
        'c',
    );

    assert_eq!(
        active_versions(&connection, WORKSPACE, &[selected, selected]).unwrap(),
        vec![active]
    );
    assert!(active_versions(&connection, WORKSPACE, &[])
        .unwrap()
        .is_empty());
    assert!(active_versions(&connection, "workspace-b", &[selected])
        .unwrap()
        .is_empty());
}

#[test]
fn hybrid_retrieval_filters_before_fusion_and_fetches_authoritative_text() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let selected = base("11111111-1111-4111-8111-111111111111");
    let other = base("22222222-2222-4222-8222-222222222222");
    seed_base(&connection, selected, "Selected");
    seed_base(&connection, other, "Other");
    let active = version("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    let inactive = version("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
    let other_active = version("cccccccc-cccc-4ccc-8ccc-cccccccccccc");
    seed_document(
        &connection,
        "33333333-3333-4333-8333-333333333333",
        selected,
        active,
    );
    seed_version(
        &connection,
        active,
        "33333333-3333-4333-8333-333333333333",
        'a',
    );
    seed_chunk(&mut connection, active, "lexical", 0, "yield strength rule");
    seed_chunk(&mut connection, active, "semantic", 1, "fracture toughness");
    seed_version(
        &connection,
        inactive,
        "33333333-3333-4333-8333-333333333333",
        'b',
    );
    seed_chunk(
        &mut connection,
        inactive,
        "inactive",
        0,
        "obsolete yield strength",
    );
    seed_document(
        &connection,
        "44444444-4444-4444-8444-444444444444",
        other,
        other_active,
    );
    seed_version(
        &connection,
        other_active,
        "44444444-4444-4444-8444-444444444444",
        'c',
    );
    seed_chunk(
        &mut connection,
        other_active,
        "other",
        0,
        "other yield strength",
    );
    connection
        .execute(
            "UPDATE knowledge_chunks SET text = 'authoritative current text'
             WHERE version_id = ?1 AND id = 'lexical'",
            [active.to_string()],
        )
        .unwrap();

    let index = FakeVectorIndex::new(vec![
        hit(inactive, "inactive", 0.01),
        hit(other_active, "other", 0.02),
        hit(active, "semantic", 0.03),
        hit(active, "lexical", 0.04),
        hit(active, "semantic", 0.05),
    ]);
    let results = retrieve(
        &connection,
        &index,
        &HybridSearchRequest {
            workspace_id: WORKSPACE.to_string(),
            query: "yield strength".to_string(),
            query_vector: vec![1.0, 0.0],
            knowledge_base_ids: vec![selected],
            lexical_limit: 10,
            dense_limit: 10,
            candidate_limit: 2,
            rrf_k: 60,
        },
    )
    .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].chunk_id.as_str(), "lexical");
    assert_eq!(results[0].text, "authoritative current text");
    assert_eq!(results[0].knowledge_base_id, selected);
    assert_eq!(results[1].chunk_id.as_str(), "semantic");
    assert!(results.iter().all(|result| result.version_id == active));

    let dense_only = retrieve(
        &connection,
        &index,
        &HybridSearchRequest {
            workspace_id: WORKSPACE.to_string(),
            query: String::new(),
            query_vector: vec![1.0, 0.0],
            knowledge_base_ids: vec![selected],
            lexical_limit: 0,
            dense_limit: 10,
            candidate_limit: 2,
            rrf_k: 60,
        },
    )
    .unwrap();
    assert_eq!(
        dense_only
            .iter()
            .map(|result| result.chunk_id.as_str())
            .collect::<Vec<_>>(),
        vec!["semantic", "lexical"]
    );
}

#[test]
fn rerank_reorders_only_the_bounded_prefix_and_preserves_candidate_identity() {
    let provider = FakeReranker::new(
        10,
        Ok(vec![
            rerank_result("candidate-2", 0.9),
            rerank_result("candidate-1", 0.1),
        ]),
    );
    let outcome = tauri::async_runtime::block_on(rerank_candidates(
        "yield strength",
        retrieved_candidates(3),
        RerankProviderState::Ready(&provider),
        2,
        &|| false,
    ));

    assert_eq!(
        chunk_ids(&outcome.chunks),
        vec!["candidate-2", "candidate-1", "candidate-3"]
    );
    assert_eq!(outcome.chunks[0].rerank_score, Some(0.9));
    assert_eq!(outcome.chunks[1].rerank_score, Some(0.1));
    assert_eq!(outcome.chunks[2].rerank_score, None);
    assert_eq!(outcome.degradation, None);
    assert_eq!(
        provider.seen_ids(),
        vec![rerank_id("candidate-1"), rerank_id("candidate-2")]
    );
}

#[test]
fn rerank_missing_key_and_quota_preserve_rrf_order_with_structured_degradation() {
    let original = retrieved_candidates(3);
    let missing_key = tauri::async_runtime::block_on(rerank_candidates(
        "query",
        original.clone(),
        RerankProviderState::Unavailable(RerankDegradationReason::MissingCredential),
        3,
        &|| false,
    ));
    assert_eq!(missing_key.chunks, original);
    assert_eq!(
        missing_key.degradation,
        Some(RerankDegradationReason::MissingCredential)
    );

    let quota = FakeReranker::error(ProviderErrorCode::Quota);
    let outcome = tauri::async_runtime::block_on(rerank_candidates(
        "query",
        original.clone(),
        RerankProviderState::Ready(&quota),
        3,
        &|| false,
    ));
    assert_eq!(outcome.chunks, original);
    assert_eq!(outcome.degradation, Some(RerankDegradationReason::Quota));
}

#[test]
fn rerank_timeout_and_cancellation_do_not_block_retrieval() {
    let original = retrieved_candidates(2);
    let timeout = FakeReranker::error(ProviderErrorCode::Timeout);
    let timed_out = tauri::async_runtime::block_on(rerank_candidates(
        "query",
        original.clone(),
        RerankProviderState::Ready(&timeout),
        2,
        &|| false,
    ));
    assert_eq!(timed_out.chunks, original);
    assert_eq!(
        timed_out.degradation,
        Some(RerankDegradationReason::Timeout)
    );

    let provider = FakeReranker::new(10, Ok(Vec::new()));
    let cancelled = tauri::async_runtime::block_on(rerank_candidates(
        "query",
        original.clone(),
        RerankProviderState::Ready(&provider),
        2,
        &|| true,
    ));
    assert_eq!(cancelled.chunks, original);
    assert_eq!(
        cancelled.degradation,
        Some(RerankDegradationReason::Cancelled)
    );
    assert!(provider.seen_ids().is_empty());
}

#[test]
fn rerank_rejects_malformed_counts_ids_duplicates_and_scores() {
    let original = retrieved_candidates(2);
    let malformed = [
        vec![rerank_result("candidate-1", 0.5)],
        vec![
            rerank_result("candidate-1", 0.5),
            RerankResult {
                id: "unknown".to_string(),
                score: 0.4,
            },
        ],
        vec![
            rerank_result("candidate-1", 0.5),
            rerank_result("candidate-1", 0.4),
        ],
        vec![
            rerank_result("candidate-1", f32::NAN),
            rerank_result("candidate-2", 0.4),
        ],
    ];

    for response in malformed {
        let provider = FakeReranker::new(10, Ok(response));
        let outcome = tauri::async_runtime::block_on(rerank_candidates(
            "query",
            original.clone(),
            RerankProviderState::Ready(&provider),
            2,
            &|| false,
        ));
        assert_eq!(outcome.chunks, original);
        assert_eq!(
            outcome.degradation,
            Some(RerankDegradationReason::MalformedResponse)
        );
    }
}

fn ranked(version_id: DocumentVersionId, chunk_id: &str) -> RankedChunk {
    RankedChunk {
        version_id,
        chunk_id: ChunkId::new(chunk_id).unwrap(),
    }
}

fn ids(hits: &[FusedChunk]) -> Vec<&str> {
    hits.iter().map(|hit| hit.chunk_id.as_str()).collect()
}

fn chunk_ids(chunks: &[RetrievedChunk]) -> Vec<&str> {
    chunks.iter().map(|chunk| chunk.chunk_id.as_str()).collect()
}

fn retrieved_candidates(count: usize) -> Vec<RetrievedChunk> {
    let knowledge_base_id = base("11111111-1111-4111-8111-111111111111");
    let document_id = SourceDocumentId::from_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap();
    let version_id = version("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    (1..=count)
        .map(|position| RetrievedChunk {
            knowledge_base_id,
            document_id,
            version_id,
            chunk_id: ChunkId::new(format!("candidate-{position}")).unwrap(),
            source_name: "steel-spec.txt".to_string(),
            source_location: SourceLocation::TextOffsets {
                start: position as u64,
                end: position as u64 + 1,
            },
            text: format!("candidate text {position}"),
            lexical_rank: Some(position),
            dense_rank: Some(position),
            rrf_score: 1.0 / position as f64,
            rerank_score: None,
        })
        .collect()
}

fn rerank_id(chunk_id: &str) -> String {
    format!("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa:{chunk_id}")
}

fn rerank_result(chunk_id: &str, score: f32) -> RerankResult {
    RerankResult {
        id: rerank_id(chunk_id),
        score,
    }
}

struct FakeReranker {
    max_documents: usize,
    response: Result<Vec<RerankResult>, ProviderError>,
    seen: Mutex<Vec<RerankDocument>>,
}

impl FakeReranker {
    fn new(max_documents: usize, response: Result<Vec<RerankResult>, ProviderError>) -> Self {
        Self {
            max_documents,
            response,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn error(code: ProviderErrorCode) -> Self {
        Self::new(10, Err(ProviderError::new(code, None, "test failure")))
    }

    fn seen_ids(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|document| document.id.clone())
            .collect()
    }
}

impl RerankRemote for FakeReranker {
    fn max_documents(&self) -> usize {
        self.max_documents
    }

    fn rerank(&self, _query: String, documents: Vec<RerankDocument>) -> RerankRemoteFuture {
        *self.seen.lock().unwrap() = documents;
        let response = self.response.clone();
        Box::pin(async move { response })
    }
}

fn base(value: &str) -> KnowledgeBaseId {
    KnowledgeBaseId::from_str(value).unwrap()
}

fn version(value: &str) -> DocumentVersionId {
    DocumentVersionId::from_str(value).unwrap()
}

fn seed_base(connection: &Connection, id: KnowledgeBaseId, name: &str) {
    connection
        .execute(
            "INSERT INTO knowledge_bases (id, workspace_id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'now', 'now')",
            params![id.to_string(), WORKSPACE, name],
        )
        .unwrap();
}

fn seed_document(
    connection: &Connection,
    id: &str,
    knowledge_base_id: KnowledgeBaseId,
    active_version_id: DocumentVersionId,
) {
    connection
        .execute(
            "INSERT INTO knowledge_source_documents
             (id, workspace_id, knowledge_base_id, display_name, source_kind,
              active_version_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?1, 'test', ?4, 'now', 'now')",
            params![
                id,
                WORKSPACE,
                knowledge_base_id.to_string(),
                active_version_id.to_string()
            ],
        )
        .unwrap();
}

fn seed_version(connection: &Connection, id: DocumentVersionId, document_id: &str, hash: char) {
    connection
        .execute(
            "INSERT INTO knowledge_document_versions
             (id, workspace_id, document_id, content_sha256, mime_type, parser,
              parser_version, chunk_policy_version, embedding_profile_id,
              embedding_model_id, embedding_dimension, expected_asset_count,
              expected_chunk_count, created_at)
             VALUES (?1, ?2, ?3, ?4, 'text/plain', 'test', '1', 'steel-v1',
                     'profile', 'model', 2, 0, 0, 'now')",
            params![
                id.to_string(),
                WORKSPACE,
                document_id,
                hash.to_string().repeat(64)
            ],
        )
        .unwrap();
}

fn seed_chunk(
    connection: &mut Connection,
    version_id: DocumentVersionId,
    chunk_id: &str,
    ordinal: u32,
    text: &str,
) {
    connection
        .execute(
            "INSERT INTO knowledge_chunks
             (id, workspace_id, version_id, ordinal, text, source_location_json,
              content_sha256, policy_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     '{\"kind\":\"text_offsets\",\"start\":0,\"end\":1}',
                     ?6, 'steel-v1', 'now')",
            params![
                chunk_id,
                WORKSPACE,
                version_id.to_string(),
                ordinal,
                text,
                format!("{:064x}", ordinal + 1)
            ],
        )
        .unwrap();
    knowledge::index_chunk_fts(
        connection,
        WORKSPACE,
        version_id,
        &ChunkId::new(chunk_id).unwrap(),
    )
    .unwrap();
}

fn hit(version_id: DocumentVersionId, chunk_id: &str, distance: f32) -> VectorHit {
    VectorHit {
        version_id,
        chunk_id: ChunkId::new(chunk_id).unwrap(),
        distance,
    }
}

struct FakeVectorIndex {
    watermark: IndexWatermark,
    hits: Vec<VectorHit>,
}

impl FakeVectorIndex {
    fn new(hits: Vec<VectorHit>) -> Self {
        Self {
            watermark: IndexWatermark {
                format_version: 1,
                workspace_id: WORKSPACE.to_string(),
                provider_profile_id: PROFILE.to_string(),
                model_id: MODEL.to_string(),
                dimension: 2,
                chunk_count: hits.len() as u32,
                sqlite_watermark: "fixture".to_string(),
            },
            hits,
        }
    }
}

impl VectorIndex for FakeVectorIndex {
    fn search(
        &self,
        _query: &[f32],
        limit: usize,
        _filter: &CandidateFilter,
    ) -> Result<Vec<VectorHit>, IndexError> {
        Ok(self.hits.iter().take(limit).cloned().collect())
    }

    fn watermark(&self) -> &IndexWatermark {
        &self.watermark
    }
}
