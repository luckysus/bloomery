use bloomery::rag::citation::{persist_evidence_pack, resolve_citation, RetrievalConfigSnapshot};
use bloomery::rag::index::lifecycle::{build_hnsw, open_hnsw};
use bloomery::rag::index::rebuild::{index_root, load_index_snapshot, IndexRebuildRequest};
use bloomery::rag::ingest::SourceFormat;
use bloomery::rag::model::KnowledgeBaseId;
use bloomery::rag::parse::{parse_document, DocumentBlock, ParseLimits};
use bloomery::rag::retrieve::{retrieve, HybridSearchRequest, RetrievedChunk};
use bloomery::storage::migrations::migrate;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::fs;
use std::str::FromStr;

const WORKSPACE: &str = "workspace-a";
const BASE_ID: &str = "11111111-1111-4111-8111-111111111111";
const DOCUMENT_ID: &str = "22222222-2222-4222-8222-222222222222";
const VERSION_ID: &str = "33333333-3333-4333-8333-333333333333";
const PROFILE_ID: &str = "44444444-4444-4444-8444-444444444444";
const MODEL_ID: &str = "BAAI/bge-m3";

#[test]
fn pdf_citation_and_hnsw_survive_restart() {
    let root = std::env::temp_dir().join(format!("bloomery-gate-c-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let pdf = root.join("standard.pdf");
    fs::write(
        &pdf,
        br#"%PDF-1.4
1 0 obj << /Type /Page /Contents 2 0 R >> endobj
2 0 obj << /Length 72 >> stream
BT /F1 12 Tf 72 720 Td (Q355B yield strength 355 MPa) Tj ET
endstream endobj
%%EOF"#,
    )
    .unwrap();
    let parsed = parse_document(&pdf, SourceFormat::Pdf, ParseLimits::default()).unwrap();
    let (text, location) = match parsed.blocks.into_iter().next().unwrap() {
        DocumentBlock::Paragraph { text, location } => (text, location),
        block => panic!("unexpected PDF block: {block:?}"),
    };

    let database = root.join("bloomery.sqlite3");
    let mut connection = Connection::open(&database).unwrap();
    migrate(&mut connection).unwrap();
    seed_pdf_chunk(&connection, &text, &location);
    let request = IndexRebuildRequest {
        provider_profile_id: PROFILE_ID.to_string(),
        model_id: MODEL_ID.to_string(),
        dimension: 2,
    };
    let snapshot = load_index_snapshot(&connection, WORKSPACE, &request).unwrap();
    let index_path = index_root(&root, &snapshot.watermark);
    build_hnsw(&index_path, snapshot.watermark.clone(), &snapshot.records).unwrap();
    let index = open_hnsw(&index_path, &snapshot.watermark).unwrap();
    let first = search(&connection, &index);
    let pack = persist_evidence_pack(
        &connection,
        WORKSPACE,
        "Q355B yield strength",
        config(),
        first,
    )
    .unwrap();
    let citation = resolve_citation(&connection, WORKSPACE, pack.id, 1)
        .unwrap()
        .unwrap();
    assert_eq!(citation.label, "standard.pdf, page 1");
    drop(index);
    drop(connection);

    connection = Connection::open(&database).unwrap();
    let reopened_snapshot = load_index_snapshot(&connection, WORKSPACE, &request).unwrap();
    let reopened = open_hnsw(&index_path, &reopened_snapshot.watermark).unwrap();
    assert_eq!(
        search(&connection, &reopened)[0].chunk_id.as_str(),
        "pdf-page-1"
    );
    let persisted = resolve_citation(&connection, WORKSPACE, pack.id, 1)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.label, "standard.pdf, page 1");
    drop(reopened);
    drop(connection);
    fs::remove_dir_all(root).unwrap();
}

fn search(
    connection: &Connection,
    index: &dyn bloomery::rag::index::vector::VectorIndex,
) -> Vec<RetrievedChunk> {
    let hits = retrieve(
        connection,
        index,
        &HybridSearchRequest {
            workspace_id: WORKSPACE.to_string(),
            query: "Q355B yield strength".to_string(),
            query_vector: vec![1.0, 0.0],
            knowledge_base_ids: vec![KnowledgeBaseId::from_str(BASE_ID).unwrap()],
            lexical_limit: 10,
            dense_limit: 10,
            candidate_limit: 10,
            rrf_k: 60,
        },
    )
    .unwrap();
    assert_eq!(hits.len(), 1);
    hits
}

fn config() -> RetrievalConfigSnapshot {
    RetrievalConfigSnapshot {
        knowledge_base_ids: vec![KnowledgeBaseId::from_str(BASE_ID).unwrap()],
        lexical_limit: 10,
        dense_limit: 10,
        candidate_limit: 10,
        rrf_k: 60,
        embedding_provider_profile_id: PROFILE_ID.to_string(),
        embedding_model_id: MODEL_ID.to_string(),
        rerank_provider_profile_id: None,
        rerank_model_id: None,
        rerank_degradation: None,
    }
}

fn seed_pdf_chunk(
    connection: &Connection,
    text: &str,
    location: &bloomery::rag::model::SourceLocation,
) {
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
             VALUES (?1, ?2, ?3, 'standard.pdf', 'pdf', ?4, 'now', 'now')",
            params![DOCUMENT_ID, WORKSPACE, BASE_ID, VERSION_ID],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_document_versions
             (id, workspace_id, document_id, content_sha256, mime_type, parser,
              parser_version, chunk_policy_version, embedding_profile_id,
              embedding_model_id, embedding_dimension, expected_asset_count,
              expected_chunk_count, created_at, activated_at)
             VALUES (?1, ?2, ?3, ?4, 'application/pdf', 'local_pdf', '1', 'steel-v1',
                     ?5, ?6, 2, 0, 1, 'now', 'now')",
            params![
                VERSION_ID,
                WORKSPACE,
                DOCUMENT_ID,
                digest(b"pdf"),
                PROFILE_ID,
                MODEL_ID
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_chunks
             (id, workspace_id, version_id, ordinal, text, source_location_json,
              content_sha256, policy_version, created_at)
             VALUES ('pdf-page-1', ?1, ?2, 0, ?3, ?4, ?5, 'steel-v1', 'now')",
            params![
                WORKSPACE,
                VERSION_ID,
                text,
                serde_json::to_string(location).unwrap(),
                digest(text.as_bytes())
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_chunks_fts
             (workspace_id, knowledge_base_id, document_id, version_id, chunk_id,
              title_path, source_name, grade_aliases, text)
             VALUES (?1, ?2, ?3, ?4, 'pdf-page-1', '', 'standard.pdf', 'q355b', ?5)",
            params![WORKSPACE, BASE_ID, DOCUMENT_ID, VERSION_ID, text],
        )
        .unwrap();
    let blob = [1.0_f32, 0.0_f32]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let vector_sha256 = digest(&blob);
    connection
        .execute(
            "INSERT INTO knowledge_vectors
             (id, workspace_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_blob, vector_sha256, created_at)
             VALUES ('vector-pdf', ?1, ?2, ?3, 2, ?4, 'steel-v1', ?5, ?6, 'now')",
            params![
                WORKSPACE,
                PROFILE_ID,
                MODEL_ID,
                digest(text.as_bytes()),
                blob,
                vector_sha256
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_chunk_embeddings
             (workspace_id, version_id, chunk_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_key, created_at)
             VALUES (?1, ?2, 'pdf-page-1', ?3, ?4, 2, ?5, 'steel-v1', 'vector-pdf', 'now')",
            params![
                WORKSPACE,
                VERSION_ID,
                PROFILE_ID,
                MODEL_ID,
                digest(text.as_bytes())
            ],
        )
        .unwrap();
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
