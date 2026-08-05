pub mod lifecycle;
mod lifecycle_io;
pub mod rebuild;
pub mod repair;
pub mod vector;

use crate::providers::capabilities::EmbeddingResponse;
use crate::providers::http::{ProviderError, ProviderErrorCode};
use crate::rag::model::{ChunkId, DocumentVersionId, EmbeddingIdentity, EmbeddingVectorBatch};
use crate::storage::repositories::knowledge;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

pub type EmbeddingRemoteFuture =
    Pin<Box<dyn Future<Output = Result<EmbeddingResponse, ProviderError>> + Send + 'static>>;

pub trait EmbeddingRemote: Send + Sync {
    fn model_id(&self) -> &str;
    fn max_batch_size(&self) -> usize;
    fn embed(&self, inputs: Vec<String>) -> EmbeddingRemoteFuture;
}

pub trait EmbeddingRemoteFactory: Send + Sync {
    fn load(
        &self,
        workspace_id: &str,
        profile_id: Uuid,
        expected_revision: u64,
        expected_secret_generation: u64,
    ) -> Result<Arc<dyn EmbeddingRemote>, EmbeddingError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingIndexRequest {
    pub workspace_id: String,
    pub version_id: DocumentVersionId,
    pub provider_profile_id: String,
    pub model_id: String,
    pub dimension: u32,
    pub policy_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingManifest {
    pub chunk_count: u32,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingError {
    code: &'static str,
    message: String,
    retryable: bool,
}

impl EmbeddingError {
    pub(crate) fn permanent(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    pub(crate) fn transient(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: true,
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }

    pub const fn retryable(&self) -> bool {
        self.retryable
    }
}

impl fmt::Display for EmbeddingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for EmbeddingError {}

struct PendingGroup {
    normalized_text: String,
    identity: EmbeddingIdentity,
    chunk_ids: Vec<ChunkId>,
}

pub async fn embed_version(
    connection: &mut Connection,
    request: EmbeddingIndexRequest,
    remote: &dyn EmbeddingRemote,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<EmbeddingManifest, EmbeddingError> {
    validate_request(connection, &request, remote)?;
    let chunks =
        knowledge::list_chunks_for_embedding(connection, &request.workspace_id, request.version_id)
            .map_err(storage)?;
    let base_identity = identity(&request, String::new());
    let linked = knowledge::linked_embedding_chunk_ids(
        connection,
        &request.workspace_id,
        request.version_id,
        &base_identity,
    )
    .map_err(storage)?
    .into_iter()
    .collect::<HashSet<_>>();

    let mut groups: Vec<PendingGroup> = Vec::new();
    for chunk in &chunks {
        if linked.contains(&chunk.id) {
            continue;
        }
        let normalized_text = normalize(&chunk.text);
        if normalized_text.is_empty() {
            return Err(EmbeddingError::permanent(
                "embedding_text_empty",
                "normalized chunk text is empty",
            ));
        }
        let text_hash = digest(normalized_text.as_bytes());
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.identity.normalized_text_sha256 == text_hash)
        {
            group.chunk_ids.push(chunk.id.clone());
        } else {
            groups.push(PendingGroup {
                normalized_text,
                identity: identity(&request, text_hash),
                chunk_ids: vec![chunk.id.clone()],
            });
        }
    }

    let mut missing = Vec::new();
    for group in groups {
        cancelled(is_cancelled)?;
        match knowledge::find_reusable_vector(connection, &request.workspace_id, &group.identity)
            .map_err(storage)?
        {
            Some(vector_key) => knowledge::persist_reused_embedding_links(
                connection,
                &request.workspace_id,
                request.version_id,
                &group.identity,
                &vector_key,
                &group.chunk_ids,
            )
            .map_err(storage)?,
            None => missing.push(group),
        }
    }

    let batch_size = remote.max_batch_size();
    if batch_size == 0 {
        return Err(EmbeddingError::permanent(
            "embedding_provider_limit_invalid",
            "embedding provider batch size must be positive",
        ));
    }
    for batch in missing.chunks(batch_size) {
        cancelled(is_cancelled)?;
        let inputs = batch
            .iter()
            .map(|group| group.normalized_text.clone())
            .collect::<Vec<_>>();
        let response = remote.embed(inputs).await.map_err(provider)?;
        validate_response(&request, batch.len(), &response)?;
        let vectors = batch
            .iter()
            .zip(response.vectors)
            .map(|(group, vector)| vector_batch(group, vector))
            .collect::<Result<Vec<_>, _>>()?;
        knowledge::persist_embedding_batch(
            connection,
            &request.workspace_id,
            request.version_id,
            &vectors,
        )
        .map_err(storage)?;
    }

    let completed = knowledge::linked_embedding_chunk_ids(
        connection,
        &request.workspace_id,
        request.version_id,
        &base_identity,
    )
    .map_err(storage)?
    .into_iter()
    .collect::<HashSet<_>>();
    if completed.len() != chunks.len() || chunks.iter().any(|chunk| !completed.contains(&chunk.id))
    {
        return Err(EmbeddingError::permanent(
            "embedding_incomplete",
            "not every chunk has an exact embedding link",
        ));
    }
    let mut manifest = Sha256::new();
    manifest.update(request.version_id.to_string().as_bytes());
    for chunk in &chunks {
        manifest.update(chunk.ordinal.to_le_bytes());
        manifest.update(chunk.id.as_str().as_bytes());
    }
    Ok(EmbeddingManifest {
        chunk_count: u32::try_from(chunks.len()).map_err(|_| {
            EmbeddingError::permanent("too_many_chunks", "embedding manifest is too large")
        })?,
        sha256: format!("{:x}", manifest.finalize()),
    })
}

fn validate_request(
    connection: &Connection,
    request: &EmbeddingIndexRequest,
    remote: &dyn EmbeddingRemote,
) -> Result<(), EmbeddingError> {
    Uuid::parse_str(&request.provider_profile_id).map_err(|_| {
        EmbeddingError::permanent(
            "embedding_identity_invalid",
            "provider profile ID is invalid",
        )
    })?;
    let version =
        knowledge::get_document_version(connection, &request.workspace_id, request.version_id)
            .map_err(storage)?
            .ok_or_else(|| {
                EmbeddingError::permanent(
                    "embedding_version_missing",
                    "document version was not found",
                )
            })?;
    if (
        request.provider_profile_id.as_str(),
        request.model_id.as_str(),
        request.dimension,
        request.policy_version.as_str(),
    ) != (
        version.embedding_profile_id.as_str(),
        version.embedding_model_id.as_str(),
        version.embedding_dimension,
        version.chunk_policy_version.as_str(),
    ) {
        return Err(EmbeddingError::permanent(
            "embedding_identity_mismatch",
            "embedding request does not match the immutable document version",
        ));
    }
    if remote.model_id() != request.model_id {
        return Err(EmbeddingError::permanent(
            "embedding_model_mismatch",
            "embedding provider model does not match the document version",
        ));
    }
    Ok(())
}

fn validate_response(
    request: &EmbeddingIndexRequest,
    expected_count: usize,
    response: &EmbeddingResponse,
) -> Result<(), EmbeddingError> {
    if response.model_id != request.model_id {
        return Err(EmbeddingError::permanent(
            "embedding_model_mismatch",
            "embedding response model does not match the request",
        ));
    }
    if response.vectors.len() != expected_count {
        return Err(EmbeddingError::permanent(
            "embedding_count_mismatch",
            "embedding response count does not match the request",
        ));
    }
    for vector in &response.vectors {
        if vector.len() != request.dimension as usize {
            return Err(EmbeddingError::permanent(
                "embedding_dimension_mismatch",
                "embedding vector dimension does not match the document version",
            ));
        }
        if vector.iter().any(|value| !value.is_finite()) {
            return Err(EmbeddingError::permanent(
                "embedding_value_invalid",
                "embedding vector contains a non-finite value",
            ));
        }
    }
    Ok(())
}

fn vector_batch(
    group: &PendingGroup,
    vector: Vec<f32>,
) -> Result<EmbeddingVectorBatch, EmbeddingError> {
    let mut blob = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    let vector_sha256 = digest(&blob);
    let mut key = Sha256::new();
    key.update(group.identity.provider_profile_id.as_bytes());
    key.update(group.identity.model_id.as_bytes());
    key.update(group.identity.dimension.to_le_bytes());
    key.update(group.identity.normalized_text_sha256.as_bytes());
    key.update(group.identity.policy_version.as_bytes());
    Ok(EmbeddingVectorBatch {
        vector_key: format!("vector-{:x}", key.finalize()),
        identity: group.identity.clone(),
        vector_blob: blob,
        vector_sha256,
        chunk_ids: group.chunk_ids.clone(),
    })
}

fn identity(request: &EmbeddingIndexRequest, text_hash: String) -> EmbeddingIdentity {
    EmbeddingIdentity {
        provider_profile_id: request.provider_profile_id.clone(),
        model_id: request.model_id.clone(),
        dimension: request.dimension,
        normalized_text_sha256: text_hash,
        policy_version: request.policy_version.clone(),
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cancelled(is_cancelled: &(dyn Fn() -> bool + Send + Sync)) -> Result<(), EmbeddingError> {
    if is_cancelled() {
        Err(EmbeddingError::permanent(
            "embedding_cancelled",
            "embedding was cancelled",
        ))
    } else {
        Ok(())
    }
}

fn provider(error: ProviderError) -> EmbeddingError {
    let (code, retryable) = match error.code() {
        ProviderErrorCode::Network => ("embedding_network", true),
        ProviderErrorCode::Timeout => ("embedding_timeout", true),
        ProviderErrorCode::Quota => ("embedding_quota", true),
        ProviderErrorCode::Cancelled => ("embedding_cancelled", false),
        ProviderErrorCode::Authentication => ("embedding_authentication", false),
        ProviderErrorCode::UnsupportedCapability => ("embedding_unsupported", false),
        ProviderErrorCode::ProviderResponse
            if error.status().is_some_and(|status| status >= 500) =>
        {
            ("embedding_provider_response", true)
        }
        ProviderErrorCode::ProviderResponse => ("embedding_provider_response", false),
    };
    EmbeddingError {
        code,
        message: error.to_string(),
        retryable,
    }
}

fn storage(error: String) -> EmbeddingError {
    let retryable = {
        let normalized = error.to_ascii_lowercase();
        normalized.contains("database is locked") || normalized.contains("database is busy")
    };
    if retryable {
        EmbeddingError::transient("embedding_storage", error)
    } else {
        EmbeddingError::permanent("embedding_storage", error)
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub mod fts;
#[cfg(test)]
mod tests;
