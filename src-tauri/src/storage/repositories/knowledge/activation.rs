use super::support::{now, scope};
use super::{get_source_document, SourceDocumentRecord};
use crate::rag::model::{DocumentVersionId, SourceDocumentId};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

pub fn activate_document_version(
    connection: &mut Connection,
    workspace_id: &str,
    document_id: SourceDocumentId,
    version_id: DocumentVersionId,
) -> Result<SourceDocumentRecord, String> {
    activate(connection, workspace_id, document_id, version_id, None)
}

pub fn activate_document_version_for_task(
    connection: &mut Connection,
    workspace_id: &str,
    document_id: SourceDocumentId,
    version_id: DocumentVersionId,
    task_id: uuid::Uuid,
    attempt: u32,
    checkpoint_json: &str,
) -> Result<SourceDocumentRecord, String> {
    activate(
        connection,
        workspace_id,
        document_id,
        version_id,
        Some(TaskFinalization {
            task_id,
            attempt,
            checkpoint_json,
        }),
    )
}

struct TaskFinalization<'a> {
    task_id: uuid::Uuid,
    attempt: u32,
    checkpoint_json: &'a str,
}

fn activate(
    connection: &mut Connection,
    workspace_id: &str,
    document_id: SourceDocumentId,
    version_id: DocumentVersionId,
    finalization: Option<TaskFinalization<'_>>,
) -> Result<SourceDocumentRecord, String> {
    scope(workspace_id)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    if let Some(finalization) = finalization.as_ref() {
        let current = transaction
            .query_row(
                "SELECT 1 FROM background_tasks
                 WHERE workspace_id = ?1 AND id = ?2 AND kind = 'mineru_parse'
                   AND state = 'running' AND attempt = ?3 AND cancel_requested = 0",
                params![
                    workspace_id,
                    finalization.task_id.to_string(),
                    finalization.attempt
                ],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if current.is_none() {
            return Err(
                "task_finalization_blocked: task is no longer active or was cancelled".to_string(),
            );
        }
    }
    let expected = transaction
        .query_row(
            "SELECT expected_asset_count, expected_chunk_count, embedding_profile_id,
                    embedding_model_id, embedding_dimension, chunk_policy_version,
                    manifest_sealed
             FROM knowledge_document_versions
             WHERE workspace_id = ?1 AND id = ?2 AND document_id = ?3",
            params![
                workspace_id,
                version_id.to_string(),
                document_id.to_string()
            ],
            |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)? != 0,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "document version not found".to_string())?;
    let count = |sql: &str| -> Result<u32, String> {
        transaction
            .query_row(sql, params![workspace_id, version_id.to_string()], |row| {
                row.get(0)
            })
            .map_err(|error| error.to_string())
    };
    let assets =
        count("SELECT COUNT(*) FROM knowledge_assets WHERE workspace_id = ?1 AND version_id = ?2")?;
    let chunks =
        count("SELECT COUNT(*) FROM knowledge_chunks WHERE workspace_id = ?1 AND version_id = ?2")?;
    let embeddings: u32 = transaction
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk_embeddings
             WHERE workspace_id = ?1 AND version_id = ?2 AND provider_profile_id = ?3
               AND model_id = ?4 AND dimension = ?5 AND policy_version = ?6",
            params![
                workspace_id,
                version_id.to_string(),
                expected.2,
                expected.3,
                expected.4,
                expected.5
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let fts: (u32, u32) = transaction
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT chunk_id) FROM knowledge_chunks_fts
             WHERE workspace_id = ?1 AND version_id = ?2",
            params![workspace_id, version_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let watermark: u32 = transaction
        .query_row(
            "SELECT COUNT(*) FROM knowledge_vector_watermarks
             WHERE workspace_id = ?1 AND version_id = ?2 AND provider_profile_id = ?3
               AND model_id = ?4 AND dimension = ?5
               AND expected_count = ?6 AND indexed_count = ?6",
            params![
                workspace_id,
                version_id.to_string(),
                expected.2,
                expected.3,
                expected.4,
                expected.1
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !expected.6
        || assets != expected.0
        || chunks != expected.1
        || embeddings != expected.1
        || fts != (expected.1, expected.1)
        || watermark != 1
    {
        return Err("incomplete_document_version: index components are not ready".to_string());
    }
    let timestamp = now();
    let changed = transaction
        .execute(
            "UPDATE knowledge_source_documents SET active_version_id = ?1, updated_at = ?2
             WHERE workspace_id = ?3 AND id = ?4",
            params![
                version_id.to_string(),
                timestamp,
                workspace_id,
                document_id.to_string()
            ],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("source document not found".to_string());
    }
    transaction
        .execute(
            "UPDATE knowledge_document_versions SET activated_at = ?1
             WHERE workspace_id = ?2 AND id = ?3",
            params![timestamp, workspace_id, version_id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if let Some(finalization) = finalization {
        let completed = transaction
            .execute(
                "UPDATE background_tasks
                 SET state = 'completed', checkpoint_json = ?1, next_run_at = NULL,
                     progress = 100, error_code = NULL, cancel_requested = 0, updated_at = ?2
                 WHERE workspace_id = ?3 AND id = ?4 AND kind = 'mineru_parse'
                   AND state = 'running' AND attempt = ?5 AND cancel_requested = 0",
                params![
                    finalization.checkpoint_json,
                    timestamp,
                    workspace_id,
                    finalization.task_id.to_string(),
                    finalization.attempt
                ],
            )
            .map_err(|error| error.to_string())?;
        if completed != 1 {
            return Err("task_finalization_blocked: task state changed".to_string());
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    get_source_document(connection, workspace_id, document_id)?
        .ok_or_else(|| "source document not found".to_string())
}
