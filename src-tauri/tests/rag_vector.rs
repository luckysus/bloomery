use bloomery::rag::index::lifecycle::{
    build_hnsw, open_hnsw, open_with_flat_fallback, VectorRecord,
};
use bloomery::rag::index::vector::{CandidateFilter, IndexWatermark, VectorIndex};
use bloomery::rag::model::{ChunkId, DocumentVersionId};
use bloomery::storage::migrations::migrate;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use uuid::Uuid;

const WORKSPACE: &str = "workspace-a";
const PROFILE: &str = "11111111-1111-4111-8111-111111111111";
const MODEL: &str = "BAAI/bge-m3";

#[test]
fn hnsw_build_reopen_filter_insert_and_old_generation_retention() {
    let root = temp_root();
    fs::create_dir_all(root.join(".tmp-stale")).unwrap();
    let version_a = version("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    let version_b = version("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
    let first_records = vec![
        record(version_a, "a", vec![1.0, 0.0, 0.0]),
        record(version_b, "b", vec![0.0, 1.0, 0.0]),
    ];
    let first_watermark = watermark(first_records.len());

    let first = build_hnsw(&root, first_watermark.clone(), &first_records).unwrap();
    assert!(!root.join(".tmp-stale").exists());
    let reopened = open_hnsw(&root, &first_watermark).unwrap();
    let hits = reopened
        .search(&[0.9, 0.1, 0.0], 5, &CandidateFilter::new(vec![version_a]))
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].chunk_id.as_str(), "a");

    let mut second_records = first_records;
    second_records.push(record(version_a, "inserted", vec![0.9, 0.1, 0.0]));
    let second_watermark = watermark(second_records.len());
    let second = build_hnsw(&root, second_watermark.clone(), &second_records).unwrap();
    assert_ne!(first.generation_id, second.generation_id);
    assert!(first.generation_path.is_dir());
    assert!(second.generation_path.is_dir());
    let reopened = open_hnsw(&root, &second_watermark).unwrap();
    assert_eq!(
        reopened
            .search(&[0.9, 0.1, 0.0], 1, &CandidateFilter::new(vec![version_a]),)
            .unwrap()[0]
            .chunk_id
            .as_str(),
        "inserted"
    );
    cleanup(root);
}

#[test]
fn hnsw_rejects_checksum_truncation_and_identity_drift_without_replacing_current() {
    let root = temp_root();
    let records = vec![record(
        version("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
        "a",
        vec![1.0, 0.0, 0.0],
    )];
    let expected = watermark(records.len());
    let built = build_hnsw(&root, expected.clone(), &records).unwrap();
    let current_before = fs::read_to_string(root.join("CURRENT")).unwrap();

    let mut wrong_model = expected.clone();
    wrong_model.model_id = "different-model".to_string();
    assert_eq!(
        open_hnsw(&root, &wrong_model).unwrap_err().code(),
        "index_identity_mismatch"
    );

    let mut invalid = records.clone();
    invalid[0].vector[0] = f32::NAN;
    assert_eq!(
        build_hnsw(&root, expected.clone(), &invalid)
            .unwrap_err()
            .code(),
        "index_vector_invalid"
    );
    assert_eq!(
        fs::read_to_string(root.join("CURRENT")).unwrap(),
        current_before
    );

    let graph = built.generation_path.join("index.hnsw.graph");
    let bytes = fs::read(&graph).unwrap();
    fs::write(&graph, &bytes[..bytes.len() / 2]).unwrap();
    assert_eq!(
        open_hnsw(&root, &expected).unwrap_err().code(),
        "index_checksum_mismatch"
    );
    cleanup(root);
}

#[test]
fn missing_or_corrupt_hnsw_degrades_to_authoritative_sqlite_flat_search() {
    let root = temp_root();
    let version_a = version("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    let version_b = version("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
    let records = vec![
        record(version_a, "near", vec![1.0, 0.0, 0.0]),
        record(version_b, "filtered", vec![0.99, 0.01, 0.0]),
    ];
    let expected = watermark(records.len());
    let connection = vector_database(&records);

    let fallback = open_with_flat_fallback(&connection, &root, &expected).unwrap();
    assert!(fallback.is_flat());
    let hits = fallback
        .search(&[1.0, 0.0, 0.0], 10, &CandidateFilter::new(vec![version_a]))
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].chunk_id.as_str(), "near");

    let built = build_hnsw(&root, expected.clone(), &records).unwrap();
    fs::write(built.generation_path.join("index.hnsw.data"), b"truncated").unwrap();
    let fallback = open_with_flat_fallback(&connection, &root, &expected).unwrap();
    assert!(fallback.is_flat());
    assert_eq!(
        fallback
            .search(&[1.0, 0.0, 0.0], 1, &CandidateFilter::new(vec![version_a]),)
            .unwrap()[0]
            .chunk_id
            .as_str(),
        "near"
    );
    cleanup(root);
}

fn watermark(chunk_count: usize) -> IndexWatermark {
    IndexWatermark {
        format_version: 1,
        workspace_id: WORKSPACE.to_string(),
        provider_profile_id: PROFILE.to_string(),
        model_id: MODEL.to_string(),
        dimension: 3,
        chunk_count: chunk_count as u32,
        sqlite_watermark: format!("sqlite-{chunk_count}"),
    }
}

fn version(value: &str) -> DocumentVersionId {
    DocumentVersionId::from_str(value).unwrap()
}

fn record(version_id: DocumentVersionId, chunk_id: &str, vector: Vec<f32>) -> VectorRecord {
    VectorRecord {
        version_id,
        chunk_id: ChunkId::new(chunk_id).unwrap(),
        vector,
    }
}

fn vector_database(records: &[VectorRecord]) -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_bases
             (id, workspace_id, name, created_at, updated_at)
             VALUES ('base', ?1, 'Steel', 'now', 'now')",
            [WORKSPACE],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_source_documents
             (id, workspace_id, knowledge_base_id, display_name, source_kind, created_at, updated_at)
             VALUES ('document', ?1, 'base', 'Vectors', 'text', 'now', 'now')",
            [WORKSPACE],
        )
        .unwrap();
    for (ordinal, record) in records.iter().enumerate() {
        let key = format!("vector-{ordinal}");
        connection
            .execute(
                "INSERT OR IGNORE INTO knowledge_document_versions
                 (id, workspace_id, document_id, content_sha256, mime_type, parser,
                  parser_version, chunk_policy_version, embedding_profile_id,
                  embedding_model_id, embedding_dimension, expected_asset_count,
                  expected_chunk_count, created_at)
                 VALUES (?1, ?2, 'document', ?3, 'text/plain', 'test', '1', 'steel-v1',
                         ?4, ?5, 3, 0, ?6, 'now')",
                params![
                    record.version_id.to_string(),
                    WORKSPACE,
                    format!("content-{}", record.version_id),
                    PROFILE,
                    MODEL,
                    records.len() as u32
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO knowledge_chunks
                 (id, workspace_id, version_id, ordinal, text, source_location_json,
                  content_sha256, policy_version, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?1, '{}', ?5, 'steel-v1', 'now')",
                params![
                    record.chunk_id.to_string(),
                    WORKSPACE,
                    record.version_id.to_string(),
                    ordinal as u32,
                    format!("chunk-{ordinal}")
                ],
            )
            .unwrap();
        let blob = record
            .vector
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let vector_sha256 = format!("{:x}", Sha256::digest(&blob));
        connection
            .execute(
                "INSERT INTO knowledge_vectors
                 (id, workspace_id, provider_profile_id, model_id, dimension,
                  normalized_text_sha256, policy_version, vector_blob, vector_sha256, created_at)
                 VALUES (?1, ?2, ?3, ?4, 3, ?5, 'steel-v1', ?6, ?7, 'now')",
                params![
                    key,
                    WORKSPACE,
                    PROFILE,
                    MODEL,
                    format!("{:064x}", ordinal + 1),
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
                 VALUES (?1, ?2, ?3, ?4, ?5, 3, ?6, 'steel-v1', ?7, 'now')",
                params![
                    WORKSPACE,
                    record.version_id.to_string(),
                    record.chunk_id.to_string(),
                    PROFILE,
                    MODEL,
                    format!("{:064x}", ordinal + 1),
                    key
                ],
            )
            .unwrap();
    }
    connection
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("bloomery-vector-{}", Uuid::new_v4()))
}

fn cleanup(path: impl AsRef<Path>) {
    let _ = fs::remove_dir_all(path);
}
