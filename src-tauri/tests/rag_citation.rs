use bloomery::rag::citation::{
    load_evidence_pack, persist_evidence_pack, resolve_citation, CitationSourceState,
    RetrievalConfigSnapshot,
};
use bloomery::rag::model::{
    ChunkId, DocumentVersionId, KnowledgeBaseId, SourceDocumentId, SourceLocation,
};
use bloomery::rag::rerank::RerankDegradationReason;
use bloomery::rag::retrieve::RetrievedChunk;
use bloomery::storage::migrations::migrate;
use rusqlite::{params, Connection};
use std::str::FromStr;

const WORKSPACE: &str = "workspace-a";
const BASE_ID: &str = "11111111-1111-4111-8111-111111111111";
const DOCUMENT_ID: &str = "22222222-2222-4222-8222-222222222222";
const VERSION_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

#[test]
fn evidence_pack_persists_rank_scores_locations_models_and_assets() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    seed_active_source(&connection, VERSION_ID, "steel-reference.pdf");
    let image_location = SourceLocation::PdfPage {
        page: 5,
        bbox: None,
    };
    seed_asset(&connection, VERSION_ID, &image_location);
    let chunks = vec![
        chunk(
            "pdf",
            SourceLocation::PdfPage {
                page: 3,
                bbox: None,
            },
            1,
        ),
        chunk(
            "table",
            SourceLocation::SheetRange {
                sheet: "Heat Data".to_string(),
                range: "A1:C3".to_string(),
            },
            2,
        ),
        chunk(
            "heading",
            SourceLocation::Heading {
                path: vec!["Rolling".to_string(), "Cooling".to_string()],
            },
            3,
        ),
        chunk(
            "offsets",
            SourceLocation::TextOffsets { start: 20, end: 48 },
            4,
        ),
        chunk("image", image_location, 5),
    ];

    let pack = persist_evidence_pack(
        &connection,
        WORKSPACE,
        "How does controlled cooling affect strength?",
        config(),
        chunks,
    )
    .unwrap();
    let loaded = load_evidence_pack(&connection, WORKSPACE, pack.id)
        .unwrap()
        .unwrap();

    assert_eq!(loaded, pack);
    assert_eq!(loaded.configuration.embedding_model_id, "BAAI/bge-m3");
    assert_eq!(
        loaded.configuration.rerank_model_id.as_deref(),
        Some("BAAI/bge-reranker-v2-m3")
    );
    assert_eq!(loaded.evidence[0].chunk.lexical_rank, Some(1));
    assert_eq!(loaded.evidence[0].chunk.dense_rank, Some(2));
    assert_eq!(loaded.evidence[0].chunk.rrf_score, 0.75);
    assert_eq!(loaded.evidence[0].chunk.rerank_score, Some(0.9));

    let labels = (1..=4)
        .map(|number| {
            resolve_citation(&connection, WORKSPACE, pack.id, number)
                .unwrap()
                .unwrap()
                .label
        })
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        vec![
            "steel-reference.pdf, page 3",
            "steel-reference.pdf, Heat Data!A1:C3",
            "steel-reference.pdf, Rolling > Cooling",
            "steel-reference.pdf, characters 20-48",
        ]
    );
    let image = resolve_citation(&connection, WORKSPACE, pack.id, 5)
        .unwrap()
        .unwrap();
    assert_eq!(image.assets.len(), 1);
    assert_eq!(image.assets[0].kind, "image");
    assert_eq!(image.assets[0].storage_key, "assets/figure-5.png");
}

#[test]
fn citations_survive_inactive_versions_and_deleted_sources() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    seed_active_source(&connection, VERSION_ID, "steel-reference.pdf");
    let pack = persist_evidence_pack(
        &connection,
        WORKSPACE,
        "query",
        config(),
        vec![chunk(
            "pdf",
            SourceLocation::PdfPage {
                page: 7,
                bbox: None,
            },
            1,
        )],
    )
    .unwrap();

    let replacement = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    seed_version(&connection, replacement);
    connection
        .execute(
            "UPDATE knowledge_source_documents SET active_version_id = ?1 WHERE id = ?2",
            params![replacement, DOCUMENT_ID],
        )
        .unwrap();
    let inactive = resolve_citation(&connection, WORKSPACE, pack.id, 1)
        .unwrap()
        .unwrap();
    assert_eq!(inactive.source_state, CitationSourceState::Inactive);
    assert_eq!(inactive.chunk.text, "evidence pdf");

    connection
        .execute(
            "DELETE FROM knowledge_source_documents WHERE id = ?1",
            [DOCUMENT_ID],
        )
        .unwrap();
    let deleted = resolve_citation(&connection, WORKSPACE, pack.id, 1)
        .unwrap()
        .unwrap();
    assert_eq!(deleted.source_state, CitationSourceState::Deleted);
    assert_eq!(deleted.label, "steel-reference.pdf, page 7");
}

#[test]
fn citation_resolution_is_workspace_scoped_and_rejects_zero_number() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    seed_active_source(&connection, VERSION_ID, "steel-reference.pdf");
    let pack = persist_evidence_pack(
        &connection,
        WORKSPACE,
        "query",
        config(),
        vec![chunk(
            "pdf",
            SourceLocation::PdfPage {
                page: 1,
                bbox: None,
            },
            1,
        )],
    )
    .unwrap();

    assert!(load_evidence_pack(&connection, "workspace-b", pack.id)
        .unwrap()
        .is_none());
    assert!(resolve_citation(&connection, "workspace-b", pack.id, 1)
        .unwrap()
        .is_none());
    assert_eq!(
        resolve_citation(&connection, WORKSPACE, pack.id, 0)
            .unwrap_err()
            .code(),
        "citation_number_invalid"
    );
}

fn config() -> RetrievalConfigSnapshot {
    RetrievalConfigSnapshot {
        knowledge_base_ids: vec![KnowledgeBaseId::from_str(BASE_ID).unwrap()],
        lexical_limit: 20,
        dense_limit: 30,
        candidate_limit: 10,
        rrf_k: 60,
        embedding_provider_profile_id: "33333333-3333-4333-8333-333333333333".to_string(),
        embedding_model_id: "BAAI/bge-m3".to_string(),
        rerank_provider_profile_id: Some("44444444-4444-4444-8444-444444444444".to_string()),
        rerank_model_id: Some("BAAI/bge-reranker-v2-m3".to_string()),
        rerank_degradation: Some(RerankDegradationReason::Quota),
    }
}

fn chunk(id: &str, source_location: SourceLocation, rank: usize) -> RetrievedChunk {
    RetrievedChunk {
        knowledge_base_id: KnowledgeBaseId::from_str(BASE_ID).unwrap(),
        document_id: SourceDocumentId::from_str(DOCUMENT_ID).unwrap(),
        version_id: DocumentVersionId::from_str(VERSION_ID).unwrap(),
        chunk_id: ChunkId::new(id).unwrap(),
        source_name: "steel-reference.pdf".to_string(),
        source_location,
        text: format!("evidence {id}"),
        lexical_rank: Some(rank),
        dense_rank: Some(rank + 1),
        rrf_score: 0.75 / rank as f64,
        rerank_score: Some(0.9 / rank as f32),
    }
}

fn seed_active_source(connection: &Connection, version_id: &str, source_name: &str) {
    connection
        .execute(
            "INSERT INTO knowledge_bases (id, workspace_id, name, created_at, updated_at)
             VALUES (?1, ?2, 'Steel', 'now', 'now')",
            params![BASE_ID, WORKSPACE],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_source_documents
             (id, workspace_id, knowledge_base_id, display_name, source_kind,
              active_version_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'pdf', ?5, 'now', 'now')",
            params![DOCUMENT_ID, WORKSPACE, BASE_ID, source_name, version_id],
        )
        .unwrap();
    seed_version(connection, version_id);
}

fn seed_version(connection: &Connection, version_id: &str) {
    connection
        .execute(
            "INSERT INTO knowledge_document_versions
             (id, workspace_id, document_id, content_sha256, mime_type, parser,
              parser_version, chunk_policy_version, embedding_profile_id,
              embedding_model_id, embedding_dimension, expected_asset_count,
              expected_chunk_count, created_at)
             VALUES (?1, ?2, ?3, ?4, 'application/pdf', 'test', '1', 'steel-v1',
                     'profile', 'model', 2, 0, 0, 'now')",
            params![
                version_id,
                WORKSPACE,
                DOCUMENT_ID,
                version_id.chars().next().unwrap().to_string().repeat(64)
            ],
        )
        .unwrap();
}

fn seed_asset(connection: &Connection, version_id: &str, location: &SourceLocation) {
    connection
        .execute(
            "INSERT INTO knowledge_assets
             (id, workspace_id, version_id, kind, storage_key, sha256, media_type,
              source_location_json, created_at)
             VALUES ('55555555-5555-4555-8555-555555555555', ?1, ?2, 'image',
                     'assets/figure-5.png', ?3, 'image/png', ?4, 'now')",
            params![
                WORKSPACE,
                version_id,
                "5".repeat(64),
                serde_json::to_string(location).unwrap()
            ],
        )
        .unwrap();
}
