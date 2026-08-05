use super::lifecycle::{build_hnsw, VectorRecord};
use super::vector::IndexWatermark;
use crate::rag::model::{ChunkId, DocumentVersionId};
use crate::tasks::scheduler::{
    HandlerContext, HandlerError, HandlerFuture, HandlerOutcome, TaskHandler,
};
use crate::tasks::{repository, NewTask, TaskRecord};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

pub const INDEX_REBUILD_KIND: &str = "rag_index_rebuild";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRebuildRequest {
    pub provider_profile_id: String,
    pub model_id: String,
    pub dimension: u32,
}

#[derive(Debug, Clone)]
pub struct IndexSnapshot {
    pub watermark: IndexWatermark,
    pub records: Vec<VectorRecord>,
}

impl IndexRebuildRequest {
    fn validate(&self) -> Result<(), String> {
        Uuid::parse_str(&self.provider_profile_id)
            .map_err(|error| format!("invalid provider profile ID: {error}"))?;
        if self.model_id.trim().is_empty() || self.model_id.trim() != self.model_id {
            return Err("index model ID is invalid".to_string());
        }
        if self.dimension == 0 {
            return Err("index dimension must be positive".to_string());
        }
        Ok(())
    }
}

pub fn queue_index_rebuild(
    connection: &mut Connection,
    workspace_id: &str,
    request: IndexRebuildRequest,
) -> Result<Uuid, String> {
    request.validate()?;
    let task = repository::create(
        connection,
        NewTask {
            workspace_id: workspace_id.to_string(),
            kind: INDEX_REBUILD_KIND.to_string(),
            payload_json: serde_json::to_string(&request).map_err(|error| error.to_string())?,
            checkpoint_json: None,
            next_run_at: None,
            progress: 0,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(task.id)
}

pub fn load_index_snapshot(
    connection: &Connection,
    workspace_id: &str,
    request: &IndexRebuildRequest,
) -> Result<IndexSnapshot, String> {
    request.validate()?;
    let mut statement = connection
        .prepare(
            "SELECT embeddings.version_id, embeddings.chunk_id, vectors.vector_blob,
                    vectors.vector_sha256
             FROM knowledge_chunk_embeddings embeddings
             JOIN knowledge_vectors vectors
               ON vectors.workspace_id = embeddings.workspace_id
              AND vectors.id = embeddings.vector_key
             WHERE embeddings.workspace_id = ?1
               AND embeddings.provider_profile_id = ?2
               AND embeddings.model_id = ?3
               AND embeddings.dimension = ?4
             ORDER BY embeddings.version_id, embeddings.chunk_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params![
                workspace_id,
                request.provider_profile_id,
                request.model_id,
                request.dimension
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    let mut records = Vec::new();
    let mut watermark = Sha256::new();
    for row in rows {
        let (version, chunk, blob, expected_sha256) = row.map_err(|error| error.to_string())?;
        if format!("{:x}", Sha256::digest(&blob)) != expected_sha256 {
            return Err("index_checksum_mismatch: SQLite vector checksum mismatch".to_string());
        }
        if blob.len() != request.dimension as usize * 4 {
            return Err("index_vector_invalid: stored vector dimension mismatch".to_string());
        }
        let vector = blob
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte vector value")))
            .collect::<Vec<_>>();
        if vector.iter().any(|value| !value.is_finite()) {
            return Err(
                "index_vector_invalid: stored vector contains non-finite value".to_string(),
            );
        }
        watermark.update(version.as_bytes());
        watermark.update([0]);
        watermark.update(chunk.as_bytes());
        watermark.update([0]);
        watermark.update(expected_sha256.as_bytes());
        records.push(VectorRecord {
            version_id: DocumentVersionId::from_str(&version).map_err(|error| error.to_string())?,
            chunk_id: ChunkId::new(chunk)?,
            vector,
        });
    }
    Ok(IndexSnapshot {
        watermark: IndexWatermark {
            format_version: 1,
            workspace_id: workspace_id.to_string(),
            provider_profile_id: request.provider_profile_id.clone(),
            model_id: request.model_id.clone(),
            dimension: request.dimension,
            chunk_count: u32::try_from(records.len())
                .map_err(|_| "index contains too many chunks".to_string())?,
            sqlite_watermark: format!("{:x}", watermark.finalize()),
        },
        records,
    })
}

pub fn index_root(content_root: &Path, watermark: &IndexWatermark) -> PathBuf {
    let mut identity = Sha256::new();
    identity.update(watermark.provider_profile_id.as_bytes());
    identity.update([0]);
    identity.update(watermark.model_id.as_bytes());
    identity.update([0]);
    identity.update(watermark.dimension.to_le_bytes());
    content_root
        .join("indexes")
        .join(format!("{:x}", identity.finalize()))
}

pub struct IndexRebuildHandler {
    database: PathBuf,
    content_root: PathBuf,
}

impl IndexRebuildHandler {
    pub fn new(database: PathBuf, content_root: PathBuf) -> Self {
        Self {
            database,
            content_root,
        }
    }
}

impl TaskHandler for IndexRebuildHandler {
    fn kind(&self) -> &str {
        INDEX_REBUILD_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let database = self.database.clone();
        let content_root = self.content_root.clone();
        Box::pin(async move {
            if context.cancellation_requested().map_err(task_error)? {
                return Ok(HandlerOutcome::Cancelled);
            }
            let request: IndexRebuildRequest = serde_json::from_str(&task.payload_json)
                .map_err(|_| HandlerError::permanent("index_rebuild_payload_invalid"))?;
            request
                .validate()
                .map_err(|_| HandlerError::permanent("index_rebuild_payload_invalid"))?;
            context.checkpoint(None, 10, None).map_err(task_error)?;
            let connection = Connection::open(&database).map_err(sqlite_error)?;
            connection
                .busy_timeout(Duration::from_secs(5))
                .map_err(sqlite_error)?;
            let snapshot = load_index_snapshot(&connection, &task.workspace_id, &request)
                .map_err(snapshot_error)?;
            drop(connection);
            context.checkpoint(None, 40, None).map_err(task_error)?;
            if context.cancellation_requested().map_err(task_error)? {
                return Ok(HandlerOutcome::Cancelled);
            }
            let root = index_root(&content_root, &snapshot.watermark);
            let built = build_hnsw(&root, snapshot.watermark.clone(), &snapshot.records)
                .map_err(index_error)?;
            let checkpoint = serde_json::json!({
                "generation_id": built.generation_id,
                "chunk_count": snapshot.watermark.chunk_count,
                "sqlite_watermark": snapshot.watermark.sqlite_watermark,
            })
            .to_string();
            context
                .checkpoint(Some(&checkpoint), 95, None)
                .map_err(task_error)?;
            if context.cancellation_requested().map_err(task_error)? {
                Ok(HandlerOutcome::Cancelled)
            } else {
                Ok(HandlerOutcome::Completed)
            }
        })
    }
}

fn snapshot_error(error: String) -> HandlerError {
    if error.to_ascii_lowercase().contains("database is locked") {
        HandlerError::retryable("index_rebuild_storage_busy")
    } else {
        HandlerError::permanent("index_rebuild_snapshot_failed")
    }
}

fn sqlite_error(error: rusqlite::Error) -> HandlerError {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            HandlerError::retryable("index_rebuild_storage_busy")
        }
        _ => HandlerError::permanent("index_rebuild_storage_failed"),
    }
}

fn index_error(error: super::vector::IndexError) -> HandlerError {
    if matches!(error.code(), "index_io_failed" | "index_worker_failed") {
        HandlerError::retryable(error.code())
    } else {
        HandlerError::permanent(error.code())
    }
}

fn task_error(error: crate::tasks::TaskError) -> HandlerError {
    HandlerError::retryable(error.code())
}
