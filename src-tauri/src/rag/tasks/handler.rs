use super::handler_api::{
    MinerUPostprocessor, MinerURemoteFactory, TaskFinalization, MINERU_TASK_KIND,
};
use super::handler_support::{
    cancel_remote, cancellation_requested, checkpoint_error, persist, persist_final,
    provider_error, remote_id, storage_error, submit_request_hash, task_operation_error,
};
use super::store::ContentStore;
use super::{decode_mineru_checkpoint, MinerUCheckpoint, MinerUStage, MinerUTaskPayload};
use crate::providers::capabilities::{DocumentParseRequest, DocumentTaskState};
use crate::rag::ingest::SourceFormat;
use crate::rag::parse::{parse_document_bytes, parse_mineru_artifact, ParseLimits};
use crate::tasks::scheduler::{
    HandlerContext, HandlerError, HandlerFuture, HandlerOutcome, TaskHandler,
};
use crate::tasks::TaskRecord;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const MAX_SOURCE_BYTES: u64 = 200 * 1024 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_AST_BYTES: u64 = 512 * 1024 * 1024;

pub struct MinerUTaskHandler {
    store: ContentStore,
    remotes: Arc<dyn MinerURemoteFactory>,
    postprocessor: Arc<dyn MinerUPostprocessor>,
    poll_interval: Duration,
}

impl MinerUTaskHandler {
    pub fn new(
        content_root: PathBuf,
        remotes: Arc<dyn MinerURemoteFactory>,
        postprocessor: Arc<dyn MinerUPostprocessor>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            store: ContentStore::new(content_root),
            remotes,
            postprocessor,
            poll_interval,
        }
    }
}

impl TaskHandler for MinerUTaskHandler {
    fn kind(&self) -> &str {
        MINERU_TASK_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let store = self.store.clone();
        let remotes = Arc::clone(&self.remotes);
        let postprocessor = Arc::clone(&self.postprocessor);
        let poll_interval = self.poll_interval;
        Box::pin(async move {
            run_task(task, context, store, remotes, postprocessor, poll_interval).await
        })
    }
}

async fn run_task(
    task: TaskRecord,
    context: HandlerContext,
    store: ContentStore,
    remotes: Arc<dyn MinerURemoteFactory>,
    postprocessor: Arc<dyn MinerUPostprocessor>,
    poll_interval: Duration,
) -> Result<HandlerOutcome, HandlerError> {
    let payload: MinerUTaskPayload = serde_json::from_str(&task.payload_json)
        .map_err(|_| HandlerError::permanent("invalid_mineru_payload"))?;
    payload
        .validate()
        .map_err(|_| HandlerError::permanent("invalid_mineru_payload"))?;
    let mut checkpoint = match task.checkpoint_json.as_deref() {
        Some(value) => decode_mineru_checkpoint(value)
            .map_err(|_| HandlerError::permanent("invalid_mineru_checkpoint"))?,
        None => persist(
            &context,
            MinerUCheckpoint::source_stored(payload.source.clone()),
            None,
        )?,
    };
    if checkpoint.source() != &payload.source {
        return Err(HandlerError::permanent("invalid_mineru_checkpoint"));
    }
    let profile_id = payload
        .provider_profile_id
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| HandlerError::permanent("invalid_mineru_payload"))?;

    loop {
        if cancellation_requested(&context)? {
            if let Some(profile_id) = profile_id {
                cancel_remote(
                    remotes.as_ref(),
                    &task.workspace_id,
                    profile_id,
                    payload.provider_profile_revision,
                    payload.provider_secret_generation,
                    &checkpoint,
                )
                .await;
            }
            return Ok(HandlerOutcome::Cancelled);
        }
        if context.shutdown_requested() {
            return Ok(HandlerOutcome::Interrupted);
        }
        checkpoint = match checkpoint.stage() {
            MinerUStage::SourceStored => {
                if profile_id.is_none() {
                    let bytes = store
                        .read(&payload.source, MAX_SOURCE_BYTES)
                        .map_err(storage_error)?;
                    let format =
                        SourceFormat::from_mime_type(&payload.mime_type).ok_or_else(|| {
                            HandlerError::permanent("local_parser_unsupported_format")
                        })?;
                    let parsed = parse_document_bytes(&bytes, format, ParseLimits::default())
                        .map_err(|error| HandlerError::permanent(error.code()))?;
                    let ast = serde_json::to_vec(&parsed)
                        .map_err(|_| HandlerError::permanent("mineru_ast_encode_failed"))?;
                    if ast.len() as u64 > MAX_AST_BYTES {
                        return Err(HandlerError::permanent("mineru_ast_too_large"));
                    }
                    let object = store.put(&ast).map_err(storage_error)?;
                    persist(
                        &context,
                        checkpoint
                            .mark_local_parsed(object)
                            .map_err(checkpoint_error)?,
                        None,
                    )?
                } else {
                    let profile_id = profile_id.expect("checked above");
                    let remote = remotes.load(
                        &task.workspace_id,
                        profile_id,
                        payload.provider_profile_revision,
                        payload.provider_secret_generation,
                    )?;
                    let bytes = store
                        .read(&payload.source, MAX_SOURCE_BYTES)
                        .map_err(storage_error)?;
                    let submitting = persist(
                        &context,
                        checkpoint
                            .mark_submitting(submit_request_hash(&payload))
                            .map_err(checkpoint_error)?,
                        None,
                    )?;
                    let ticket = remote
                        .create_batch(DocumentParseRequest {
                            file_name: payload.file_name.clone(),
                            mime_type: payload.mime_type.clone(),
                            bytes,
                        })
                        .await
                        .map_err(|_| HandlerError::permanent("mineru_submit_outcome_unknown"))?;
                    let batch_created = persist(
                        &context,
                        submitting
                            .mark_batch_created(ticket.id().0.clone())
                            .map_err(checkpoint_error)?,
                        None,
                    )?;
                    remote
                        .upload(ticket)
                        .await
                        .map_err(|_| HandlerError::permanent("mineru_upload_outcome_unknown"))?;
                    persist(
                        &context,
                        batch_created.mark_submitted().map_err(checkpoint_error)?,
                        None,
                    )?
                }
            }
            MinerUStage::Submitting => {
                return Err(HandlerError::permanent("mineru_submit_outcome_unknown"));
            }
            MinerUStage::BatchCreated => {
                return Err(HandlerError::permanent("mineru_upload_ticket_unavailable"));
            }
            MinerUStage::Submitted => persist(
                &context,
                checkpoint.mark_polling().map_err(checkpoint_error)?,
                None,
            )?,
            MinerUStage::Polling => {
                let profile_id =
                    profile_id.ok_or_else(|| HandlerError::permanent("invalid_mineru_payload"))?;
                let remote = remotes.load(
                    &task.workspace_id,
                    profile_id,
                    payload.provider_profile_revision,
                    payload.provider_secret_generation,
                )?;
                let id = remote_id(&checkpoint)?;
                let status = remote.poll(id.clone()).await.map_err(provider_error)?;
                if status.id != id {
                    return Err(HandlerError::permanent("mineru_task_id_mismatch"));
                }
                if cancellation_requested(&context)? {
                    cancel_remote(
                        remotes.as_ref(),
                        &task.workspace_id,
                        profile_id,
                        payload.provider_profile_revision,
                        payload.provider_secret_generation,
                        &checkpoint,
                    )
                    .await;
                    return Ok(HandlerOutcome::Cancelled);
                }
                match status.state {
                    DocumentTaskState::Running => {
                        let next = Utc::now()
                            + chrono::Duration::from_std(poll_interval)
                                .map_err(|_| HandlerError::permanent("invalid_poll_interval"))?;
                        persist(&context, checkpoint, Some(next))?;
                        return Ok(HandlerOutcome::WaitingExternal);
                    }
                    DocumentTaskState::Completed => {
                        let artifact = remote.download(id).await.map_err(provider_error)?;
                        if artifact.mime_type != "application/zip" {
                            return Err(HandlerError::permanent("mineru_artifact_invalid"));
                        }
                        let object = store.put(&artifact.bytes).map_err(storage_error)?;
                        persist(
                            &context,
                            checkpoint
                                .mark_artifact_downloaded(object)
                                .map_err(checkpoint_error)?,
                            None,
                        )?
                    }
                    DocumentTaskState::Failed => {
                        return Err(HandlerError::permanent("mineru_remote_failed"));
                    }
                    DocumentTaskState::Cancelled => return Ok(HandlerOutcome::Cancelled),
                }
            }
            MinerUStage::ArtifactDownloaded => {
                let artifact = checkpoint
                    .artifact()
                    .ok_or_else(|| HandlerError::permanent("invalid_mineru_checkpoint"))?;
                let bytes = store
                    .read(artifact, MAX_ARTIFACT_BYTES)
                    .map_err(storage_error)?;
                let parsed = parse_mineru_artifact(&bytes, ParseLimits::default())
                    .map_err(|_| HandlerError::permanent("mineru_artifact_invalid"))?;
                let ast = serde_json::to_vec(&parsed)
                    .map_err(|_| HandlerError::permanent("mineru_ast_encode_failed"))?;
                if ast.len() as u64 > MAX_AST_BYTES {
                    return Err(HandlerError::permanent("mineru_ast_too_large"));
                }
                let object = store.put(&ast).map_err(storage_error)?;
                persist(
                    &context,
                    checkpoint.mark_parsed(object).map_err(checkpoint_error)?,
                    None,
                )?
            }
            MinerUStage::Parsed => {
                let parsed_ast = checkpoint
                    .parsed_ast()
                    .cloned()
                    .ok_or_else(|| HandlerError::permanent("invalid_mineru_checkpoint"))?;
                let hash = postprocessor
                    .chunk(task.workspace_id.clone(), payload.clone(), parsed_ast)
                    .await?;
                persist(
                    &context,
                    checkpoint.mark_chunked(hash).map_err(checkpoint_error)?,
                    None,
                )?
            }
            MinerUStage::Chunked => {
                let cancellation_context = context.clone();
                let cancellation_failure = Arc::new(Mutex::new(None));
                let captured_failure = Arc::clone(&cancellation_failure);
                let result = postprocessor
                    .embed(
                        task.workspace_id.clone(),
                        payload.clone(),
                        Arc::new(
                            move || match cancellation_context.cancellation_requested() {
                                Ok(cancelled) => cancelled,
                                Err(error) => {
                                    if let Ok(mut failure) = captured_failure.lock() {
                                        *failure = Some(task_operation_error(
                                            "mineru_cancellation_check_failed",
                                            error,
                                        ));
                                    }
                                    true
                                }
                            },
                        ),
                    )
                    .await;
                if let Ok(mut failure) = cancellation_failure.lock() {
                    if let Some(error) = failure.take() {
                        return Err(error);
                    }
                }
                let hash = result?;
                persist(
                    &context,
                    checkpoint.mark_embedded(hash).map_err(checkpoint_error)?,
                    None,
                )?
            }
            MinerUStage::Embedded => {
                let hash = postprocessor
                    .index(task.workspace_id.clone(), payload.clone())
                    .await?;
                persist(
                    &context,
                    checkpoint.mark_indexed(hash).map_err(checkpoint_error)?,
                    None,
                )?
            }
            MinerUStage::Indexed => {
                if cancellation_requested(&context)? {
                    return Ok(HandlerOutcome::Cancelled);
                }
                if context.shutdown_requested() {
                    return Ok(HandlerOutcome::Interrupted);
                }
                let activated = checkpoint
                    .clone()
                    .mark_activated(payload.version_id)
                    .map_err(checkpoint_error)?;
                let checkpoint_json = serde_json::to_string(&activated)
                    .map_err(|_| HandlerError::permanent("mineru_checkpoint_encode_failed"))?;
                let version_id = postprocessor
                    .activate(
                        task.workspace_id.clone(),
                        payload.clone(),
                        TaskFinalization::new(task.id, task.attempt, checkpoint_json.clone()),
                    )
                    .await?;
                if version_id != payload.version_id {
                    return Err(HandlerError::permanent("mineru_version_mismatch"));
                }
                persist_final(&context, activated, checkpoint_json)?
            }
            MinerUStage::Activated => return Ok(HandlerOutcome::Completed),
        };
    }
}
