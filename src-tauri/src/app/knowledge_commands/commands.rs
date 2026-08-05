use super::logic::{self, LocalKnowledgeQueryRequest};
use crate::db::DbState;
use crate::rag::citation::{EvidencePack, ResolvedCitation};
use crate::rag::index::rebuild::IndexRebuildRequest;
use crate::rag::ingest::{DocumentImportRequest, DocumentImportResponse};
use crate::storage::repositories::knowledge::{
    DocumentVersionRecord, KnowledgeBaseDeleteImpact, KnowledgeBaseRecord, KnowledgeHealth,
    SourceDocumentRecord,
};
use crate::storage::secrets::SecretState;
use uuid::Uuid;

#[tauri::command]
pub fn list_knowledge_bases(db: tauri::State<DbState>) -> Result<Vec<KnowledgeBaseRecord>, String> {
    logic::list_knowledge_bases(db)
}

#[tauri::command]
pub fn create_knowledge_base(
    db: tauri::State<DbState>,
    name: String,
) -> Result<KnowledgeBaseRecord, String> {
    logic::create_knowledge_base(db, name)
}

#[tauri::command]
pub fn rename_knowledge_base(
    db: tauri::State<DbState>,
    id: String,
    name: String,
) -> Result<KnowledgeBaseRecord, String> {
    logic::rename_knowledge_base(db, id, name)
}

#[tauri::command]
pub fn preview_delete_knowledge_base(
    db: tauri::State<DbState>,
    id: String,
) -> Result<KnowledgeBaseDeleteImpact, String> {
    logic::preview_delete_knowledge_base(db, id)
}

#[tauri::command]
pub fn delete_knowledge_base_confirmed(
    db: tauri::State<DbState>,
    id: String,
) -> Result<(), String> {
    logic::delete_knowledge_base_confirmed(db, id)
}

#[tauri::command]
pub fn list_knowledge_documents(
    db: tauri::State<DbState>,
    knowledge_base_id: String,
) -> Result<Vec<SourceDocumentRecord>, String> {
    logic::list_knowledge_documents(db, knowledge_base_id)
}

#[tauri::command]
pub fn list_document_versions(
    db: tauri::State<DbState>,
    document_id: String,
) -> Result<Vec<DocumentVersionRecord>, String> {
    logic::list_document_versions(db, document_id)
}

#[tauri::command]
pub fn import_local_document(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
    request: DocumentImportRequest,
) -> Result<DocumentImportResponse, String> {
    logic::import_local_document(app, db, secrets, request)
}

#[tauri::command]
pub fn rebuild_knowledge_index(
    db: tauri::State<DbState>,
    request: IndexRebuildRequest,
) -> Result<Uuid, String> {
    logic::rebuild_knowledge_index(db, request)
}

#[tauri::command]
pub fn resolve_knowledge_citation(
    db: tauri::State<DbState>,
    audit_id: String,
    citation_number: u32,
) -> Result<Option<ResolvedCitation>, String> {
    logic::resolve_knowledge_citation(db, audit_id, citation_number)
}

#[tauri::command]
pub fn get_knowledge_health(db: tauri::State<DbState>) -> Result<KnowledgeHealth, String> {
    logic::get_knowledge_health(db)
}

#[tauri::command]
pub async fn query_local_knowledge(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    request: LocalKnowledgeQueryRequest,
) -> Result<EvidencePack, String> {
    logic::query_local_knowledge(app, db, secrets, request).await
}
