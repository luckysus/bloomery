use bloomery::rag::index::lifecycle::build_hnsw;
use bloomery::rag::index::rebuild::{
    index_root, load_index_snapshot, queue_index_rebuild, IndexRebuildRequest,
};
use bloomery::rag::index::repair::{
    cleanup_interrupted_builds, inspect_index_health, IndexRepairReason, IndexRepairState,
    IndexServingMode,
};
use bloomery::rag::model::{
    ChunkId, NewChunk, NewDocumentVersion, NewSourceDocument, SourceLocation,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::knowledge;
use bloomery::tasks::{repository as task_repository, TaskState};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const WORKSPACE: &str = "workspace-a";
const OTHER_WORKSPACE: &str = "workspace-b";
const PROFILE: &str = "11111111-1111-4111-8111-111111111111";
const MODEL: &str = "BAAI/bge-m3";

#[test]
fn missing_sidecar_degrades_to_flat_and_low_disk_blocks_rebuild() {
    let (root, connection) = database();
    seed_vector(&connection, PROFILE, MODEL, "a", [1.0, 0.0]);
    let request = request(PROFILE, MODEL);

    let degraded =
        inspect_index_health(&connection, WORKSPACE, &root, &request, Some(u64::MAX)).unwrap();
    assert_eq!(degraded.state, IndexRepairState::DegradedFlat);
    assert_eq!(degraded.reason, Some(IndexRepairReason::MissingSidecar));
    assert_eq!(degraded.serving_mode, IndexServingMode::Flat);

    let low_disk = inspect_index_health(&connection, WORKSPACE, &root, &request, Some(0)).unwrap();
    assert_eq!(low_disk.state, IndexRepairState::Failed);
    assert_eq!(low_disk.reason, Some(IndexRepairReason::LowDisk));
    assert_eq!(low_disk.serving_mode, IndexServingMode::Flat);
    assert_eq!(low_disk.required_rebuild_bytes, 64 * 1024 * 1024 + 24);
    cleanup(root, connection);
}

#[test]
fn sqlite_watermark_divergence_requires_rebuild_but_keeps_flat_available() {
    let (root, connection) = database();
    seed_vector(&connection, PROFILE, MODEL, "a", [1.0, 0.0]);
    let request = request(PROFILE, MODEL);
    build_current(&connection, &root, &request);
    seed_vector(&connection, PROFILE, MODEL, "b", [0.0, 1.0]);

    let health =
        inspect_index_health(&connection, WORKSPACE, &root, &request, Some(u64::MAX)).unwrap();
    assert_eq!(health.state, IndexRepairState::RebuildRequired);
    assert_eq!(health.reason, Some(IndexRepairReason::WatermarkDiverged));
    assert_eq!(health.serving_mode, IndexServingMode::Flat);
    cleanup(root, connection);
}

#[test]
fn model_change_is_distinct_from_a_first_missing_sidecar() {
    let (root, connection) = database();
    seed_vector(&connection, PROFILE, MODEL, "a", [1.0, 0.0]);
    let original = request(PROFILE, MODEL);
    build_current(&connection, &root, &original);
    let changed_model = "BAAI/bge-m3-v2";
    seed_vector(&connection, PROFILE, changed_model, "b", [0.0, 1.0]);

    let health = inspect_index_health(
        &connection,
        WORKSPACE,
        &root,
        &request(PROFILE, changed_model),
        Some(u64::MAX),
    )
    .unwrap();
    assert_eq!(health.state, IndexRepairState::RebuildRequired);
    assert_eq!(health.reason, Some(IndexRepairReason::ModelChanged));
    assert_eq!(health.serving_mode, IndexServingMode::Flat);
    cleanup(root, connection);
}

#[test]
fn interrupted_temporary_build_is_cleaned_without_removing_valid_current() {
    let (root, connection) = database();
    seed_vector(&connection, PROFILE, MODEL, "a", [1.0, 0.0]);
    let request = request(PROFILE, MODEL);
    let current_root = build_current(&connection, &root, &request);
    std::fs::create_dir_all(current_root.join(".tmp-interrupted")).unwrap();
    let current_before = std::fs::read_to_string(current_root.join("CURRENT")).unwrap();

    let health =
        inspect_index_health(&connection, WORKSPACE, &root, &request, Some(u64::MAX)).unwrap();
    assert_eq!(health.state, IndexRepairState::Healthy);
    assert_eq!(health.reason, Some(IndexRepairReason::InterruptedBuild));
    assert_eq!(health.stale_temporary_count, 1);
    assert_eq!(cleanup_interrupted_builds(&current_root).unwrap(), 1);
    assert_eq!(
        std::fs::read_to_string(current_root.join("CURRENT")).unwrap(),
        current_before
    );
    assert!(!current_root.join(".tmp-interrupted").exists());
    cleanup(root, connection);
}

#[test]
fn matching_background_task_reports_rebuilding_then_failed() {
    let (root, mut connection) = database();
    seed_vector(&connection, PROFILE, MODEL, "a", [1.0, 0.0]);
    let request = request(PROFILE, MODEL);
    let task_id = queue_index_rebuild(&mut connection, WORKSPACE, request.clone()).unwrap();

    let rebuilding =
        inspect_index_health(&connection, WORKSPACE, &root, &request, Some(u64::MAX)).unwrap();
    assert_eq!(rebuilding.state, IndexRepairState::Rebuilding);
    assert_eq!(rebuilding.serving_mode, IndexServingMode::Flat);

    let running = task_repository::claim_next(&mut connection, WORKSPACE, "2099-01-01T00:00:00Z")
        .unwrap()
        .unwrap();
    task_repository::transition(
        &mut connection,
        WORKSPACE,
        task_id,
        running.attempt,
        TaskState::Running,
        TaskState::Failed,
        Some("index_rebuild_snapshot_failed"),
    )
    .unwrap();
    let failed =
        inspect_index_health(&connection, WORKSPACE, &root, &request, Some(u64::MAX)).unwrap();
    assert_eq!(failed.state, IndexRepairState::Failed);
    assert_eq!(failed.reason, Some(IndexRepairReason::RebuildFailed));
    cleanup(root, connection);
}

#[test]
fn inspection_is_workspace_scoped() {
    let (root, connection) = database();
    seed_vector_for_workspace(
        &connection,
        OTHER_WORKSPACE,
        PROFILE,
        MODEL,
        "c",
        [1.0, 0.0],
    );

    let health = inspect_index_health(
        &connection,
        OTHER_WORKSPACE,
        &root,
        &request(PROFILE, MODEL),
        Some(u64::MAX),
    )
    .unwrap();

    assert_eq!(health.chunk_count, 1);
    assert_eq!(health.serving_mode, IndexServingMode::Flat);
    assert_eq!(health.required_rebuild_bytes, 64 * 1024 * 1024 + 24);
    cleanup(root, connection);
}

fn request(profile: &str, model: &str) -> IndexRebuildRequest {
    IndexRebuildRequest {
        provider_profile_id: profile.to_string(),
        model_id: model.to_string(),
        dimension: 2,
    }
}

fn database() -> (PathBuf, Connection) {
    let root = std::env::temp_dir().join(format!("bloomery-repair-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut connection = Connection::open(root.join("bloomery.sqlite3")).unwrap();
    migrate(&mut connection).unwrap();
    (root, connection)
}

fn build_current(connection: &Connection, root: &Path, request: &IndexRebuildRequest) -> PathBuf {
    let snapshot = load_index_snapshot(connection, WORKSPACE, request).unwrap();
    let current_root = index_root(root, &snapshot.watermark);
    build_hnsw(&current_root, snapshot.watermark, &snapshot.records).unwrap();
    current_root
}

fn seed_vector(
    connection: &Connection,
    profile: &str,
    model: &str,
    suffix: &str,
    values: [f32; 2],
) {
    seed_vector_for_workspace(connection, WORKSPACE, profile, model, suffix, values);
}

fn seed_vector_for_workspace(
    connection: &Connection,
    workspace: &str,
    profile: &str,
    model: &str,
    suffix: &str,
    values: [f32; 2],
) {
    let base =
        knowledge::create_knowledge_base(connection, workspace, &format!("base-{model}-{suffix}"))
            .unwrap();
    let document = knowledge::create_source_document(
        connection,
        workspace,
        NewSourceDocument {
            knowledge_base_id: base.id,
            display_name: format!("source-{suffix}.txt"),
            source_kind: "text".to_string(),
        },
    )
    .unwrap();
    let version = knowledge::create_document_version(
        connection,
        workspace,
        NewDocumentVersion {
            document_id: document.id,
            content_sha256: suffix.repeat(64),
            mime_type: "text/plain".to_string(),
            parser: "test".to_string(),
            parser_version: "1".to_string(),
            chunk_policy_version: "steel-v1".to_string(),
            embedding_profile_id: profile.to_string(),
            embedding_model_id: model.to_string(),
            embedding_dimension: 2,
            expected_asset_count: 0,
            expected_chunk_count: 1,
        },
    )
    .unwrap();
    let chunk_id = ChunkId::new(format!("chunk-{suffix}")).unwrap();
    knowledge::add_chunk(
        connection,
        workspace,
        NewChunk {
            id: chunk_id.clone(),
            version_id: version.id,
            ordinal: 0,
            text: format!("steel {suffix}"),
            source_location: SourceLocation::TextOffsets { start: 0, end: 1 },
            content_sha256: format!("{:0>64}", suffix),
            policy_version: "steel-v1".to_string(),
        },
    )
    .unwrap();
    let blob = values
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let sha = format!("{:x}", Sha256::digest(&blob));
    connection
        .execute(
            "INSERT INTO knowledge_vectors
             (id, workspace_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_blob, vector_sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, 2, ?5, 'steel-v1', ?6, ?7, 'now')",
            params![
                format!("vector-{model}-{suffix}"),
                workspace,
                profile,
                model,
                format!("{:0>64}", suffix),
                blob,
                sha
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_chunk_embeddings
             (workspace_id, version_id, chunk_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_key, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 2, ?6, 'steel-v1', ?7, 'now')",
            params![
                workspace,
                version.id.to_string(),
                chunk_id.to_string(),
                profile,
                model,
                format!("{:0>64}", suffix),
                format!("vector-{model}-{suffix}")
            ],
        )
        .unwrap();
}

fn cleanup(root: PathBuf, connection: Connection) {
    drop(connection);
    std::fs::remove_dir_all(root).unwrap();
}
