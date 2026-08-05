use bloomery::rag::model::{
    ChunkId, IngestAttemptState, NewAsset, NewChunk, NewChunkEmbedding, NewDocumentVersion,
    NewSourceDocument, Rect, SourceLocation, VectorWatermark,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::knowledge;
use bloomery::tasks::{repository as task_repository, NewTask, TaskState};
use rusqlite::Connection;

const WORKSPACE: &str = "workspace-a";

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open database");
    migrate(&mut connection).expect("migrate database");
    connection
}

fn source(connection: &mut Connection) -> knowledge::SourceDocumentRecord {
    let base = knowledge::create_knowledge_base(connection, WORKSPACE, "Steel library")
        .expect("create knowledge base");
    knowledge::create_source_document(
        connection,
        WORKSPACE,
        NewSourceDocument {
            knowledge_base_id: base.id,
            display_name: "GB-T 1591.pdf".to_string(),
            source_kind: "file".to_string(),
        },
    )
    .expect("create source document")
}

fn version(
    connection: &mut Connection,
    document: &knowledge::SourceDocumentRecord,
    hash: &str,
    expected_assets: u32,
    expected_chunks: u32,
) -> knowledge::DocumentVersionRecord {
    knowledge::create_document_version(
        connection,
        WORKSPACE,
        NewDocumentVersion {
            document_id: document.id,
            content_sha256: hash.to_string(),
            mime_type: "application/pdf".to_string(),
            parser: "mineru".to_string(),
            parser_version: "v4".to_string(),
            chunk_policy_version: "steel-v1".to_string(),
            embedding_profile_id: "11111111-1111-4111-8111-111111111111".to_string(),
            embedding_model_id: "BAAI/bge-m3".to_string(),
            embedding_dimension: 1024,
            expected_asset_count: expected_assets,
            expected_chunk_count: expected_chunks,
        },
    )
    .expect("create document version")
}

fn add_complete_content(
    connection: &mut Connection,
    document: &knowledge::SourceDocumentRecord,
    version: &knowledge::DocumentVersionRecord,
) {
    knowledge::add_asset(
        connection,
        WORKSPACE,
        NewAsset {
            version_id: version.id,
            kind: "page_image".to_string(),
            storage_key: "sha256/asset-1.png".to_string(),
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            media_type: "image/png".to_string(),
            source_location: Some(SourceLocation::PdfPage {
                page: 1,
                bbox: Some(Rect {
                    x: 10.0,
                    y: 20.0,
                    width: 30.0,
                    height: 40.0,
                }),
            }),
        },
    )
    .expect("add asset");
    let chunk_id = ChunkId::new("chunk-standard-1").expect("chunk ID");
    knowledge::add_chunk(
        connection,
        WORKSPACE,
        NewChunk {
            id: chunk_id.clone(),
            version_id: version.id,
            ordinal: 0,
            text: "Q355B 屈服强度要求".to_string(),
            source_location: SourceLocation::PdfPage {
                page: 2,
                bbox: None,
            },
            content_sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_string(),
            policy_version: "steel-v1".to_string(),
        },
    )
    .expect("add chunk");
    knowledge::record_chunk_embedding(
        connection,
        WORKSPACE,
        NewChunkEmbedding {
            version_id: version.id,
            chunk_id: chunk_id.clone(),
            provider_profile_id: version.embedding_profile_id.clone(),
            model_id: version.embedding_model_id.clone(),
            dimension: version.embedding_dimension,
            normalized_text_sha256:
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
            policy_version: version.chunk_policy_version.clone(),
            vector_key: "vectors/version-1/chunk-standard-1".to_string(),
        },
    )
    .expect("record embedding");
    knowledge::index_chunk_fts(connection, WORKSPACE, version.id, &chunk_id)
        .expect("index chunk FTS");
    knowledge::set_vector_watermark(
        connection,
        WORKSPACE,
        VectorWatermark {
            version_id: version.id,
            provider_profile_id: version.embedding_profile_id.clone(),
            model_id: version.embedding_model_id.clone(),
            dimension: version.embedding_dimension,
            expected_count: 1,
            indexed_count: 1,
            index_version: "hnsw-v1".to_string(),
        },
    )
    .expect("set vector watermark");

    assert_ne!(
        knowledge::get_source_document(connection, WORKSPACE, document.id)
            .expect("get source")
            .expect("source exists")
            .active_version_id,
        Some(version.id)
    );
}

#[test]
fn knowledge_records_are_typed_and_workspace_scoped() {
    let mut connection = database();
    let document = source(&mut connection);
    let version = version(
        &mut connection,
        &document,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        1,
        1,
    );
    let attempt = knowledge::create_ingest_attempt(
        &mut connection,
        WORKSPACE,
        document.id,
        Some(version.id),
        Some("22222222-2222-4222-8222-222222222222".to_string()),
    )
    .expect("create attempt");

    assert_eq!(attempt.state, IngestAttemptState::Running);
    assert_eq!(
        knowledge::list_knowledge_bases(&connection, WORKSPACE)
            .expect("list bases")
            .len(),
        1
    );
    assert!(knowledge::list_knowledge_bases(&connection, "workspace-b")
        .expect("list other workspace")
        .is_empty());
    assert!(
        knowledge::get_source_document(&connection, "workspace-b", document.id)
            .expect("read other workspace")
            .is_none()
    );

    let location = SourceLocation::SheetRange {
        sheet: "炉次数据".to_string(),
        range: "A2:G25".to_string(),
    };
    assert_eq!(
        serde_json::from_str::<SourceLocation>(
            &serde_json::to_string(&location).expect("serialize location")
        )
        .expect("deserialize location"),
        location
    );
}

#[test]
fn ingest_attempts_finish_once_with_consistent_error_state() {
    let mut connection = database();
    let document = source(&mut connection);
    let created =
        knowledge::create_ingest_attempt(&mut connection, WORKSPACE, document.id, None, None)
            .expect("create attempt");

    assert!(knowledge::finish_ingest_attempt(
        &mut connection,
        WORKSPACE,
        created.id,
        IngestAttemptState::Failed,
        None,
    )
    .unwrap_err()
    .contains("attempt_error_required"));
    let finished = knowledge::finish_ingest_attempt(
        &mut connection,
        WORKSPACE,
        created.id,
        IngestAttemptState::Completed,
        None,
    )
    .expect("finish attempt");
    assert_eq!(finished.state, IngestAttemptState::Completed);
    assert!(finished.finished_at.is_some());
    assert!(knowledge::finish_ingest_attempt(
        &mut connection,
        WORKSPACE,
        created.id,
        IngestAttemptState::Cancelled,
        None,
    )
    .unwrap_err()
    .contains("attempt_already_finished"));
    assert!(
        knowledge::get_ingest_attempt(&connection, "workspace-b", created.id)
            .expect("read other workspace")
            .is_none()
    );
}

#[test]
fn document_versions_are_immutable_and_content_deduplicated() {
    let mut connection = database();
    let document = source(&mut connection);
    let hash = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let created = version(&mut connection, &document, hash, 0, 0);

    let duplicate = knowledge::create_document_version(
        &mut connection,
        WORKSPACE,
        NewDocumentVersion {
            document_id: document.id,
            content_sha256: hash.to_string(),
            mime_type: "application/pdf".to_string(),
            parser: "local".to_string(),
            parser_version: "1".to_string(),
            chunk_policy_version: "steel-v1".to_string(),
            embedding_profile_id: created.embedding_profile_id.clone(),
            embedding_model_id: created.embedding_model_id.clone(),
            embedding_dimension: created.embedding_dimension,
            expected_asset_count: 0,
            expected_chunk_count: 0,
        },
    )
    .expect_err("duplicate content version must fail");
    assert!(duplicate.contains("duplicate_document_version"));

    let immutable = connection
        .execute(
            "UPDATE knowledge_document_versions SET parser = 'changed' WHERE id = ?1",
            [created.id.to_string()],
        )
        .expect_err("version metadata must be immutable");
    assert!(immutable.to_string().contains("immutable_document_version"));
}

#[test]
fn pending_document_manifest_can_be_sealed_once_after_parsing() {
    let mut connection = database();
    let document = source(&mut connection);
    let pending = knowledge::create_pending_document_version(
        &mut connection,
        WORKSPACE,
        NewDocumentVersion {
            document_id: document.id,
            content_sha256: "dededededededededededededededededededededededededededededededede"
                .to_string(),
            mime_type: "application/pdf".to_string(),
            parser: "mineru".to_string(),
            parser_version: "v4".to_string(),
            chunk_policy_version: "steel-v1".to_string(),
            embedding_profile_id: "11111111-1111-4111-8111-111111111111".to_string(),
            embedding_model_id: "BAAI/bge-m3".to_string(),
            embedding_dimension: 1024,
            expected_asset_count: 0,
            expected_chunk_count: 0,
        },
    )
    .expect("create pending version");

    assert!(!pending.manifest_sealed);
    assert!(knowledge::activate_document_version(
        &mut connection,
        WORKSPACE,
        document.id,
        pending.id,
    )
    .unwrap_err()
    .contains("incomplete_document_version"));

    knowledge::seal_document_manifest(&mut connection, WORKSPACE, pending.id, 0, 2)
        .expect("seal parsed manifest");
    knowledge::seal_document_manifest(&mut connection, WORKSPACE, pending.id, 0, 2)
        .expect("repeat matching seal");
    let sealed = knowledge::get_document_version(&connection, WORKSPACE, pending.id)
        .unwrap()
        .unwrap();
    assert!(sealed.manifest_sealed);
    assert_eq!(sealed.expected_asset_count, 0);
    assert_eq!(sealed.expected_chunk_count, 2);

    assert!(
        knowledge::seal_document_manifest(&mut connection, WORKSPACE, pending.id, 1, 2)
            .unwrap_err()
            .contains("document_manifest_mismatch")
    );
    let immutable = connection
        .execute(
            "UPDATE knowledge_document_versions SET expected_chunk_count = 3 WHERE id = ?1",
            [pending.id.to_string()],
        )
        .expect_err("sealed manifest must be immutable");
    assert!(immutable.to_string().contains("immutable_document_version"));
}

#[test]
fn activation_requires_every_persisted_index_component() {
    let mut connection = database();
    let document = source(&mut connection);
    let version = version(
        &mut connection,
        &document,
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        1,
        1,
    );

    assert!(knowledge::activate_document_version(
        &mut connection,
        WORKSPACE,
        document.id,
        version.id
    )
    .unwrap_err()
    .contains("incomplete_document_version"));
    add_complete_content(&mut connection, &document, &version);

    let activated =
        knowledge::activate_document_version(&mut connection, WORKSPACE, document.id, version.id)
            .expect("activate complete version");

    assert_eq!(activated.active_version_id, Some(version.id));
}

#[test]
fn task_guarded_activation_is_atomic_with_cancellation_and_completion() {
    let mut connection = database();
    let document = source(&mut connection);
    let version = version(
        &mut connection,
        &document,
        "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
        1,
        1,
    );
    add_complete_content(&mut connection, &document, &version);
    let cancelled_id = task_repository::create(
        &mut connection,
        NewTask {
            workspace_id: WORKSPACE.to_string(),
            kind: "mineru_parse".to_string(),
            payload_json: "{}".to_string(),
            checkpoint_json: None,
            next_run_at: None,
            progress: 95,
        },
    )
    .unwrap()
    .id;
    let cancelled = task_repository::claim_next(&mut connection, WORKSPACE, "2099-01-01T00:00:00Z")
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.id, cancelled_id);

    assert!(knowledge::activate_document_version_for_task(
        &mut connection,
        WORKSPACE,
        document.id,
        version.id,
        cancelled.id,
        cancelled.attempt + 1,
        r#"{"stage":"activated"}"#,
    )
    .unwrap_err()
    .contains("task_finalization_blocked"));
    task_repository::request_running_cancellation(
        &mut connection,
        WORKSPACE,
        cancelled.id,
        cancelled.attempt,
    )
    .unwrap();

    assert!(knowledge::activate_document_version_for_task(
        &mut connection,
        WORKSPACE,
        document.id,
        version.id,
        cancelled.id,
        cancelled.attempt,
        r#"{"stage":"activated"}"#,
    )
    .unwrap_err()
    .contains("task_finalization_blocked"));
    assert_eq!(
        knowledge::get_source_document(&connection, WORKSPACE, document.id)
            .unwrap()
            .unwrap()
            .active_version_id,
        None
    );

    task_repository::transition(
        &mut connection,
        WORKSPACE,
        cancelled.id,
        cancelled.attempt,
        TaskState::Running,
        TaskState::Cancelled,
        None,
    )
    .unwrap();
    assert!(knowledge::activate_document_version_for_task(
        &mut connection,
        WORKSPACE,
        document.id,
        version.id,
        cancelled.id,
        cancelled.attempt,
        r#"{"stage":"activated"}"#,
    )
    .unwrap_err()
    .contains("task_finalization_blocked"));
    assert_eq!(
        knowledge::get_source_document(&connection, WORKSPACE, document.id)
            .unwrap()
            .unwrap()
            .active_version_id,
        None
    );
    assert!(
        knowledge::get_document_version(&connection, WORKSPACE, version.id)
            .unwrap()
            .unwrap()
            .activated_at
            .is_none()
    );

    let completing_id = task_repository::create(
        &mut connection,
        NewTask {
            workspace_id: WORKSPACE.to_string(),
            kind: "mineru_parse".to_string(),
            payload_json: "{}".to_string(),
            checkpoint_json: None,
            next_run_at: None,
            progress: 95,
        },
    )
    .unwrap()
    .id;
    let completing =
        task_repository::claim_next(&mut connection, WORKSPACE, "2099-01-01T00:00:00Z")
            .unwrap()
            .unwrap();
    assert_eq!(completing.id, completing_id);

    knowledge::activate_document_version_for_task(
        &mut connection,
        WORKSPACE,
        document.id,
        version.id,
        completing.id,
        completing.attempt,
        r#"{"stage":"activated"}"#,
    )
    .expect("atomically activate and complete task");

    let completed = task_repository::get(&connection, WORKSPACE, completing.id)
        .unwrap()
        .unwrap();
    assert_eq!(completed.state, TaskState::Completed);
    assert_eq!(completed.progress, 100);
    assert_eq!(
        completed.checkpoint_json.as_deref(),
        Some(r#"{"stage":"activated"}"#)
    );
    assert_eq!(
        knowledge::get_source_document(&connection, WORKSPACE, document.id)
            .unwrap()
            .unwrap()
            .active_version_id,
        Some(version.id)
    );
    assert!(task_repository::request_running_cancellation(
        &mut connection,
        WORKSPACE,
        completing.id,
        completing.attempt,
    )
    .is_err());
}

#[test]
fn task_completion_failure_rolls_back_document_activation_and_task_state() {
    let mut connection = database();
    let document = source(&mut connection);
    let version = version(
        &mut connection,
        &document,
        "5656565656565656565656565656565656565656565656565656565656565656",
        1,
        1,
    );
    add_complete_content(&mut connection, &document, &version);
    let task_id = task_repository::create(
        &mut connection,
        NewTask {
            workspace_id: WORKSPACE.to_string(),
            kind: "mineru_parse".to_string(),
            payload_json: "{}".to_string(),
            checkpoint_json: None,
            next_run_at: None,
            progress: 95,
        },
    )
    .unwrap()
    .id;
    let task = task_repository::claim_next(&mut connection, WORKSPACE, "2099-01-01T00:00:00Z")
        .unwrap()
        .unwrap();
    assert_eq!(task.id, task_id);
    connection
        .execute_batch(&format!(
            "CREATE TEMP TRIGGER abort_task_completion
             BEFORE UPDATE OF state ON main.background_tasks
             WHEN NEW.id = '{}' AND NEW.state = 'completed'
             BEGIN SELECT RAISE(ABORT, 'task completion failed'); END;",
            task.id
        ))
        .expect("create task completion failure trigger");

    assert!(knowledge::activate_document_version_for_task(
        &mut connection,
        WORKSPACE,
        document.id,
        version.id,
        task.id,
        task.attempt,
        r#"{"stage":"activated"}"#,
    )
    .unwrap_err()
    .contains("task completion failed"));

    assert_eq!(
        knowledge::get_source_document(&connection, WORKSPACE, document.id)
            .unwrap()
            .unwrap()
            .active_version_id,
        None
    );
    assert!(
        knowledge::get_document_version(&connection, WORKSPACE, version.id)
            .unwrap()
            .unwrap()
            .activated_at
            .is_none()
    );
    let task = task_repository::get(&connection, WORKSPACE, task.id)
        .unwrap()
        .unwrap();
    assert_eq!(task.state, TaskState::Running);
    assert_eq!(task.progress, 95);
    assert_eq!(task.checkpoint_json, None);
}

#[test]
fn activation_rejects_mismatched_vector_watermark() {
    let mut connection = database();
    let document = source(&mut connection);
    let version = version(
        &mut connection,
        &document,
        "abababababababababababababababababababababababababababababababab",
        1,
        1,
    );
    add_complete_content(&mut connection, &document, &version);
    connection
        .execute(
            "UPDATE knowledge_vector_watermarks SET indexed_count = 0 WHERE version_id = ?1",
            [version.id.to_string()],
        )
        .expect("corrupt watermark");

    assert!(knowledge::activate_document_version(
        &mut connection,
        WORKSPACE,
        document.id,
        version.id
    )
    .unwrap_err()
    .contains("incomplete_document_version"));
    assert_eq!(
        knowledge::get_source_document(&connection, WORKSPACE, document.id)
            .expect("get source")
            .expect("source exists")
            .active_version_id,
        None
    );
}

#[test]
fn active_version_switch_rolls_back_as_one_transaction() {
    let mut connection = database();
    let document = source(&mut connection);
    let first = version(
        &mut connection,
        &document,
        "1212121212121212121212121212121212121212121212121212121212121212",
        1,
        1,
    );
    add_complete_content(&mut connection, &document, &first);
    knowledge::activate_document_version(&mut connection, WORKSPACE, document.id, first.id)
        .expect("activate first version");
    let second = version(
        &mut connection,
        &document,
        "3434343434343434343434343434343434343434343434343434343434343434",
        1,
        1,
    );
    add_complete_content(&mut connection, &document, &second);
    connection
        .execute_batch(&format!(
            "CREATE TRIGGER abort_second_activation
             BEFORE UPDATE OF activated_at ON knowledge_document_versions
             WHEN NEW.id = '{}'
             BEGIN SELECT RAISE(ABORT, 'activation write failed'); END;",
            second.id
        ))
        .expect("create failure trigger");

    knowledge::activate_document_version(&mut connection, WORKSPACE, document.id, second.id)
        .expect_err("second activation must roll back");

    assert_eq!(
        knowledge::get_source_document(&connection, WORKSPACE, document.id)
            .expect("get source")
            .expect("source exists")
            .active_version_id,
        Some(first.id)
    );
}
