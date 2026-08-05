// Local knowledge command domain logic.
use crate::db::{current_workspace_id, database_path, with_conn, with_conn_mut, DbState};
use crate::providers::capabilities::{EmbeddingProvider, RerankProvider};
use crate::providers::profiles::{ProviderCapability, ProviderKind, ProviderProfileRecord};
use crate::providers::siliconflow::{SiliconFlowPlan, SiliconFlowProvider};
use crate::rag::citation::{
    persist_evidence_pack, resolve_citation, EvidencePack, ResolvedCitation,
    RetrievalConfigSnapshot,
};
use crate::rag::index::lifecycle::open_with_flat_fallback;
use crate::rag::index::rebuild::{
    index_root, load_index_snapshot, queue_index_rebuild, IndexRebuildRequest,
};
use crate::rag::ingest::{queue_document_import, DocumentImportRequest, DocumentImportResponse};
use crate::rag::model::{KnowledgeBaseId, SourceDocumentId};
use crate::rag::rerank::{rerank_candidates, RerankDegradationReason, RerankProviderState};
use crate::rag::retrieve::{retrieve, HybridSearchRequest};
use crate::storage::repositories::knowledge::{
    self, DocumentVersionRecord, KnowledgeBaseDeleteImpact, KnowledgeBaseRecord, KnowledgeHealth,
    SourceDocumentRecord,
};
use crate::storage::repositories::provider_profiles;
use crate::storage::secrets::{SecretRef, SecretState, SecretStore, SecretValue};
use rusqlite::Connection;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const DEFAULT_LEXICAL_LIMIT: usize = 40;
const DEFAULT_DENSE_LIMIT: usize = 40;
const DEFAULT_CANDIDATE_LIMIT: usize = 20;
const DEFAULT_RRF_K: u32 = 60;
const DEFAULT_RERANK_LIMIT: usize = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct LocalKnowledgeQueryRequest {
    pub query: String,
    pub knowledge_base_ids: Vec<KnowledgeBaseId>,
    #[serde(default = "default_lexical_limit")]
    pub lexical_limit: usize,
    #[serde(default = "default_dense_limit")]
    pub dense_limit: usize,
    #[serde(default = "default_candidate_limit")]
    pub candidate_limit: usize,
    #[serde(default = "default_rrf_k")]
    pub rrf_k: u32,
    #[serde(default = "default_rerank_limit")]
    pub rerank_limit: usize,
}

impl LocalKnowledgeQueryRequest {
    fn validate(&mut self) -> Result<(), String> {
        self.query = self.query.trim().to_string();
        if self.query.is_empty() {
            return Err("knowledge query is required".to_string());
        }
        self.knowledge_base_ids.sort_by_key(ToString::to_string);
        self.knowledge_base_ids.dedup();
        if self.knowledge_base_ids.is_empty() || self.knowledge_base_ids.len() > 128 {
            return Err("select between 1 and 128 knowledge bases".to_string());
        }
        if self.lexical_limit > 500
            || self.dense_limit > 500
            || self.candidate_limit == 0
            || self.candidate_limit > 500
            || (self.lexical_limit == 0 && self.dense_limit == 0)
        {
            return Err("knowledge retrieval limits are invalid".to_string());
        }
        Ok(())
    }
}

struct PreparedReranker {
    profile_id: Option<String>,
    model_id: Option<String>,
    provider: Option<Arc<SiliconFlowProvider>>,
    unavailable: Option<RerankDegradationReason>,
}

impl PreparedReranker {
    fn state(&self) -> RerankProviderState<'_> {
        if let Some(provider) = &self.provider {
            RerankProviderState::Ready(provider)
        } else if let Some(reason) = self.unavailable {
            RerankProviderState::Unavailable(reason)
        } else {
            RerankProviderState::Disabled
        }
    }
}

pub(crate) fn list_knowledge_bases(
    db: tauri::State<DbState>,
) -> Result<Vec<KnowledgeBaseRecord>, String> {
    with_conn(&db, |connection| {
        knowledge::list_knowledge_bases(connection, current_workspace_id())
    })
}

pub(crate) fn create_knowledge_base(
    db: tauri::State<DbState>,
    name: String,
) -> Result<KnowledgeBaseRecord, String> {
    with_conn(&db, |connection| {
        knowledge::create_knowledge_base(connection, current_workspace_id(), &name)
    })
}

pub(crate) fn rename_knowledge_base(
    db: tauri::State<DbState>,
    id: String,
    name: String,
) -> Result<KnowledgeBaseRecord, String> {
    let id = KnowledgeBaseId::from_str(&id).map_err(|error| error.to_string())?;
    with_conn(&db, |connection| {
        knowledge::rename_knowledge_base(connection, current_workspace_id(), id, &name)
    })
}

pub(crate) fn preview_delete_knowledge_base(
    db: tauri::State<DbState>,
    id: String,
) -> Result<KnowledgeBaseDeleteImpact, String> {
    let id = KnowledgeBaseId::from_str(&id).map_err(|error| error.to_string())?;
    with_conn(&db, |connection| {
        knowledge::preview_delete_knowledge_base(connection, current_workspace_id(), id)
    })
}

pub(crate) fn delete_knowledge_base_confirmed(
    db: tauri::State<DbState>,
    id: String,
) -> Result<(), String> {
    let id = KnowledgeBaseId::from_str(&id).map_err(|error| error.to_string())?;
    with_conn_mut(&db, |connection| {
        knowledge::delete_knowledge_base_confirmed(connection, current_workspace_id(), id)
    })
}

pub(crate) fn list_knowledge_documents(
    db: tauri::State<DbState>,
    knowledge_base_id: String,
) -> Result<Vec<SourceDocumentRecord>, String> {
    let id = KnowledgeBaseId::from_str(&knowledge_base_id).map_err(|error| error.to_string())?;
    with_conn(&db, |connection| {
        knowledge::list_source_documents(connection, current_workspace_id(), id)
    })
}

pub(crate) fn list_document_versions(
    db: tauri::State<DbState>,
    document_id: String,
) -> Result<Vec<DocumentVersionRecord>, String> {
    let id = SourceDocumentId::from_str(&document_id).map_err(|error| error.to_string())?;
    with_conn(&db, |connection| {
        knowledge::list_document_versions(connection, current_workspace_id(), id)
    })
}

pub(crate) fn import_local_document(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
    request: DocumentImportRequest,
) -> Result<DocumentImportResponse, String> {
    let content_root = database_path(&app)?
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "resolve RAG content root failed".to_string())?;
    with_conn_mut(&db, |connection| {
        queue_document_import(
            connection,
            current_workspace_id(),
            secrets.store(),
            &content_root,
            request,
        )
    })
}

pub(crate) fn rebuild_knowledge_index(
    db: tauri::State<DbState>,
    request: IndexRebuildRequest,
) -> Result<Uuid, String> {
    with_conn_mut(&db, |connection| {
        queue_index_rebuild(connection, current_workspace_id(), request)
    })
}

pub(crate) fn resolve_knowledge_citation(
    db: tauri::State<DbState>,
    audit_id: String,
    citation_number: u32,
) -> Result<Option<ResolvedCitation>, String> {
    let audit_id = Uuid::parse_str(&audit_id).map_err(|error| error.to_string())?;
    with_conn(&db, |connection| {
        resolve_citation(
            connection,
            current_workspace_id(),
            audit_id,
            citation_number,
        )
        .map_err(|error| error.to_string())
    })
}

pub(crate) fn get_knowledge_health(db: tauri::State<DbState>) -> Result<KnowledgeHealth, String> {
    with_conn(&db, |connection| {
        knowledge::read_knowledge_health(connection, current_workspace_id())
    })
}

pub(crate) async fn query_local_knowledge(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    mut request: LocalKnowledgeQueryRequest,
) -> Result<EvidencePack, String> {
    request.validate()?;
    let (embedding_record, rerank_record) = with_conn(&db, |connection| {
        let embedding = provider_profiles::get_default_record(
            connection,
            current_workspace_id(),
            ProviderCapability::Embedding,
        )?
        .ok_or_else(|| "default embedding provider is not configured".to_string())?;
        let rerank = provider_profiles::get_default_record(
            connection,
            current_workspace_id(),
            ProviderCapability::Rerank,
        )?;
        Ok((embedding, rerank))
    })?;
    let embedding_provider = prepare_embedding_provider(&embedding_record, secrets.store())?;
    let reranker = prepare_reranker(rerank_record, secrets.store());
    let embedding = embedding_provider
        .embed(vec![request.query.clone()])
        .await
        .map_err(|error| error.to_string())?;
    if embedding.vectors.len() != 1
        || embedding.vectors[0].is_empty()
        || embedding.vectors[0].iter().any(|value| !value.is_finite())
    {
        return Err("embedding provider returned an invalid query vector".to_string());
    }
    let query_vector = embedding.vectors.into_iter().next().expect("one vector");
    let dimension = u32::try_from(query_vector.len())
        .map_err(|_| "query embedding dimension is too large".to_string())?;
    let index_request = IndexRebuildRequest {
        provider_profile_id: embedding_record.profile.id.to_string(),
        model_id: embedding.model_id.clone(),
        dimension,
    };
    let path = database_path(&app)?;
    let content_root = path
        .parent()
        .ok_or_else(|| "resolve RAG content root failed".to_string())?;
    let connection = open_query_connection(&path)?;
    let snapshot = load_index_snapshot(&connection, current_workspace_id(), &index_request)?;
    let index = open_with_flat_fallback(
        &connection,
        &index_root(content_root, &snapshot.watermark),
        &snapshot.watermark,
    )
    .map_err(|error| error.to_string())?;
    let chunks = retrieve(
        &connection,
        &index,
        &HybridSearchRequest {
            workspace_id: current_workspace_id().to_string(),
            query: request.query.clone(),
            query_vector,
            knowledge_base_ids: request.knowledge_base_ids.clone(),
            lexical_limit: request.lexical_limit,
            dense_limit: request.dense_limit,
            candidate_limit: request.candidate_limit,
            rrf_k: request.rrf_k,
        },
    )
    .map_err(|error| error.to_string())?;
    drop(index);
    drop(connection);

    let reranked = rerank_candidates(
        &request.query,
        chunks,
        reranker.state(),
        request.rerank_limit,
        &|| false,
    )
    .await;
    let connection = open_query_connection(&path)?;
    persist_evidence_pack(
        &connection,
        current_workspace_id(),
        &request.query,
        RetrievalConfigSnapshot {
            knowledge_base_ids: request.knowledge_base_ids,
            lexical_limit: request.lexical_limit,
            dense_limit: request.dense_limit,
            candidate_limit: request.candidate_limit,
            rrf_k: request.rrf_k,
            embedding_provider_profile_id: embedding_record.profile.id.to_string(),
            embedding_model_id: embedding.model_id,
            rerank_provider_profile_id: reranker.profile_id,
            rerank_model_id: reranker.model_id,
            rerank_degradation: reranked.degradation,
        },
        reranked.chunks,
    )
    .map_err(|error| error.to_string())
}

fn prepare_embedding_provider(
    record: &ProviderProfileRecord,
    secrets: &dyn SecretStore,
) -> Result<SiliconFlowProvider, String> {
    if record.profile.kind != ProviderKind::SiliconFlow {
        return Err("configured embedding provider is not supported".to_string());
    }
    let model = record
        .profile
        .model_id
        .clone()
        .ok_or_else(|| "embedding model is not configured".to_string())?;
    SiliconFlowProvider::with_models(
        record.profile.clone(),
        Some(read_credential(record, secrets)?),
        SiliconFlowPlan::Free,
        Some(model),
        None,
    )
    .map_err(|error| error.to_string())
}

fn prepare_reranker(
    record: Option<ProviderProfileRecord>,
    secrets: &dyn SecretStore,
) -> PreparedReranker {
    let Some(record) = record else {
        return PreparedReranker {
            profile_id: None,
            model_id: None,
            provider: None,
            unavailable: None,
        };
    };
    let profile_id = Some(record.profile.id.to_string());
    if record.profile.kind != ProviderKind::SiliconFlow {
        return PreparedReranker {
            profile_id,
            model_id: record.profile.model_id,
            provider: None,
            unavailable: Some(RerankDegradationReason::UnsupportedCapability),
        };
    }
    let credential = match read_credential(&record, secrets) {
        Ok(value) => value,
        Err(_) => {
            return PreparedReranker {
                profile_id,
                model_id: record.profile.model_id,
                provider: None,
                unavailable: Some(RerankDegradationReason::MissingCredential),
            };
        }
    };
    match SiliconFlowProvider::with_models(
        record.profile.clone(),
        Some(credential),
        SiliconFlowPlan::Free,
        None,
        record.profile.model_id.clone(),
    ) {
        Ok(provider) => PreparedReranker {
            profile_id,
            model_id: Some(RerankProvider::capabilities(&provider).model_id.clone()),
            provider: Some(Arc::new(provider)),
            unavailable: None,
        },
        Err(_) => PreparedReranker {
            profile_id,
            model_id: record.profile.model_id,
            provider: None,
            unavailable: Some(RerankDegradationReason::InvalidConfiguration),
        },
    }
}

fn read_credential(
    record: &ProviderProfileRecord,
    secrets: &dyn SecretStore,
) -> Result<SecretValue, String> {
    let name = record
        .profile
        .secret_ref
        .as_deref()
        .ok_or_else(|| "provider credential is not configured".to_string())?;
    let reference = SecretRef::at_generation(record.profile.id, name, record.secret_generation)
        .map_err(|error| error.to_string())?;
    secrets.get(&reference).map_err(|error| error.to_string())
}

fn open_query_connection(path: &std::path::Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

const fn default_lexical_limit() -> usize {
    DEFAULT_LEXICAL_LIMIT
}

const fn default_dense_limit() -> usize {
    DEFAULT_DENSE_LIMIT
}

const fn default_candidate_limit() -> usize {
    DEFAULT_CANDIDATE_LIMIT
}

const fn default_rrf_k() -> u32 {
    DEFAULT_RRF_K
}

const fn default_rerank_limit() -> usize {
    DEFAULT_RERANK_LIMIT
}
