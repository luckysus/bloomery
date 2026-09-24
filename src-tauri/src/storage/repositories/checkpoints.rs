use crate::agent::runtime::{AgentContextCheckpoint, ContextCheckpointReason};
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn save(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
    checkpoint: &AgentContextCheckpoint,
    timestamp: DateTime<Utc>,
) -> Result<(), StorageError> {
    let mut durable_checkpoint = checkpoint.clone();
    for message in &mut durable_checkpoint.messages {
        // Image payloads are request-scoped and may contain sensitive base64
        // data. A checkpoint is durable state, so enforce this at the storage
        // boundary instead of trusting every caller to sanitize first.
        message.images.clear();
    }
    let checkpoint_json = serde_json::to_string(&durable_checkpoint)
        .map_err(|error| StorageError::new("agent_checkpoint_encode_failed", error.to_string()))?;
    if checkpoint_json.len() > 512 * 1024 {
        return Err(StorageError::new(
            "agent_checkpoint_too_large",
            "agent checkpoint exceeds 512 KiB",
        ));
    }
    connection
        .execute(
            "INSERT INTO agent_run_checkpoints
             (run_id, workspace_id, reason, checkpoint_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(run_id) DO UPDATE SET
               workspace_id = excluded.workspace_id,
               reason = excluded.reason,
               checkpoint_json = excluded.checkpoint_json,
               updated_at = excluded.updated_at",
            params![
                run_id.to_string(),
                workspace_id,
                reason_text(checkpoint.reason),
                checkpoint_json,
                timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            ],
        )
        .map_err(|error| StorageError::new("agent_checkpoint_storage_failed", error.to_string()))?;
    Ok(())
}

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Option<AgentContextCheckpoint>, StorageError> {
    connection
        .query_row(
            "SELECT checkpoint_json FROM agent_run_checkpoints
             WHERE workspace_id = ?1 AND run_id = ?2",
            params![workspace_id, run_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| StorageError::new("agent_checkpoint_storage_failed", error.to_string()))?
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                StorageError::new("agent_checkpoint_decode_failed", error.to_string())
            })
        })
        .transpose()
}

fn reason_text(reason: ContextCheckpointReason) -> &'static str {
    match reason {
        ContextCheckpointReason::ModelCall => "model_call",
        ContextCheckpointReason::AssistantResult => "assistant_result",
        ContextCheckpointReason::AssistantError => "assistant_error",
    }
}
