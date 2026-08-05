use super::store::ContentStore;
use super::{
    CancellationCheck, MinerUPostprocessor, MinerUProcessFuture, MinerUTaskPayload,
    StoredObjectRef, TaskFinalization,
};
use crate::rag::chunk::{chunk_document, ChunkPolicy};
use crate::rag::index::{embed_version, EmbeddingIndexRequest, EmbeddingRemoteFactory};
use crate::rag::model::{DocumentVersionId, NewAsset, NewChunk};
use crate::rag::parse::ParsedDocument;
use crate::storage::repositories::knowledge;
use crate::tasks::scheduler::HandlerError;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const MAX_AST_BYTES: u64 = 512 * 1024 * 1024;

pub struct LocalRagPostprocessor {
    database: PathBuf,
    store: ContentStore,
    embeddings: Arc<dyn EmbeddingRemoteFactory>,
}

impl LocalRagPostprocessor {
    pub fn new(
        database: PathBuf,
        content_root: PathBuf,
        embeddings: Arc<dyn EmbeddingRemoteFactory>,
    ) -> Self {
        Self {
            database,
            store: ContentStore::new(content_root),
            embeddings,
        }
    }
}

impl MinerUPostprocessor for LocalRagPostprocessor {
    fn chunk(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
        parsed_ast: StoredObjectRef,
    ) -> MinerUProcessFuture<String> {
        let database = self.database.clone();
        let store = self.store.clone();
        Box::pin(async move {
            let bytes = store.read(&parsed_ast, MAX_AST_BYTES).map_err(rag_error)?;
            let document: ParsedDocument = serde_json::from_slice(&bytes)
                .map_err(|_| HandlerError::permanent("mineru_ast_invalid"))?;
            let mut connection = open(&database)?;
            let version = version(&connection, &workspace_id, &payload)?;
            let policy = ChunkPolicy {
                version: version.chunk_policy_version.clone(),
                ..ChunkPolicy::default()
            };
            let chunks = chunk_document(&document, &policy)
                .map_err(|_| HandlerError::permanent("chunking_failed"))?;
            if version.manifest_sealed && chunks.len() != version.expected_chunk_count as usize {
                return Err(HandlerError::permanent("chunk_count_mismatch"));
            }

            let mut seen_assets = HashSet::new();
            let mut assets = Vec::new();
            for asset in document.assets {
                let object = store.put(&asset.bytes).map_err(rag_error)?;
                if seen_assets.insert(object.storage_key().to_string()) {
                    assets.push(NewAsset {
                        version_id: payload.version_id,
                        kind: asset.kind,
                        storage_key: object.storage_key().to_string(),
                        sha256: object.sha256().to_string(),
                        media_type: asset.media_type,
                        source_location: asset.location,
                    });
                }
            }
            if assets.len() != version.expected_asset_count as usize {
                if version.manifest_sealed {
                    return Err(HandlerError::permanent("asset_count_mismatch"));
                }
            }
            let records = chunks
                .iter()
                .map(|chunk| NewChunk {
                    id: chunk.id.clone(),
                    version_id: payload.version_id,
                    ordinal: chunk.ordinal,
                    text: chunk.text.clone(),
                    source_location: chunk.source_location.clone(),
                    content_sha256: chunk.content_sha256.clone(),
                    policy_version: policy.version.clone(),
                })
                .collect::<Vec<_>>();
            knowledge::seal_document_manifest(
                &mut connection,
                &workspace_id,
                payload.version_id,
                u32::try_from(assets.len())
                    .map_err(|_| HandlerError::permanent("asset_count_overflow"))?,
                u32::try_from(records.len())
                    .map_err(|_| HandlerError::permanent("chunk_count_overflow"))?,
            )
            .map_err(storage_error)?;
            knowledge::persist_parsed_content(
                &mut connection,
                &workspace_id,
                payload.version_id,
                &assets,
                &records,
            )
            .map_err(storage_error)?;
            let mut manifest = Sha256::new();
            manifest.update(payload.version_id.to_string().as_bytes());
            for asset in &assets {
                manifest.update(asset.sha256.as_bytes());
            }
            for chunk in &chunks {
                manifest.update(chunk.id.as_str().as_bytes());
                manifest.update(chunk.content_sha256.as_bytes());
            }
            Ok(format!("{:x}", manifest.finalize()))
        })
    }

    fn embed(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
        is_cancelled: CancellationCheck,
    ) -> MinerUProcessFuture<String> {
        let database = self.database.clone();
        let embeddings = Arc::clone(&self.embeddings);
        Box::pin(async move {
            let mut connection = open(&database)?;
            let version = version(&connection, &workspace_id, &payload)?;
            let profile_id = Uuid::parse_str(&version.embedding_profile_id)
                .map_err(|_| HandlerError::permanent("embedding_identity_invalid"))?;
            let remote = embeddings
                .load(
                    &workspace_id,
                    profile_id,
                    payload.embedding_profile_revision,
                    payload.embedding_secret_generation,
                )
                .map_err(embedding_error)?;
            let manifest = embed_version(
                &mut connection,
                EmbeddingIndexRequest {
                    workspace_id,
                    version_id: payload.version_id,
                    provider_profile_id: version.embedding_profile_id,
                    model_id: version.embedding_model_id,
                    dimension: version.embedding_dimension,
                    policy_version: version.chunk_policy_version,
                },
                remote.as_ref(),
                is_cancelled.as_ref(),
            )
            .await
            .map_err(embedding_error)?;
            Ok(manifest.sha256)
        })
    }

    fn index(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
    ) -> MinerUProcessFuture<String> {
        let database = self.database.clone();
        Box::pin(async move {
            let mut connection = open(&database)?;
            version(&connection, &workspace_id, &payload)?;
            let entries =
                knowledge::finalize_flat_index(&mut connection, &workspace_id, payload.version_id)
                    .map_err(storage_error)?;
            let mut manifest = Sha256::new();
            manifest.update(payload.version_id.to_string().as_bytes());
            for (chunk_id, vector_key) in entries {
                manifest.update(chunk_id.as_str().as_bytes());
                manifest.update(vector_key.as_bytes());
            }
            Ok(format!("{:x}", manifest.finalize()))
        })
    }

    fn activate(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
        finalization: TaskFinalization,
    ) -> MinerUProcessFuture<DocumentVersionId> {
        let database = self.database.clone();
        Box::pin(async move {
            let mut connection = open(&database)?;
            version(&connection, &workspace_id, &payload)?;
            knowledge::activate_document_version_for_task(
                &mut connection,
                &workspace_id,
                payload.document_id,
                payload.version_id,
                finalization.task_id(),
                finalization.attempt(),
                finalization.checkpoint_json(),
            )
            .map_err(storage_error)?;
            Ok(payload.version_id)
        })
    }
}

fn open(path: &PathBuf) -> Result<Connection, HandlerError> {
    let connection = Connection::open(path).map_err(sqlite_error)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(sqlite_error)?;
    Ok(connection)
}

fn version(
    connection: &Connection,
    workspace_id: &str,
    payload: &MinerUTaskPayload,
) -> Result<knowledge::DocumentVersionRecord, HandlerError> {
    let version = knowledge::get_document_version(connection, workspace_id, payload.version_id)
        .map_err(storage_error)?
        .ok_or_else(|| HandlerError::permanent("document_version_missing"))?;
    if version.document_id != payload.document_id {
        return Err(HandlerError::permanent("document_version_mismatch"));
    }
    Ok(version)
}

fn embedding_error(error: crate::rag::index::EmbeddingError) -> HandlerError {
    if error.retryable() {
        HandlerError::retryable(error.code())
    } else {
        HandlerError::permanent(error.code())
    }
}

fn rag_error(error: super::RagTaskError) -> HandlerError {
    HandlerError::permanent(error.code())
}

fn storage_error(error: String) -> HandlerError {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("database is locked") || normalized.contains("database is busy") {
        HandlerError::retryable("rag_storage_busy")
    } else {
        HandlerError::permanent("rag_storage_failed")
    }
}

fn sqlite_error(error: rusqlite::Error) -> HandlerError {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            HandlerError::retryable("rag_storage_busy")
        }
        _ => HandlerError::permanent("rag_database_unavailable"),
    }
}
