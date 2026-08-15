use super::{ingest_file, IngestLimits};
use crate::providers::profiles::{ProviderKind, ProviderProfileRecord};
use crate::rag::chunk::ChunkPolicy;
use crate::rag::model::{
    DocumentVersionId, IngestAttemptId, KnowledgeBaseId, NewDocumentVersion, NewSourceDocument,
    SourceDocumentId,
};
use crate::rag::tasks::{MinerUCheckpoint, MinerUTaskPayload, StoredObjectRef, MINERU_TASK_KIND};
use crate::storage::repositories::{knowledge, provider_profiles};
use crate::storage::secrets::{status, SecretRef, SecretStore};
use crate::tasks::{repository as task_repository, NewTask};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum KnowledgeBaseTarget {
    Existing { id: KnowledgeBaseId },
    Create { name: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct DocumentImportRequest {
    pub source_path: PathBuf,
    pub knowledge_base: KnowledgeBaseTarget,
    pub mineru_profile_id: Option<Uuid>,
    pub embedding_profile_id: Uuid,
    pub embedding_dimension: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentImportResponse {
    pub knowledge_base_id: KnowledgeBaseId,
    pub document_id: SourceDocumentId,
    pub version_id: DocumentVersionId,
    pub ingest_attempt_id: IngestAttemptId,
    pub task_id: Uuid,
    pub duplicate_content: bool,
}

pub fn queue_document_import(
    connection: &mut Connection,
    workspace_id: &str,
    secrets: &dyn SecretStore,
    content_root: &Path,
    request: DocumentImportRequest,
) -> Result<DocumentImportResponse, String> {
    if request.embedding_dimension == 0 {
        return Err("embedding dimension must be positive".to_string());
    }
    let source = ingest_file(&request.source_path, content_root, IngestLimits::default())
        .map_err(|error| error.to_string())?;
    let display_name = request
        .source_path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .ok_or_else(|| "source file name is required".to_string())?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let mineru = request
        .mineru_profile_id
        .map(|id| {
            provider(
                &transaction,
                workspace_id,
                secrets,
                id,
                ProviderKind::MinerU,
            )
        })
        .transpose()?;
    let embedding = provider(
        &transaction,
        workspace_id,
        secrets,
        request.embedding_profile_id,
        ProviderKind::SiliconFlow,
    )?;
    let embedding_model = embedding
        .profile
        .model_id
        .clone()
        .ok_or_else(|| "embedding provider model is required".to_string())?;
    let knowledge_base = match request.knowledge_base {
        KnowledgeBaseTarget::Existing { id } => {
            knowledge::get_knowledge_base(&transaction, workspace_id, id)?
                .ok_or_else(|| "knowledge base not found".to_string())?
        }
        KnowledgeBaseTarget::Create { name } => {
            knowledge::create_knowledge_base(&transaction, workspace_id, &name)?
        }
    };
    let document = knowledge::create_source_document(
        &transaction,
        workspace_id,
        NewSourceDocument {
            knowledge_base_id: knowledge_base.id,
            display_name: display_name.clone(),
            source_kind: source.format.as_str().to_string(),
        },
    )?;
    let version = knowledge::create_pending_document_version(
        &transaction,
        workspace_id,
        NewDocumentVersion {
            document_id: document.id,
            content_sha256: source.content_sha256.clone(),
            mime_type: source.mime_type.clone(),
            parser: if mineru.is_some() {
                "mineru".to_string()
            } else {
                "local".to_string()
            },
            parser_version: if mineru.is_some() {
                "v4".to_string()
            } else {
                "v1".to_string()
            },
            chunk_policy_version: ChunkPolicy::default().version,
            embedding_profile_id: embedding.profile.id.to_string(),
            embedding_model_id: embedding_model,
            embedding_dimension: request.embedding_dimension,
            expected_asset_count: 0,
            expected_chunk_count: 0,
        },
    )?;
    let object = StoredObjectRef::new(source.content_sha256, source.storage_key)
        .map_err(|error| error.to_string())?;
    let payload = MinerUTaskPayload {
        document_id: document.id,
        version_id: version.id,
        provider_profile_id: mineru.as_ref().map(|value| value.profile.id.to_string()),
        provider_profile_revision: mineru.as_ref().map_or(0, |value| value.revision),
        provider_secret_generation: mineru.as_ref().map_or(0, |value| value.secret_generation),
        embedding_profile_revision: embedding.revision,
        embedding_secret_generation: embedding.secret_generation,
        source: object.clone(),
        file_name: display_name,
        mime_type: source.mime_type,
    };
    payload.validate().map_err(|error| error.to_string())?;
    let checkpoint = MinerUCheckpoint::source_stored(object);
    let checkpoint_json = serde_json::to_string(&checkpoint).map_err(|error| error.to_string())?;
    let task = task_repository::create(
        &transaction,
        NewTask {
            workspace_id: workspace_id.to_string(),
            kind: MINERU_TASK_KIND.to_string(),
            payload_json: serde_json::to_string(&payload).map_err(|error| error.to_string())?,
            checkpoint_json: Some(checkpoint_json),
            next_run_at: None,
            progress: checkpoint.progress(),
        },
    )
    .map_err(|error| error.to_string())?;
    let attempt = knowledge::create_ingest_attempt(
        &transaction,
        workspace_id,
        document.id,
        Some(version.id),
        Some(task.id.to_string()),
    )?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(DocumentImportResponse {
        knowledge_base_id: knowledge_base.id,
        document_id: document.id,
        version_id: version.id,
        ingest_attempt_id: attempt.id,
        task_id: task.id,
        duplicate_content: source.duplicate,
    })
}

fn provider(
    connection: &Connection,
    workspace_id: &str,
    secrets: &dyn SecretStore,
    id: Uuid,
    expected_kind: ProviderKind,
) -> Result<ProviderProfileRecord, String> {
    let record = provider_profiles::get_record(connection, workspace_id, id)?
        .ok_or_else(|| "provider profile not found".to_string())?;
    if record.profile.kind != expected_kind {
        return Err("provider profile kind mismatch".to_string());
    }
    if !record.profile.enabled {
        return Err("provider profile is disabled".to_string());
    }
    let secret_name = record
        .profile
        .secret_ref
        .as_deref()
        .ok_or_else(|| "provider credential is not configured".to_string())?;
    let reference =
        SecretRef::at_generation(record.profile.id, secret_name, record.secret_generation)
            .map_err(|error| error.to_string())?;
    if !status(secrets, &reference)
        .map_err(|error| error.to_string())?
        .configured
    {
        return Err("provider credential is not configured".to_string());
    }
    Ok(record)
}
