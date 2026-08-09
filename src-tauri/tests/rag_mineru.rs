use bloomery::providers::capabilities::{
    DocumentParseRequest, DocumentTaskState, DocumentTaskStatus, ParsedDocumentArtifact,
    RemoteTaskId,
};
use bloomery::providers::http::{ProviderError, ProviderErrorCode};
use bloomery::rag::model::{DocumentVersionId, SourceDocumentId};
use bloomery::rag::parse::{parse_mineru_artifact, DocumentBlock, ParseLimits};
use bloomery::rag::tasks::{
    decode_mineru_checkpoint, MinerUCheckpoint, MinerUPostprocessor, MinerUProcessFuture,
    MinerURemote, MinerURemoteFactory, MinerURemoteFuture, MinerUStage, MinerUTaskHandler,
    MinerUTaskPayload, MinerUUploadTicket, StoredObjectRef, TaskFinalization, MINERU_TASK_KIND,
};
use bloomery::storage::migrations::migrate;
use bloomery::tasks::repository;
use bloomery::tasks::scheduler::{
    Clock, EventSink, HandlerError, Scheduler, SchedulerConfig, SchedulerEvent, SystemClock,
    TaskHandler,
};
use bloomery::tasks::{NewTask, TaskState};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;
use zip::write::SimpleFileOptions;

const SOURCE_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ARTIFACT_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const AST_HASH: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn object(hash: &str) -> StoredObjectRef {
    StoredObjectRef::new(hash, format!("objects/sha256/{}/{}", &hash[..2], hash))
        .expect("stored object reference")
}

#[test]
fn mineru_checkpoint_sequence_is_strict_restart_safe_and_hash_preserving() {
    let source = object(SOURCE_HASH);
    let mut checkpoint = MinerUCheckpoint::source_stored(source.clone());
    assert_eq!(checkpoint.stage(), MinerUStage::SourceStored);
    assert_eq!(checkpoint.progress(), 5);
    assert!(checkpoint.clone().mark_parsed(object(AST_HASH)).is_err());
    assert!(checkpoint.clone().mark_batch_created("batch-123").is_err());

    checkpoint = checkpoint
        .mark_submitting(SOURCE_HASH)
        .expect("mark submitting");
    assert_eq!(checkpoint.stage(), MinerUStage::Submitting);
    assert_eq!(checkpoint.submit_request_sha256(), Some(SOURCE_HASH));
    checkpoint = checkpoint
        .mark_batch_created("batch-123")
        .expect("mark batch created");
    assert_eq!(checkpoint.stage(), MinerUStage::BatchCreated);
    checkpoint = checkpoint.mark_submitted().expect("mark submitted");
    checkpoint = decode_mineru_checkpoint(
        &serde_json::to_string(&checkpoint).expect("serialize submitted checkpoint"),
    )
    .expect("restore submitted checkpoint");
    assert_eq!(checkpoint.remote_task_id(), Some("batch-123"));

    checkpoint = checkpoint.mark_polling().expect("mark polling");
    checkpoint = checkpoint
        .mark_artifact_downloaded(object(ARTIFACT_HASH))
        .expect("mark artifact downloaded");
    checkpoint = checkpoint
        .mark_parsed(object(AST_HASH))
        .expect("mark parsed");
    checkpoint = checkpoint
        .mark_chunked("dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd")
        .expect("mark chunked");
    checkpoint = checkpoint
        .mark_embedded("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
        .expect("mark embedded");
    checkpoint = checkpoint
        .mark_indexed("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
        .expect("mark indexed");
    checkpoint = checkpoint
        .mark_activated(DocumentVersionId::new())
        .expect("mark activated");

    assert_eq!(checkpoint.stage(), MinerUStage::Activated);
    assert_eq!(checkpoint.progress(), 100);
    assert_eq!(checkpoint.source(), &source);
    assert_eq!(checkpoint.remote_task_id(), Some("batch-123"));
    assert_eq!(checkpoint.artifact().unwrap().sha256(), ARTIFACT_HASH);
    assert_eq!(checkpoint.parsed_ast().unwrap().sha256(), AST_HASH);
    decode_mineru_checkpoint(&serde_json::to_string(&checkpoint).unwrap())
        .expect("restore activated checkpoint");
}

#[test]
fn mineru_checkpoint_rejects_corrupt_or_incomplete_persisted_state() {
    let incomplete = serde_json::json!({
        "stage": "artifact_downloaded",
        "source": {
            "sha256": SOURCE_HASH,
            "storage_key": format!("objects/sha256/aa/{SOURCE_HASH}")
        },
        "remote_task_id": "batch-123",
        "artifact": null,
        "parsed_ast": null,
        "chunk_manifest_sha256": null,
        "embedding_manifest_sha256": null,
        "index_manifest_sha256": null,
        "activated_version_id": null
    });
    assert_eq!(
        decode_mineru_checkpoint(&incomplete.to_string())
            .unwrap_err()
            .code(),
        "invalid_mineru_checkpoint"
    );

    let mut checkpoint = MinerUCheckpoint::source_stored(object(SOURCE_HASH))
        .mark_submitting(SOURCE_HASH)
        .unwrap()
        .mark_batch_created("batch-123")
        .unwrap()
        .mark_submitted()
        .unwrap();
    assert!(checkpoint.mark_polling().is_ok());
    checkpoint = MinerUCheckpoint::source_stored(object(SOURCE_HASH));
    assert!(checkpoint.mark_polling().is_err());
}

#[test]
fn mineru_payload_accepts_only_content_addressed_sources_and_safe_file_names() {
    let payload = MinerUTaskPayload {
        document_id: SourceDocumentId::new(),
        version_id: DocumentVersionId::new(),
        provider_profile_id: "11111111-1111-4111-8111-111111111111".to_string(),
        provider_profile_revision: 1,
        provider_secret_generation: 0,
        embedding_profile_revision: 1,
        embedding_secret_generation: 0,
        source: object(SOURCE_HASH),
        file_name: "高炉标准.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
    };
    payload.validate().expect("validate MinerU payload");
    serde_json::from_str::<MinerUTaskPayload>(&serde_json::to_string(&payload).unwrap())
        .expect("round-trip MinerU payload")
        .validate()
        .expect("validate restored payload");

    let mut unsafe_payload = payload.clone();
    unsafe_payload.file_name = "../standard.pdf".to_string();
    assert_eq!(
        unsafe_payload.validate().unwrap_err().code(),
        "invalid_mineru_payload"
    );

    let unsafe_object = StoredObjectRef::new(SOURCE_HASH, "../outside.pdf").unwrap_err();
    assert_eq!(unsafe_object.code(), "invalid_stored_object");
}

#[test]
fn partial_checkpoint_cannot_claim_activation() {
    let checkpoint = MinerUCheckpoint::source_stored(object(SOURCE_HASH))
        .mark_submitting(SOURCE_HASH)
        .unwrap()
        .mark_batch_created("batch-123")
        .unwrap()
        .mark_submitted()
        .unwrap()
        .mark_polling()
        .unwrap()
        .mark_artifact_downloaded(object(ARTIFACT_HASH))
        .unwrap()
        .mark_parsed(object(AST_HASH))
        .unwrap();

    assert_eq!(
        checkpoint
            .mark_activated(DocumentVersionId::new())
            .unwrap_err()
            .code(),
        "invalid_mineru_transition"
    );
}

#[test]
fn mineru_artifact_normalizes_markdown_and_embeds_local_images() {
    let archive = mineru_archive(&[
        (
            "paper/full.md",
            b"# Blast furnace\n\n![burden](images/burden.png)\n\n| Fe | C |\n| --- | --- |\n| 94 | 4.5 |",
        ),
        ("paper/images/burden.png", b"png-bytes"),
    ]);

    let parsed = parse_mineru_artifact(&archive, ParseLimits::default()).expect("parse MinerU");

    assert!(matches!(
        &parsed.blocks[0],
        DocumentBlock::Heading { level: 1, text, .. } if text == "Blast furnace"
    ));
    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::Image {
            alt,
            asset_index: Some(0),
            ..
        } if alt == "burden"
    )));
    assert_eq!(parsed.assets.len(), 1);
    assert_eq!(
        parsed.assets[0].original_name.as_deref(),
        Some("burden.png")
    );
    assert_eq!(parsed.assets[0].media_type, "image/png");
    assert_eq!(parsed.assets[0].bytes, b"png-bytes");
    assert!(parsed.warnings.is_empty());
}

#[test]
fn mineru_artifact_never_loads_remote_images() {
    let archive = mineru_archive(&[(
        "paper/full.md",
        b"# Report\n\n![remote](https://example.invalid/private.png)",
    )]);

    let parsed = parse_mineru_artifact(&archive, ParseLimits::default()).expect("parse MinerU");

    assert!(parsed.assets.is_empty());
    assert_eq!(parsed.warnings.len(), 1);
    assert_eq!(parsed.warnings[0].code, "remote_asset_ignored");
}

#[test]
fn mineru_artifact_rejects_unsafe_ambiguous_or_missing_main_output() {
    for (archive, expected_code) in [
        (
            mineru_archive(&[("../full.md", b"escape")]),
            "archive_path_traversal",
        ),
        (
            mineru_archive(&[("a/full.md", b"first"), ("b/full.md", b"second")]),
            "mineru_main_ambiguous",
        ),
        (
            mineru_archive(&[("paper/content.json", b"{}")]),
            "mineru_main_missing",
        ),
    ] {
        assert_eq!(
            parse_mineru_artifact(&archive, ParseLimits::default())
                .unwrap_err()
                .code(),
            expected_code
        );
    }
}

fn mineru_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut cursor);
        for (name, bytes) in entries {
            archive
                .start_file(*name, SimpleFileOptions::default())
                .expect("start ZIP entry");
            archive.write_all(bytes).expect("write ZIP entry");
        }
        archive.finish().expect("finish ZIP");
    }
    cursor.into_inner()
}

#[test]
fn mineru_handler_resumes_polling_without_duplicate_submit_and_activates() {
    let workspace = TestWorkspace::new();
    let source = workspace.store(b"pdf-source");
    let artifact = mineru_archive(&[("paper/full.md", b"# Restart safe")]);
    let remote = Arc::new(FakeRemote::new(
        vec![DocumentTaskState::Running, DocumentTaskState::Completed],
        artifact,
    ));
    let processor = Arc::new(FakePostprocessor::default());
    let handler = Arc::new(MinerUTaskHandler::new(
        workspace.root.clone(),
        Arc::new(FakeRemoteFactory(remote.clone())),
        processor.clone(),
        Duration::from_secs(60 * 60),
    ));
    let task = workspace.create_task(payload(source));

    let mut first = workspace.scheduler(handler.clone());
    drive_until(
        "initial wait",
        &mut first,
        || workspace.task(task.id).state == TaskState::WaitingExternal,
        || task_diagnostic(&workspace, task.id, &remote),
    );
    let waiting = workspace.task(task.id);
    let checkpoint = decode_mineru_checkpoint(waiting.checkpoint_json.as_deref().unwrap()).unwrap();
    assert_eq!(checkpoint.stage(), MinerUStage::Polling);
    assert_eq!(remote.counts().create, 1);
    assert_eq!(remote.counts().upload, 1);
    assert_eq!(remote.counts().download, 0);
    drop(first);

    let mut restarted = workspace.scheduler_with_clock(
        handler,
        Arc::new(FixedClock(Utc::now() + chrono::Duration::hours(2))),
    );
    restarted.recover().expect("recover scheduler");
    drive_until(
        "restart completion",
        &mut restarted,
        || workspace.task(task.id).state == TaskState::Completed,
        || task_diagnostic(&workspace, task.id, &remote),
    );

    let completed = workspace.task(task.id);
    let checkpoint =
        decode_mineru_checkpoint(completed.checkpoint_json.as_deref().unwrap()).unwrap();
    assert_eq!(checkpoint.stage(), MinerUStage::Activated);
    assert_eq!(completed.progress, 100);
    assert_eq!(
        remote.counts(),
        RemoteCounts {
            create: 1,
            upload: 1,
            poll: 2,
            download: 1,
            cancel: 0,
        }
    );
    assert_eq!(
        processor.calls(),
        vec!["chunk", "embed", "index", "activate"]
    );
}

#[test]
fn mineru_handler_cancels_locally_when_remote_cancel_is_unsupported() {
    let workspace = TestWorkspace::new();
    let source = workspace.store(b"cancel-source");
    let remote = Arc::new(FakeRemote::new(
        vec![DocumentTaskState::Running],
        mineru_archive(&[("paper/full.md", b"unused")]),
    ));
    let handler = Arc::new(MinerUTaskHandler::new(
        workspace.root.clone(),
        Arc::new(FakeRemoteFactory(remote.clone())),
        Arc::new(FakePostprocessor::default()),
        Duration::ZERO,
    ));
    let checkpoint = MinerUCheckpoint::source_stored(source.clone())
        .mark_submitting(SOURCE_HASH)
        .unwrap()
        .mark_batch_created("batch-1")
        .unwrap()
        .mark_submitted()
        .unwrap()
        .mark_polling()
        .unwrap();
    let task = workspace.create_task_with_checkpoint(payload(source), checkpoint);
    remote.cancel_during_poll(workspace.database.clone(), task.id);

    let mut scheduler = workspace.scheduler(handler);
    drive_until(
        "cancellation",
        &mut scheduler,
        || workspace.task(task.id).state == TaskState::Cancelled,
        || task_diagnostic(&workspace, task.id, &remote),
    );

    assert_eq!(remote.counts().create, 0);
    assert_eq!(remote.counts().cancel, 1);
    assert_eq!(workspace.task(task.id).error_code, None);
}

#[test]
fn mineru_handler_resumes_local_stages_without_loading_remote_provider() {
    let workspace = TestWorkspace::new();
    let source = workspace.store(b"local-resume-source");
    let artifact = workspace.store(b"artifact");
    let parsed = workspace.store(b"parsed-ast");
    let checkpoint = MinerUCheckpoint::source_stored(source.clone())
        .mark_submitting(SOURCE_HASH)
        .unwrap()
        .mark_batch_created("batch-1")
        .unwrap()
        .mark_submitted()
        .unwrap()
        .mark_polling()
        .unwrap()
        .mark_artifact_downloaded(artifact)
        .unwrap()
        .mark_parsed(parsed)
        .unwrap();
    let processor = Arc::new(FakePostprocessor::default());
    let handler = Arc::new(MinerUTaskHandler::new(
        workspace.root.clone(),
        Arc::new(UnavailableRemoteFactory),
        processor.clone(),
        Duration::ZERO,
    ));
    let task = workspace.create_task_with_checkpoint(payload(source), checkpoint);

    let mut scheduler = workspace.scheduler(handler);
    drive_until(
        "local stage recovery",
        &mut scheduler,
        || {
            matches!(
                workspace.task(task.id).state,
                TaskState::Completed
                    | TaskState::Failed
                    | TaskState::Cancelled
                    | TaskState::Interrupted
            )
        },
        || task_diagnostic_without_remote(&workspace, task.id),
    );

    assert_eq!(workspace.task(task.id).state, TaskState::Completed);
    assert_eq!(
        processor.calls(),
        vec!["chunk", "embed", "index", "activate"]
    );
}

#[test]
fn mineru_handler_never_resubmits_an_unknown_submit_outcome() {
    let workspace = TestWorkspace::new();
    let source = workspace.store(b"uncertain-submit-source");
    let checkpoint = MinerUCheckpoint::source_stored(source.clone())
        .mark_submitting(SOURCE_HASH)
        .unwrap();
    let remote = Arc::new(FakeRemote::new(Vec::new(), Vec::new()));
    let handler = Arc::new(MinerUTaskHandler::new(
        workspace.root.clone(),
        Arc::new(FakeRemoteFactory(remote.clone())),
        Arc::new(FakePostprocessor::default()),
        Duration::ZERO,
    ));
    let task = workspace.create_task_with_checkpoint(payload(source), checkpoint);

    let mut scheduler = workspace.scheduler(handler);
    drive_until(
        "unknown submit outcome",
        &mut scheduler,
        || workspace.task(task.id).state == TaskState::Failed,
        || task_diagnostic(&workspace, task.id, &remote),
    );

    assert_eq!(remote.counts().create, 0);
    assert_eq!(
        workspace.task(task.id).error_code.as_deref(),
        Some("mineru_submit_outcome_unknown")
    );
}

#[test]
fn mineru_handler_persists_batch_id_before_upload_failure() {
    let workspace = TestWorkspace::new();
    let source = workspace.store(b"upload-failure-source");
    let remote = Arc::new(FakeRemote::new(Vec::new(), Vec::new()));
    remote.fail_upload();
    let handler = Arc::new(MinerUTaskHandler::new(
        workspace.root.clone(),
        Arc::new(FakeRemoteFactory(remote.clone())),
        Arc::new(FakePostprocessor::default()),
        Duration::ZERO,
    ));
    let task = workspace.create_task(payload(source));

    let mut scheduler = workspace.scheduler(handler);
    drive_until(
        "upload failure",
        &mut scheduler,
        || workspace.task(task.id).state == TaskState::Failed,
        || task_diagnostic(&workspace, task.id, &remote),
    );

    let failed = workspace.task(task.id);
    let checkpoint = decode_mineru_checkpoint(failed.checkpoint_json.as_deref().unwrap()).unwrap();
    assert_eq!(checkpoint.stage(), MinerUStage::BatchCreated);
    assert_eq!(checkpoint.remote_task_id(), Some("batch-1"));
    assert_eq!(remote.counts().create, 1);
    assert_eq!(remote.counts().upload, 1);
    assert_eq!(remote.counts().poll, 0);
    assert_eq!(
        failed.error_code.as_deref(),
        Some("mineru_upload_outcome_unknown")
    );
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RemoteCounts {
    create: usize,
    upload: usize,
    poll: usize,
    download: usize,
    cancel: usize,
}

struct FakeRemote {
    states: Mutex<VecDeque<DocumentTaskState>>,
    artifact: Vec<u8>,
    counts: Mutex<RemoteCounts>,
    cancel_hook: Mutex<Option<(PathBuf, Uuid)>>,
    fail_upload: Mutex<bool>,
}

impl FakeRemote {
    fn new(states: Vec<DocumentTaskState>, artifact: Vec<u8>) -> Self {
        Self {
            states: Mutex::new(states.into()),
            artifact,
            counts: Mutex::new(RemoteCounts::default()),
            cancel_hook: Mutex::new(None),
            fail_upload: Mutex::new(false),
        }
    }

    fn counts(&self) -> RemoteCounts {
        *self.counts.lock().unwrap()
    }

    fn cancel_during_poll(&self, database: PathBuf, task_id: Uuid) {
        *self.cancel_hook.lock().unwrap() = Some((database, task_id));
    }

    fn fail_upload(&self) {
        *self.fail_upload.lock().unwrap() = true;
    }
}

impl MinerURemote for FakeRemote {
    fn create_batch(
        &self,
        _request: DocumentParseRequest,
    ) -> MinerURemoteFuture<MinerUUploadTicket> {
        self.counts.lock().unwrap().create += 1;
        Box::pin(async {
            MinerUUploadTicket::new(
                RemoteTaskId("batch-1".to_string()),
                "memory://upload",
                Vec::new(),
            )
        })
    }

    fn upload(&self, _ticket: MinerUUploadTicket) -> MinerURemoteFuture<()> {
        self.counts.lock().unwrap().upload += 1;
        let fail = *self.fail_upload.lock().unwrap();
        Box::pin(async move {
            if fail {
                Err(ProviderError::new(
                    ProviderErrorCode::Timeout,
                    None,
                    "injected upload failure",
                ))
            } else {
                Ok(())
            }
        })
    }

    fn poll(&self, id: RemoteTaskId) -> MinerURemoteFuture<DocumentTaskStatus> {
        self.counts.lock().unwrap().poll += 1;
        if let Some((path, task_id)) = self.cancel_hook.lock().unwrap().take() {
            let mut connection = Connection::open(path).expect("open cancellation database");
            let task = repository::get(&connection, "workspace-a", task_id)
                .unwrap()
                .unwrap();
            repository::request_running_cancellation(
                &mut connection,
                "workspace-a",
                task_id,
                task.attempt,
            )
            .expect("request running cancellation");
        }
        let state = self
            .states
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(DocumentTaskState::Running);
        Box::pin(async move {
            Ok(DocumentTaskStatus {
                id,
                state,
                progress_percent: None,
            })
        })
    }

    fn download(&self, _id: RemoteTaskId) -> MinerURemoteFuture<ParsedDocumentArtifact> {
        self.counts.lock().unwrap().download += 1;
        let bytes = self.artifact.clone();
        Box::pin(async move {
            Ok(ParsedDocumentArtifact {
                file_name: "batch-1.zip".to_string(),
                mime_type: "application/zip".to_string(),
                bytes,
            })
        })
    }

    fn cancel(&self, _id: RemoteTaskId) -> MinerURemoteFuture<()> {
        self.counts.lock().unwrap().cancel += 1;
        Box::pin(async {
            Err(ProviderError::new(
                ProviderErrorCode::UnsupportedCapability,
                None,
                "remote cancellation unsupported",
            ))
        })
    }
}

struct FakeRemoteFactory(Arc<FakeRemote>);

impl MinerURemoteFactory for FakeRemoteFactory {
    fn load(
        &self,
        _workspace_id: &str,
        _profile_id: Uuid,
        _expected_revision: u64,
        _expected_secret_generation: u64,
    ) -> Result<Arc<dyn MinerURemote>, HandlerError> {
        Ok(self.0.clone())
    }
}

struct UnavailableRemoteFactory;

impl MinerURemoteFactory for UnavailableRemoteFactory {
    fn load(
        &self,
        _workspace_id: &str,
        _profile_id: Uuid,
        _expected_revision: u64,
        _expected_secret_generation: u64,
    ) -> Result<Arc<dyn MinerURemote>, HandlerError> {
        Err(HandlerError::permanent("mineru_profile_missing"))
    }
}

#[derive(Default)]
struct FakePostprocessor {
    calls: Mutex<Vec<&'static str>>,
}

impl FakePostprocessor {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

impl MinerUPostprocessor for FakePostprocessor {
    fn chunk(
        &self,
        workspace_id: String,
        _payload: MinerUTaskPayload,
        _parsed_ast: StoredObjectRef,
    ) -> MinerUProcessFuture<String> {
        assert_eq!(workspace_id, "workspace-a");
        self.calls.lock().unwrap().push("chunk");
        Box::pin(async {
            Ok("dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".into())
        })
    }

    fn embed(
        &self,
        workspace_id: String,
        _payload: MinerUTaskPayload,
        _is_cancelled: bloomery::rag::tasks::CancellationCheck,
    ) -> MinerUProcessFuture<String> {
        assert_eq!(workspace_id, "workspace-a");
        self.calls.lock().unwrap().push("embed");
        Box::pin(async {
            Ok("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into())
        })
    }

    fn index(
        &self,
        workspace_id: String,
        _payload: MinerUTaskPayload,
    ) -> MinerUProcessFuture<String> {
        assert_eq!(workspace_id, "workspace-a");
        self.calls.lock().unwrap().push("index");
        Box::pin(async {
            Ok("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".into())
        })
    }

    fn activate(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
        _finalization: TaskFinalization,
    ) -> MinerUProcessFuture<DocumentVersionId> {
        assert_eq!(workspace_id, "workspace-a");
        self.calls.lock().unwrap().push("activate");
        Box::pin(async move { Ok(payload.version_id) })
    }
}

struct TestWorkspace {
    root: PathBuf,
    database: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("bloomery-mineru-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create test workspace");
        let database = root.join("tasks.sqlite3");
        let mut connection = Connection::open(&database).expect("open test database");
        migrate(&mut connection).expect("migrate test database");
        Self { root, database }
    }

    fn store(&self, bytes: &[u8]) -> StoredObjectRef {
        let hash = format!("{:x}", Sha256::digest(bytes));
        let object = object(&hash);
        let path = self.root.join(Path::new(object.storage_key()));
        fs::create_dir_all(path.parent().unwrap()).expect("create object directory");
        fs::write(path, bytes).expect("write object");
        object
    }

    fn create_task(&self, payload: MinerUTaskPayload) -> bloomery::tasks::TaskRecord {
        let checkpoint = MinerUCheckpoint::source_stored(payload.source.clone());
        self.create_task_with_checkpoint(payload, checkpoint)
    }

    fn create_task_with_checkpoint(
        &self,
        payload: MinerUTaskPayload,
        checkpoint: MinerUCheckpoint,
    ) -> bloomery::tasks::TaskRecord {
        repository::create(
            &mut Connection::open(&self.database).unwrap(),
            NewTask {
                workspace_id: "workspace-a".to_string(),
                kind: MINERU_TASK_KIND.to_string(),
                payload_json: serde_json::to_string(&payload).unwrap(),
                checkpoint_json: Some(serde_json::to_string(&checkpoint).unwrap()),
                next_run_at: None,
                progress: checkpoint.progress(),
            },
        )
        .expect("create MinerU task")
    }

    fn task(&self, id: Uuid) -> bloomery::tasks::TaskRecord {
        repository::get(
            &Connection::open(&self.database).expect("open task database"),
            "workspace-a",
            id,
        )
        .unwrap()
        .unwrap()
    }

    fn scheduler(&self, handler: Arc<impl TaskHandler + 'static>) -> Scheduler {
        self.scheduler_with_clock(handler, Arc::new(SystemClock))
    }

    fn scheduler_with_clock(
        &self,
        handler: Arc<impl TaskHandler + 'static>,
        clock: Arc<dyn Clock>,
    ) -> Scheduler {
        Scheduler::new(
            self.database.clone(),
            "workspace-a".to_string(),
            SchedulerConfig {
                max_workers: 1,
                max_attempts: 3,
                retry_base: Duration::ZERO,
                retry_max: Duration::ZERO,
                poll_interval: Duration::ZERO,
            },
            clock,
            vec![handler],
            Arc::new(SilentSink),
        )
        .expect("create scheduler")
    }
}

struct FixedClock(DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct SilentSink;

impl EventSink for SilentSink {
    fn emit(&self, _event: SchedulerEvent) {}
}

fn payload(source: StoredObjectRef) -> MinerUTaskPayload {
    MinerUTaskPayload {
        document_id: SourceDocumentId::new(),
        version_id: DocumentVersionId::new(),
        provider_profile_id: "11111111-1111-4111-8111-111111111111".to_string(),
        provider_profile_revision: 1,
        provider_secret_generation: 0,
        embedding_profile_revision: 1,
        embedding_secret_generation: 0,
        source,
        file_name: "standard.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
    }
}

fn drive_until(
    label: &str,
    scheduler: &mut Scheduler,
    mut condition: impl FnMut() -> bool,
    diagnostic: impl Fn() -> String,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        scheduler.tick().expect("scheduler tick");
        assert!(
            Instant::now() < deadline,
            "MinerU scheduler timed out during {label}: {}",
            diagnostic()
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn task_diagnostic(workspace: &TestWorkspace, id: Uuid, remote: &FakeRemote) -> String {
    let task = workspace.task(id);
    format!(
        "state={:?}, attempt={}, error={:?}, next={:?}, progress={}, remote={:?}",
        task.state,
        task.attempt,
        task.error_code,
        task.next_run_at,
        task.progress,
        remote.counts()
    )
}

fn task_diagnostic_without_remote(workspace: &TestWorkspace, id: Uuid) -> String {
    let task = workspace.task(id);
    format!(
        "state={:?}, progress={}, attempt={}, error={:?}",
        task.state, task.progress, task.attempt, task.error_code
    )
}
