use bloomery::rag::index::lifecycle::open_hnsw;
use bloomery::rag::index::rebuild::{
    index_root, load_index_snapshot, queue_index_rebuild, IndexRebuildHandler, IndexRebuildRequest,
    INDEX_REBUILD_KIND,
};
use bloomery::rag::index::vector::VectorIndex;
use bloomery::rag::model::{
    ChunkId, NewChunk, NewDocumentVersion, NewSourceDocument, SourceLocation,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::knowledge;
use bloomery::tasks::repository as task_repository;
use bloomery::tasks::scheduler::{
    EventSink, Scheduler, SchedulerConfig, SchedulerEvent, SystemClock,
};
use bloomery::tasks::TaskState;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WORKSPACE: &str = "workspace-a";
const PROFILE: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn command_registry_exposes_complete_local_knowledge_surface() {
    let commands = include_str!("../src/app/commands.rs");
    for command in [
        "list_knowledge_bases",
        "create_knowledge_base",
        "rename_knowledge_base",
        "preview_delete_knowledge_base",
        "delete_knowledge_base_confirmed",
        "list_knowledge_documents",
        "list_document_versions",
        "import_local_document",
        "retry_background_task",
        "cancel_background_task",
        "rebuild_knowledge_index",
        "query_local_knowledge",
        "resolve_knowledge_citation",
        "get_knowledge_health",
        "get_index_health",
    ] {
        assert!(commands.contains(command), "missing command {command}");
    }
    let adapter = include_str!("../src/app/knowledge_commands.rs");
    for sql in ["SELECT ", "INSERT ", "UPDATE ", "DELETE "] {
        assert!(!adapter.contains(sql), "knowledge adapter contains {sql}");
    }
    let bridge = include_str!("../../frontend/src/bridge/desktop.ts");
    assert!(bridge.contains("getIndexHealth"));
    assert!(bridge.contains("\"get_index_health\""));
}

#[test]
fn knowledge_catalog_supports_crud_versions_and_confirmed_delete_preview() {
    let mut connection = database();
    let base = knowledge::create_knowledge_base(&connection, WORKSPACE, "Steel").unwrap();
    let renamed =
        knowledge::rename_knowledge_base(&connection, WORKSPACE, base.id, "Steel Standards")
            .unwrap();
    assert_eq!(renamed.name, "Steel Standards");

    let document = knowledge::create_source_document(
        &connection,
        WORKSPACE,
        NewSourceDocument {
            knowledge_base_id: base.id,
            display_name: "GB-T-1591.pdf".to_string(),
            source_kind: "pdf".to_string(),
        },
    )
    .unwrap();
    let version = knowledge::create_document_version(
        &connection,
        WORKSPACE,
        NewDocumentVersion {
            document_id: document.id,
            content_sha256: "a".repeat(64),
            mime_type: "application/pdf".to_string(),
            parser: "test".to_string(),
            parser_version: "1".to_string(),
            chunk_policy_version: "steel-v1".to_string(),
            embedding_profile_id: PROFILE.to_string(),
            embedding_model_id: "BAAI/bge-m3".to_string(),
            embedding_dimension: 2,
            expected_asset_count: 0,
            expected_chunk_count: 1,
        },
    )
    .unwrap();
    knowledge::add_chunk(
        &connection,
        WORKSPACE,
        NewChunk {
            id: ChunkId::new("chunk-1").unwrap(),
            version_id: version.id,
            ordinal: 0,
            text: "Q355 yield strength".to_string(),
            source_location: SourceLocation::PdfPage {
                page: 1,
                bbox: None,
            },
            content_sha256: "b".repeat(64),
            policy_version: "steel-v1".to_string(),
        },
    )
    .unwrap();

    assert_eq!(
        knowledge::list_source_documents(&connection, WORKSPACE, base.id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        knowledge::list_document_versions(&connection, WORKSPACE, document.id)
            .unwrap()
            .len(),
        1
    );
    let impact = knowledge::preview_delete_knowledge_base(&connection, WORKSPACE, base.id).unwrap();
    assert_eq!(impact.document_count, 1);
    assert_eq!(impact.version_count, 1);
    assert_eq!(impact.chunk_count, 1);
    assert_eq!(impact.active_task_count, 0);
    assert!(
        knowledge::get_knowledge_base(&connection, WORKSPACE, base.id)
            .unwrap()
            .is_some()
    );

    knowledge::delete_knowledge_base_confirmed(&mut connection, WORKSPACE, base.id).unwrap();
    assert!(
        knowledge::get_knowledge_base(&connection, WORKSPACE, base.id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn index_rebuild_is_a_restart_safe_background_task() {
    let mut connection = database();
    let task_id = queue_index_rebuild(
        &mut connection,
        WORKSPACE,
        IndexRebuildRequest {
            provider_profile_id: PROFILE.to_string(),
            model_id: "BAAI/bge-m3".to_string(),
            dimension: 2,
        },
    )
    .unwrap();

    let task = task_repository::get(&connection, WORKSPACE, task_id)
        .unwrap()
        .unwrap();
    assert_eq!(task.kind, INDEX_REBUILD_KIND);
    assert_eq!(task.progress, 0);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&task.payload_json).unwrap(),
        serde_json::json!({
            "provider_profile_id": PROFILE,
            "model_id": "BAAI/bge-m3",
            "dimension": 2
        })
    );
}

#[test]
fn index_rebuild_handler_materializes_and_reopens_hnsw() {
    let root = std::env::temp_dir().join(format!("bloomery-command-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let database_path = root.join("bloomery.sqlite3");
    let mut connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    migrate(&mut connection).unwrap();
    seed_indexed_chunk(&mut connection);
    let task_id = queue_index_rebuild(
        &mut connection,
        WORKSPACE,
        IndexRebuildRequest {
            provider_profile_id: PROFILE.to_string(),
            model_id: "BAAI/bge-m3".to_string(),
            dimension: 2,
        },
    )
    .unwrap();
    drop(connection);

    let mut scheduler = Scheduler::new(
        database_path.clone(),
        WORKSPACE.to_string(),
        SchedulerConfig {
            max_workers: 1,
            poll_interval: Duration::from_millis(1),
            ..SchedulerConfig::default()
        },
        Arc::new(SystemClock),
        vec![Arc::new(IndexRebuildHandler::new(
            database_path.clone(),
            root.clone(),
        ))],
        Arc::new(NoopSink),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        scheduler.tick().unwrap();
        let connection = Connection::open(&database_path).unwrap();
        let state = task_repository::get(&connection, WORKSPACE, task_id)
            .unwrap()
            .unwrap()
            .state;
        if state == TaskState::Completed {
            break;
        }
        assert_ne!(state, TaskState::Failed);
        assert!(Instant::now() < deadline, "index rebuild timed out");
        std::thread::yield_now();
    }

    let connection = Connection::open(&database_path).unwrap();
    let snapshot = load_index_snapshot(
        &connection,
        WORKSPACE,
        &IndexRebuildRequest {
            provider_profile_id: PROFILE.to_string(),
            model_id: "BAAI/bge-m3".to_string(),
            dimension: 2,
        },
    )
    .unwrap();
    let index = open_hnsw(&index_root(&root, &snapshot.watermark), &snapshot.watermark).unwrap();
    assert_eq!(index.watermark().chunk_count, 1);
    drop(index);
    drop(connection);
    drop(scheduler);
    std::fs::remove_dir_all(root).unwrap();
}

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    migrate(&mut connection).unwrap();
    connection
}

fn seed_indexed_chunk(connection: &mut Connection) {
    let base = knowledge::create_knowledge_base(connection, WORKSPACE, "Steel").unwrap();
    let document = knowledge::create_source_document(
        connection,
        WORKSPACE,
        NewSourceDocument {
            knowledge_base_id: base.id,
            display_name: "vectors.txt".to_string(),
            source_kind: "text".to_string(),
        },
    )
    .unwrap();
    let version = knowledge::create_document_version(
        connection,
        WORKSPACE,
        NewDocumentVersion {
            document_id: document.id,
            content_sha256: "c".repeat(64),
            mime_type: "text/plain".to_string(),
            parser: "test".to_string(),
            parser_version: "1".to_string(),
            chunk_policy_version: "steel-v1".to_string(),
            embedding_profile_id: PROFILE.to_string(),
            embedding_model_id: "BAAI/bge-m3".to_string(),
            embedding_dimension: 2,
            expected_asset_count: 0,
            expected_chunk_count: 1,
        },
    )
    .unwrap();
    let chunk_id = ChunkId::new("chunk-vector").unwrap();
    knowledge::add_chunk(
        connection,
        WORKSPACE,
        NewChunk {
            id: chunk_id.clone(),
            version_id: version.id,
            ordinal: 0,
            text: "Q355".to_string(),
            source_location: SourceLocation::TextOffsets { start: 0, end: 4 },
            content_sha256: "d".repeat(64),
            policy_version: "steel-v1".to_string(),
        },
    )
    .unwrap();
    let vector = [1.0_f32, 0.0_f32]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let vector_sha256 = format!("{:x}", Sha256::digest(&vector));
    connection
        .execute(
            "INSERT INTO knowledge_vectors
             (id, workspace_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_blob, vector_sha256, created_at)
             VALUES ('vector-1', ?1, ?2, 'BAAI/bge-m3', 2, ?3, 'steel-v1', ?4, ?5, 'now')",
            rusqlite::params![WORKSPACE, PROFILE, "e".repeat(64), vector, vector_sha256],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO knowledge_chunk_embeddings
             (workspace_id, version_id, chunk_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_key, created_at)
             VALUES (?1, ?2, ?3, ?4, 'BAAI/bge-m3', 2, ?5, 'steel-v1', 'vector-1', 'now')",
            rusqlite::params![
                WORKSPACE,
                version.id.to_string(),
                chunk_id.to_string(),
                PROFILE,
                "e".repeat(64)
            ],
        )
        .unwrap();
}

struct NoopSink;

impl EventSink for NoopSink {
    fn emit(&self, _event: SchedulerEvent) {}
}
