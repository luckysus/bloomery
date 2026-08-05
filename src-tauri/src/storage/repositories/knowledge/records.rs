use crate::rag::model::{
    DocumentVersionId, IngestAttemptId, IngestAttemptState, KnowledgeBaseId, SourceDocumentId,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnowledgeBaseRecord {
    pub id: KnowledgeBaseId,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceDocumentRecord {
    pub id: SourceDocumentId,
    pub knowledge_base_id: KnowledgeBaseId,
    pub display_name: String,
    pub source_kind: String,
    pub active_version_id: Option<DocumentVersionId>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentVersionRecord {
    pub id: DocumentVersionId,
    pub document_id: SourceDocumentId,
    pub content_sha256: String,
    pub mime_type: String,
    pub parser: String,
    pub parser_version: String,
    pub chunk_policy_version: String,
    pub embedding_profile_id: String,
    pub embedding_model_id: String,
    pub embedding_dimension: u32,
    pub expected_asset_count: u32,
    pub expected_chunk_count: u32,
    pub manifest_sealed: bool,
    pub created_at: String,
    pub activated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IngestAttemptRecord {
    pub id: IngestAttemptId,
    pub document_id: SourceDocumentId,
    pub version_id: Option<DocumentVersionId>,
    pub task_id: Option<String>,
    pub state: IngestAttemptState,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}
