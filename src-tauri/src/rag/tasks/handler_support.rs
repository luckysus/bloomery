use super::handler_api::MinerURemoteFactory;
use super::{MinerUCheckpoint, MinerUTaskPayload, RagTaskError};
use crate::providers::capabilities::RemoteTaskId;
use crate::providers::http::{ProviderError, ProviderErrorCode};
use crate::tasks::scheduler::{HandlerContext, HandlerError};
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) fn persist_final(
    context: &HandlerContext,
    checkpoint: MinerUCheckpoint,
    checkpoint_json: String,
) -> Result<MinerUCheckpoint, HandlerError> {
    match context.checkpoint(Some(&checkpoint_json), checkpoint.progress(), None) {
        Ok(_) => Ok(checkpoint),
        Err(error) => match context.completed_with_checkpoint(&checkpoint_json) {
            Ok(true) => Ok(checkpoint),
            Ok(false) => Err(task_operation_error("mineru_checkpoint_failed", error)),
            Err(confirm_error) => Err(task_operation_error(
                "mineru_checkpoint_failed",
                confirm_error,
            )),
        },
    }
}

pub(super) fn persist(
    context: &HandlerContext,
    checkpoint: MinerUCheckpoint,
    next_run_at: Option<chrono::DateTime<Utc>>,
) -> Result<MinerUCheckpoint, HandlerError> {
    let json = serde_json::to_string(&checkpoint)
        .map_err(|_| HandlerError::permanent("mineru_checkpoint_encode_failed"))?;
    context
        .checkpoint(Some(&json), checkpoint.progress(), next_run_at)
        .map_err(|error| task_operation_error("mineru_checkpoint_failed", error))?;
    Ok(checkpoint)
}

pub(super) fn remote_id(checkpoint: &MinerUCheckpoint) -> Result<RemoteTaskId, HandlerError> {
    checkpoint
        .remote_task_id()
        .map(|value| RemoteTaskId(value.to_string()))
        .ok_or_else(|| HandlerError::permanent("invalid_mineru_checkpoint"))
}

pub(super) fn cancellation_requested(context: &HandlerContext) -> Result<bool, HandlerError> {
    context
        .cancellation_requested()
        .map_err(|error| task_operation_error("mineru_cancellation_check_failed", error))
}

pub(super) fn task_operation_error(
    code: &'static str,
    error: crate::tasks::TaskError,
) -> HandlerError {
    let message = error.to_string().to_ascii_lowercase();
    if error.code() == "storage_error"
        && (message.contains("database is locked") || message.contains("database is busy"))
    {
        HandlerError::retryable(code)
    } else {
        HandlerError::permanent(code)
    }
}

pub(super) async fn cancel_remote(
    remotes: &dyn MinerURemoteFactory,
    workspace_id: &str,
    profile_id: Uuid,
    profile_revision: u64,
    secret_generation: u64,
    checkpoint: &MinerUCheckpoint,
) {
    if let Ok(id) = remote_id(checkpoint) {
        if let Ok(remote) = remotes.load(
            workspace_id,
            profile_id,
            profile_revision,
            secret_generation,
        ) {
            let _ = remote.cancel(id).await;
        }
    }
}

pub(super) fn provider_error(error: ProviderError) -> HandlerError {
    let code = format!("mineru_{}", error.code().as_str());
    if matches!(
        error.code(),
        ProviderErrorCode::Network | ProviderErrorCode::Timeout | ProviderErrorCode::Quota
    ) || error.status().is_some_and(|status| status >= 500)
    {
        HandlerError::retryable(code)
    } else {
        HandlerError::permanent(code)
    }
}

pub(super) fn submit_request_hash(payload: &MinerUTaskPayload) -> String {
    let mut digest = Sha256::new();
    digest.update(payload.source.sha256().as_bytes());
    digest.update(payload.file_name.as_bytes());
    digest.update(payload.mime_type.as_bytes());
    if let Some(provider_profile_id) = &payload.provider_profile_id {
        digest.update(provider_profile_id.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

pub(super) fn storage_error(error: RagTaskError) -> HandlerError {
    HandlerError::permanent(error.code())
}

pub(super) fn checkpoint_error(_: RagTaskError) -> HandlerError {
    HandlerError::permanent("invalid_mineru_checkpoint")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::TaskError;

    #[test]
    fn sqlite_busy_task_errors_remain_retryable() {
        let busy = task_operation_error(
            "mineru_checkpoint_failed",
            TaskError::new("storage_error", "database is locked"),
        );
        assert_eq!(busy.code(), "mineru_checkpoint_failed");
        assert!(busy.is_retryable());
        assert!(!task_operation_error(
            "mineru_checkpoint_failed",
            TaskError::new("stale_claim", "worker no longer owns task"),
        )
        .is_retryable());
    }
}
