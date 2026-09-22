// Local knowledge command domain logic.
use crate::db::{current_workspace_id, database_path, with_conn, with_conn_mut, DbState};
use crate::knowledge_db::KnowledgeDatabaseState;
use crate::providers::capabilities::{EmbeddingProvider, RerankProvider};
use crate::providers::http::{ProviderError, ProviderErrorCode};
use crate::providers::profiles::{ProviderCapability, ProviderKind, ProviderProfileRecord};
use crate::providers::{
    configured_embedding_provider, configured_rerank_provider, ConfiguredEmbeddingProvider,
    ConfiguredRerankProvider, SiliconFlowPlan,
};
use crate::rag::citation::{
    persist_evidence_pack, resolve_citation, EvidencePack, ResolvedCitation,
    RetrievalConfigSnapshot,
};
use crate::rag::index::fts::{search as search_fts, FtsHit, FtsSearchRequest};
use crate::rag::index::lifecycle::open_with_flat_fallback;
use crate::rag::index::rebuild::{
    index_root, load_index_snapshot, queue_index_rebuild, IndexRebuildRequest,
};
use crate::rag::ingest::{
    queue_document_import, DocumentImportRequest, DocumentImportResponse, SourceFormat,
};
use crate::rag::model::{KnowledgeBaseId, SourceDocumentId};
use crate::rag::parse::{parse_document, DocumentBlock, ParseLimits};
use crate::rag::rerank::{rerank_candidates, RerankDegradationReason, RerankProviderState};
use crate::rag::retrieve::{retrieve, HybridSearchRequest, RetrievedChunk};
use crate::storage::repositories::domains;
use crate::storage::repositories::knowledge::{
    self, DocumentVersionRecord, KnowledgeBaseDeleteImpact, KnowledgeBaseRecord, KnowledgeHealth,
    SourceDocumentRecord,
};
use crate::storage::repositories::{provider_profiles, settings};
use crate::storage::secrets::{SecretRef, SecretState, SecretStore, SecretValue};
use rusqlite::Connection;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Row;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const DEFAULT_LEXICAL_LIMIT: usize = 40;
const DEFAULT_DENSE_LIMIT: usize = 40;
const DEFAULT_CANDIDATE_LIMIT: usize = 20;
const DEFAULT_RRF_K: u32 = 60;
const DEFAULT_RERANK_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgePreviewBlock {
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgePreviewSheet {
    pub name: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeDocumentPreview {
    pub processed: bool,
    pub source: Option<String>,
    pub content: Option<String>,
    pub blocks: Vec<KnowledgePreviewBlock>,
    pub raw_data_url: Option<String>,
    pub raw_sheets: Vec<KnowledgePreviewSheet>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeDocumentRaw {
    pub data_url: String,
    pub sheets: Vec<KnowledgePreviewSheet>,
}

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
    provider: Option<Arc<ConfiguredRerankProvider>>,
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

pub(crate) fn rename_knowledge_document(
    db: tauri::State<DbState>,
    id: String,
    display_name: String,
) -> Result<SourceDocumentRecord, String> {
    let id = SourceDocumentId::from_str(&id).map_err(|error| error.to_string())?;
    with_conn(&db, |connection| {
        knowledge::rename_knowledge_document(connection, current_workspace_id(), id, &display_name)
    })
}

pub(crate) fn delete_knowledge_document(
    db: tauri::State<DbState>,
    id: String,
) -> Result<(), String> {
    let id = SourceDocumentId::from_str(&id).map_err(|error| error.to_string())?;
    with_conn_mut(&db, |connection| {
        knowledge::delete_knowledge_document(connection, current_workspace_id(), id)
    })
}

pub(crate) fn merge_knowledge_bases(
    db: tauri::State<DbState>,
    request: knowledge::KnowledgeBaseMergeRequest,
) -> Result<KnowledgeBaseRecord, String> {
    with_conn_mut(&db, |connection| {
        knowledge::merge_knowledge_bases(connection, current_workspace_id(), request)
    })
}

pub(crate) fn get_knowledge_document_preview(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    document_id: String,
) -> Result<KnowledgeDocumentPreview, String> {
    let document_id =
        SourceDocumentId::from_str(&document_id).map_err(|error| error.to_string())?;
    with_conn(&db, |connection| {
        let document =
            knowledge::get_source_document(connection, current_workspace_id(), document_id)?
                .ok_or_else(|| "source document not found".to_string())?;
        let Some(version_id) = document.active_version_id else {
            return Ok(KnowledgeDocumentPreview {
                processed: false,
                source: None,
                content: None,
                blocks: Vec::new(),
                raw_data_url: None,
                raw_sheets: Vec::new(),
            });
        };
        let version =
            knowledge::get_document_version(connection, current_workspace_id(), version_id)?
                .ok_or_else(|| "document version not found".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT text FROM knowledge_chunks
                 WHERE workspace_id = ?1 AND version_id = ?2
                 ORDER BY ordinal, id",
            )
            .map_err(|error| error.to_string())?;
        let chunks = statement
            .query_map(
                rusqlite::params![current_workspace_id(), version_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let blocks = chunks
            .iter()
            .cloned()
            .map(|content| KnowledgePreviewBlock { content })
            .collect::<Vec<_>>();
        Ok(KnowledgeDocumentPreview {
            processed: true,
            source: Some(version.parser),
            content: (!chunks.is_empty()).then(|| chunks.join("\n\n")),
            blocks,
            raw_data_url: None,
            raw_sheets: Vec::new(),
        })
    })
    .map_err(|error| error.to_string())
    .and_then(|preview| {
        let _ = app;
        Ok(preview)
    })
}

pub(crate) fn get_knowledge_document_raw(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    document_id: String,
) -> Result<KnowledgeDocumentRaw, String> {
    let document_id =
        SourceDocumentId::from_str(&document_id).map_err(|error| error.to_string())?;
    let database = database_path(&app)?;
    let content_root = database
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "resolve RAG content root failed".to_string())?;
    with_conn(&db, |connection| {
        let document =
            knowledge::get_source_document(connection, current_workspace_id(), document_id)?
                .ok_or_else(|| "source document not found".to_string())?;
        let version_id = document
            .active_version_id
            .ok_or_else(|| "document is still processing".to_string())?;
        let version =
            knowledge::get_document_version(connection, current_workspace_id(), version_id)?
                .ok_or_else(|| "document version not found".to_string())?;
        let source_path = content_root.join(format!(
            "objects/sha256/{}/{}",
            &version.content_sha256[..2],
            version.content_sha256
        ));
        let bytes = std::fs::read(&source_path)
            .map_err(|error| format!("read source document failed: {error}"))?;
        let sheets = SourceFormat::from_mime_type(&version.mime_type)
            .filter(|format| matches!(format, SourceFormat::Xlsx))
            .map(|format| {
                parse_document(&source_path, format, ParseLimits::default())
                    .map(|parsed| render_sheets(&parsed))
                    .map_err(|error| error.to_string())
            })
            .transpose()?
            .unwrap_or_default();
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        Ok(KnowledgeDocumentRaw {
            data_url: format!(
                "data:{};base64,{}",
                version.mime_type,
                STANDARD.encode(bytes)
            ),
            sheets,
        })
    })
}

fn render_sheets(document: &crate::rag::parse::ParsedDocument) -> Vec<KnowledgePreviewSheet> {
    let mut sheets = Vec::new();
    for block in &document.blocks {
        let DocumentBlock::Table {
            rows,
            location: crate::rag::model::SourceLocation::SheetRange { sheet, .. },
        } = block
        else {
            continue;
        };
        let body = rows
            .iter()
            .map(|row| {
                let cells = row
                    .iter()
                    .map(|cell| format!("<td>{}</td>", escape_html(cell)))
                    .collect::<String>();
                format!("<tr>{cells}</tr>")
            })
            .collect::<String>();
        let html = format!("<table><tbody>{body}</tbody></table>");
        if let Some(existing) = sheets
            .iter_mut()
            .find(|item: &&mut KnowledgePreviewSheet| item.name == *sheet)
        {
            existing.html.push_str(&html);
        } else {
            sheets.push(KnowledgePreviewSheet {
                name: sheet.clone(),
                html,
            });
        }
    }
    sheets
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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
    secrets: tauri::State<'_, SecretState>,
    postgres: tauri::State<'_, KnowledgeDatabaseState>,
    request: LocalKnowledgeQueryRequest,
) -> Result<EvidencePack, String> {
    if crate::knowledge_db::pool_for_query(&postgres).is_ok() {
        return query_postgres_knowledge(&app, &secrets, &postgres, request).await;
    }
    let path = database_path(&app)?;
    query_local_knowledge_from_path(path, current_workspace_id(), secrets.store(), request).await
}

async fn query_postgres_knowledge(
    app: &tauri::AppHandle,
    secrets: &tauri::State<'_, SecretState>,
    state: &KnowledgeDatabaseState,
    mut request: LocalKnowledgeQueryRequest,
) -> Result<EvidencePack, String> {
    request.validate()?;
    let pool = crate::knowledge_db::pool_for_query(state)?;
    if request.dense_limit > 0 {
        let database = database_path(app)?;
        let connection = open_query_connection(&database)?;
        let embedding_record = provider_profiles::get_default_record(
            &connection,
            current_workspace_id(),
            ProviderCapability::Embedding,
        )?;
        let rerank_record = provider_profiles::get_default_record(
            &connection,
            current_workspace_id(),
            ProviderCapability::Rerank,
        )?;
        let retrieval_plan = SiliconFlowPlan::from_setting(
            settings::get(&connection, current_workspace_id(), "onboarding.retrieval")?.as_deref(),
        );
        drop(connection);
        if let Some(record) = embedding_record {
            if let Ok(provider) =
                prepare_embedding_provider(&record, secrets.store(), retrieval_plan)
            {
                if let Ok(response) = provider.embed(vec![request.query.clone()]).await {
                    if let Some(vector) = response.vectors.into_iter().next() {
                        if !vector.is_empty() && vector.iter().all(|value| value.is_finite()) {
                            let mut hits = Vec::new();
                            for knowledge_base_id in &request.knowledge_base_ids {
                                let id = Uuid::parse_str(&knowledge_base_id.to_string())
                                    .map_err(|e| e.to_string())?;
                                hits.extend(
                                    crate::knowledge_db::search_postgres_hybrid_pool(
                                        &pool,
                                        id,
                                        &request.query,
                                        &vector,
                                        request.candidate_limit,
                                        request.rrf_k,
                                    )
                                    .await?,
                                );
                            }
                            hits.sort_by(|left, right| right.rank.total_cmp(&left.rank));
                            hits.truncate(request.candidate_limit);
                            let chunks = postgres_hits_to_chunks(hits)?;
                            let reranker =
                                prepare_reranker(rerank_record, secrets.store(), retrieval_plan);
                            let reranked = rerank_candidates(
                                &request.query,
                                chunks,
                                reranker.state(),
                                request.rerank_limit,
                                &|| false,
                            )
                            .await;
                            let connection = open_query_connection(&database_path(app)?)?;
                            return persist_evidence_pack(
                                &connection,
                                current_workspace_id(),
                                &request.query,
                                RetrievalConfigSnapshot {
                                    knowledge_base_ids: request.knowledge_base_ids,
                                    lexical_limit: request.lexical_limit,
                                    dense_limit: request.dense_limit,
                                    candidate_limit: request.candidate_limit,
                                    rrf_k: request.rrf_k,
                                    embedding_provider_profile_id: record.profile.id.to_string(),
                                    embedding_model_id: response.model_id,
                                    rerank_provider_profile_id: reranker.profile_id,
                                    rerank_model_id: reranker.model_id,
                                    rerank_degradation: reranked.degradation,
                                },
                                reranked.chunks,
                            )
                            .map_err(|error| error.to_string());
                        }
                    }
                }
            }
        }
    }
    query_postgres_knowledge_with_pool(app, pool, request).await
}

fn postgres_hits_to_chunks(
    hits: Vec<crate::knowledge_db::PostgresKnowledgeSearchHit>,
) -> Result<Vec<RetrievedChunk>, String> {
    hits.into_iter()
        .enumerate()
        .map(|(index, hit)| {
            Ok(RetrievedChunk {
                knowledge_base_id: KnowledgeBaseId::from_str(&hit.knowledge_base_id.to_string())
                    .map_err(|e| e.to_string())?,
                document_id: SourceDocumentId::from_str(&hit.document_id.to_string())
                    .map_err(|e| e.to_string())?,
                version_id: crate::rag::model::DocumentVersionId::from_str(
                    &hit.version_id.to_string(),
                )
                .map_err(|e| e.to_string())?,
                chunk_id: crate::rag::model::ChunkId::from_str(&hit.chunk_id.to_string())?,
                source_name: hit.document_name,
                source_location: serde_json::from_value(hit.source_location)
                    .map_err(|e| e.to_string())?,
                text: hit.text,
                lexical_rank: Some(index + 1),
                dense_rank: Some(index + 1),
                rrf_score: hit.rank as f64,
                rerank_score: None,
            })
        })
        .collect()
}

pub(crate) async fn query_postgres_knowledge_with_pool(
    app: &tauri::AppHandle,
    pool: sqlx::PgPool,
    mut request: LocalKnowledgeQueryRequest,
) -> Result<EvidencePack, String> {
    request.validate()?;
    let ids = request
        .knowledge_base_ids
        .iter()
        .map(|id| Uuid::parse_str(&id.to_string()).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let limit = request
        .candidate_limit
        .min(request.lexical_limit.max(1))
        .min(100);
    let rows = sqlx::query(
        "SELECT c.id, c.version_id, d.id AS document_id, d.knowledge_base_id,
                d.display_name, c.text, c.source_location,
                ts_rank_cd(c.search_vector, plainto_tsquery('simple', $1))::real AS rank
         FROM document_chunks c
         JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
         JOIN source_documents d ON d.id = v.document_id
         WHERE d.knowledge_base_id = ANY($2::uuid[]) AND d.deleted_at IS NULL
           AND c.search_vector @@ plainto_tsquery('simple', $1)
         ORDER BY rank DESC, c.ordinal
         LIMIT $3",
    )
    .bind(request.query.trim())
    .bind(&ids)
    .bind(i64::try_from(limit).map_err(|_| "检索数量无效")?)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("PostgreSQL 检索失败: {}", error))?;
    let mut chunks = Vec::with_capacity(rows.len());
    for (index, row) in rows.into_iter().enumerate() {
        let kb = KnowledgeBaseId::from_str(
            &row.try_get::<Uuid, _>("knowledge_base_id")
                .map_err(|e| e.to_string())?
                .to_string(),
        )
        .map_err(|e| e.to_string())?;
        let document_id = SourceDocumentId::from_str(
            &row.try_get::<Uuid, _>("document_id")
                .map_err(|e| e.to_string())?
                .to_string(),
        )
        .map_err(|e| e.to_string())?;
        let version_id = crate::rag::model::DocumentVersionId::from_str(
            &row.try_get::<Uuid, _>("version_id")
                .map_err(|e| e.to_string())?
                .to_string(),
        )
        .map_err(|e| e.to_string())?;
        let chunk_id = crate::rag::model::ChunkId::from_str(
            &row.try_get::<Uuid, _>("id")
                .map_err(|e| e.to_string())?
                .to_string(),
        )?;
        let source_location = serde_json::from_value(
            row.try_get::<serde_json::Value, _>("source_location")
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("PostgreSQL 来源定位无效: {e}"))?;
        chunks.push(RetrievedChunk {
            knowledge_base_id: kb,
            document_id,
            version_id,
            chunk_id,
            source_name: row.try_get("display_name").map_err(|e| e.to_string())?,
            source_location,
            text: row.try_get("text").map_err(|e| e.to_string())?,
            lexical_rank: Some(index + 1),
            dense_rank: None,
            rrf_score: row.try_get::<f32, _>("rank").map_err(|e| e.to_string())? as f64,
            rerank_score: None,
        });
    }
    let connection = open_query_connection(&database_path(app)?)?;
    persist_evidence_pack(
        &connection,
        current_workspace_id(),
        &request.query,
        RetrievalConfigSnapshot {
            knowledge_base_ids: request.knowledge_base_ids,
            lexical_limit: request.lexical_limit,
            dense_limit: 0,
            candidate_limit: request.candidate_limit,
            rrf_k: request.rrf_k,
            embedding_provider_profile_id: "postgresql".to_string(),
            embedding_model_id: "tsvector".to_string(),
            rerank_provider_profile_id: None,
            rerank_model_id: None,
            rerank_degradation: None,
        },
        chunks,
    )
    .map_err(|error| error.to_string())
}

pub(crate) async fn query_local_knowledge_from_path(
    path: std::path::PathBuf,
    workspace_id: &str,
    secrets: &dyn SecretStore,
    mut request: LocalKnowledgeQueryRequest,
) -> Result<EvidencePack, String> {
    request.validate()?;
    // Apply all active domain packages' retrieval policies. The strictest evidence cap
    // wins so no package receives more candidates than it declares.
    let connection = open_query_connection(&path)?;
    let active_domains = domains::active_manifests(&connection, workspace_id)?;
    drop(connection);
    if let Some(max_items) = active_domains
        .iter()
        .map(|manifest| manifest.retrieval.max_evidence_items)
        .filter(|value| *value > 0)
        .min()
    {
        request.candidate_limit = request.candidate_limit.min(max_items);
    }
    let connection = open_query_connection(&path)?;
    let (embedding_record, rerank_record, retrieval_plan) = {
        let embedding = provider_profiles::get_default_record(
            &connection,
            workspace_id,
            ProviderCapability::Embedding,
        )?;
        let rerank = provider_profiles::get_default_record(
            &connection,
            workspace_id,
            ProviderCapability::Rerank,
        )?;
        let plan = SiliconFlowPlan::from_setting(
            settings::get(&connection, workspace_id, "onboarding.retrieval")?.as_deref(),
        );
        Ok::<_, String>((embedding, rerank, plan))
    }?;
    drop(connection);
    let Some(embedding_record) = embedding_record else {
        let connection = open_query_connection(&path)?;
        return query_local_knowledge_fts_fallback(
            &connection,
            workspace_id,
            request,
            "local_fts",
            "keyword",
            Some(RerankDegradationReason::UnsupportedCapability),
        );
    };
    if request.dense_limit == 0 {
        let connection = open_query_connection(&path)?;
        return query_local_knowledge_fts_fallback(
            &connection,
            workspace_id,
            request,
            &embedding_record.profile.id.to_string(),
            embedding_record
                .profile
                .model_id
                .as_deref()
                .unwrap_or("keyword"),
            None,
        );
    }
    let embedding_provider =
        match prepare_embedding_provider(&embedding_record, secrets, retrieval_plan) {
            Ok(provider) => provider,
            Err(error) => {
                let connection = open_query_connection(&path)?;
                return query_local_knowledge_fts_fallback(
                    &connection,
                    workspace_id,
                    request,
                    &embedding_record.profile.id.to_string(),
                    embedding_record
                        .profile
                        .model_id
                        .as_deref()
                        .unwrap_or("keyword"),
                    Some(configuration_degradation(&error)),
                );
            }
        };
    let reranker = prepare_reranker(rerank_record, secrets, retrieval_plan);
    let embedding = match embedding_provider.embed(vec![request.query.clone()]).await {
        Ok(value) => value,
        Err(error) => {
            let connection = open_query_connection(&path)?;
            return query_local_knowledge_fts_fallback(
                &connection,
                workspace_id,
                request,
                &embedding_record.profile.id.to_string(),
                embedding_record
                    .profile
                    .model_id
                    .as_deref()
                    .unwrap_or("keyword"),
                Some(provider_degradation(&error)),
            );
        }
    };
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
    let content_root = path
        .parent()
        .ok_or_else(|| "resolve RAG content root failed".to_string())?;
    let connection = open_query_connection(&path)?;
    let snapshot = match load_index_snapshot(&connection, workspace_id, &index_request) {
        Ok(value) => value,
        Err(error) => {
            return query_local_knowledge_fts_fallback(
                &connection,
                workspace_id,
                request,
                &embedding_record.profile.id.to_string(),
                &embedding.model_id,
                Some(configuration_degradation(&error)),
            );
        }
    };
    let index = match open_with_flat_fallback(
        &connection,
        &index_root(content_root, &snapshot.watermark),
        &snapshot.watermark,
    ) {
        Ok(value) => value,
        Err(error) => {
            return query_local_knowledge_fts_fallback(
                &connection,
                workspace_id,
                request,
                &embedding_record.profile.id.to_string(),
                &embedding.model_id,
                Some(configuration_degradation(&error.to_string())),
            );
        }
    };
    let chunks = match retrieve(
        &connection,
        &index,
        &HybridSearchRequest {
            workspace_id: workspace_id.to_string(),
            query: request.query.clone(),
            query_vector,
            knowledge_base_ids: request.knowledge_base_ids.clone(),
            lexical_limit: request.lexical_limit,
            dense_limit: request.dense_limit,
            candidate_limit: request.candidate_limit,
            rrf_k: request.rrf_k,
        },
    ) {
        Ok(value) => value,
        Err(error) => {
            drop(index);
            return query_local_knowledge_fts_fallback(
                &connection,
                workspace_id,
                request,
                &embedding_record.profile.id.to_string(),
                &embedding.model_id,
                Some(configuration_degradation(&error.to_string())),
            );
        }
    };
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
        workspace_id,
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

fn query_local_knowledge_fts_fallback(
    connection: &Connection,
    workspace_id: &str,
    request: LocalKnowledgeQueryRequest,
    embedding_provider_profile_id: &str,
    embedding_model_id: &str,
    degradation: Option<RerankDegradationReason>,
) -> Result<EvidencePack, String> {
    let limit = request.lexical_limit.min(request.candidate_limit);
    let hits = if limit == 0 {
        Vec::new()
    } else {
        search_fts(
            connection,
            &FtsSearchRequest {
                workspace_id: workspace_id.to_string(),
                query: request.query.clone(),
                knowledge_base_ids: request.knowledge_base_ids.clone(),
                limit,
            },
        )
        .map_err(|error| error.to_string())?
    };
    let chunks = hits
        .into_iter()
        .enumerate()
        .map(|(index, hit)| fts_hit_to_retrieved_chunk(connection, workspace_id, hit, index + 1))
        .collect::<Result<Vec<_>, _>>()?;
    persist_evidence_pack(
        connection,
        workspace_id,
        &request.query,
        RetrievalConfigSnapshot {
            knowledge_base_ids: request.knowledge_base_ids,
            lexical_limit: request.lexical_limit,
            dense_limit: 0,
            candidate_limit: request.candidate_limit,
            rrf_k: request.rrf_k,
            embedding_provider_profile_id: embedding_provider_profile_id.to_string(),
            embedding_model_id: embedding_model_id.to_string(),
            rerank_provider_profile_id: None,
            rerank_model_id: None,
            rerank_degradation: degradation,
        },
        chunks,
    )
    .map_err(|error| error.to_string())
}

fn fts_hit_to_retrieved_chunk(
    connection: &Connection,
    workspace_id: &str,
    hit: FtsHit,
    rank: usize,
) -> Result<RetrievedChunk, String> {
    let source_location_json: String = connection
        .query_row(
            "SELECT source_location_json FROM knowledge_chunks
             WHERE workspace_id = ?1 AND version_id = ?2 AND id = ?3",
            rusqlite::params![
                workspace_id,
                hit.version_id.to_string(),
                hit.chunk_id.to_string()
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(RetrievedChunk {
        knowledge_base_id: hit.knowledge_base_id,
        document_id: hit.document_id,
        version_id: hit.version_id,
        chunk_id: hit.chunk_id,
        source_name: hit.source_name,
        source_location: serde_json::from_str(&source_location_json)
            .map_err(|error| error.to_string())?,
        text: hit.text,
        lexical_rank: Some(rank),
        dense_rank: None,
        rrf_score: 1.0 / rank as f64,
        rerank_score: None,
    })
}

fn configuration_degradation(message: &str) -> RerankDegradationReason {
    let lower = message.to_ascii_lowercase();
    if lower.contains("credential") || lower.contains("secret") {
        RerankDegradationReason::MissingCredential
    } else {
        RerankDegradationReason::InvalidConfiguration
    }
}

fn provider_degradation(error: &ProviderError) -> RerankDegradationReason {
    match error.code() {
        ProviderErrorCode::Network => RerankDegradationReason::Network,
        ProviderErrorCode::Authentication => RerankDegradationReason::Authentication,
        ProviderErrorCode::Quota => RerankDegradationReason::Quota,
        ProviderErrorCode::Timeout => RerankDegradationReason::Timeout,
        ProviderErrorCode::ProviderResponse => RerankDegradationReason::ProviderResponse,
        ProviderErrorCode::ContextLimit => RerankDegradationReason::ProviderResponse,
        ProviderErrorCode::Cancelled => RerankDegradationReason::Cancelled,
        ProviderErrorCode::UnsupportedCapability => RerankDegradationReason::UnsupportedCapability,
    }
}

fn prepare_embedding_provider(
    record: &ProviderProfileRecord,
    secrets: &dyn SecretStore,
    plan: SiliconFlowPlan,
) -> Result<ConfiguredEmbeddingProvider, String> {
    if record.profile.kind != ProviderKind::SiliconFlow {
        return Err("configured embedding provider is not supported".to_string());
    }
    let model = record
        .profile
        .model_id
        .clone()
        .ok_or_else(|| "embedding model is not configured".to_string())?;
    configured_embedding_provider(
        record.profile.clone(),
        Some(read_credential(record, secrets)?),
        plan,
        Some(model),
    )
    .map_err(|error| error.to_string())
}

fn prepare_reranker(
    record: Option<ProviderProfileRecord>,
    secrets: &dyn SecretStore,
    plan: SiliconFlowPlan,
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
    match configured_rerank_provider(
        record.profile.clone(),
        Some(credential),
        plan,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::model::{
        ChunkId, EmbeddingIdentity, EmbeddingVectorBatch, NewChunk, NewDocumentVersion,
        NewSourceDocument, SourceLocation,
    };
    use crate::storage::secrets::{SecretError, SecretRef, SecretValue};
    use sha2::{Digest, Sha256};

    struct EmptySecretStore;

    impl SecretStore for EmptySecretStore {
        fn set(&self, _reference: &SecretRef, _value: &SecretValue) -> Result<(), SecretError> {
            Ok(())
        }

        fn get(&self, _reference: &SecretRef) -> Result<SecretValue, SecretError> {
            Err(SecretError::not_found())
        }

        fn delete(&self, _reference: &SecretRef) -> Result<(), SecretError> {
            Ok(())
        }
    }

    #[test]
    fn local_knowledge_query_falls_back_to_fts_without_embedding_provider() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-local-knowledge-fallback-{}.sqlite3",
            Uuid::new_v4()
        ));
        let (mut connection, _) = crate::storage::database::open(&path).expect("open test db");
        let base = knowledge::create_knowledge_base(&connection, "local", "Steel docs")
            .expect("create base")
            .id;
        let document = knowledge::create_source_document(
            &connection,
            "local",
            NewSourceDocument {
                knowledge_base_id: base,
                display_name: "Q355B.md".to_string(),
                source_kind: "file".to_string(),
            },
        )
        .expect("create document");
        let version = knowledge::create_document_version(
            &connection,
            "local",
            NewDocumentVersion {
                document_id: document.id,
                content_sha256: "a".repeat(64),
                mime_type: "text/markdown".to_string(),
                parser: "test".to_string(),
                parser_version: "1".to_string(),
                chunk_policy_version: "steel-v1".to_string(),
                embedding_profile_id: "11111111-1111-4111-8111-111111111111".to_string(),
                embedding_model_id: "BAAI/bge-m3".to_string(),
                embedding_dimension: 2,
                expected_asset_count: 0,
                expected_chunk_count: 1,
            },
        )
        .expect("create version");
        let chunk_id = ChunkId::new("chunk-1").expect("chunk id");
        knowledge::add_chunk(
            &connection,
            "local",
            NewChunk {
                id: chunk_id.clone(),
                version_id: version.id,
                ordinal: 0,
                text: "Q355B 屈服强度通常需要结合厚度和 GB/T 1591 标准判断。".to_string(),
                source_location: SourceLocation::Heading {
                    path: vec!["力学性能".to_string()],
                },
                content_sha256: "b".repeat(64),
                policy_version: "steel-v1".to_string(),
            },
        )
        .expect("add chunk");
        knowledge::index_chunk_fts(&mut connection, "local", version.id, &chunk_id)
            .expect("index chunk");
        let vector = vec![0_u8; 8];
        knowledge::persist_embedding_batch(
            &mut connection,
            "local",
            version.id,
            &[EmbeddingVectorBatch {
                vector_key: "vector-a-0".to_string(),
                identity: EmbeddingIdentity {
                    provider_profile_id: "11111111-1111-4111-8111-111111111111".to_string(),
                    model_id: "BAAI/bge-m3".to_string(),
                    dimension: 2,
                    normalized_text_sha256: "c".repeat(64),
                    policy_version: "steel-v1".to_string(),
                },
                vector_sha256: format!("{:x}", Sha256::digest(&vector)),
                vector_blob: vector,
                chunk_ids: vec![chunk_id],
            }],
        )
        .expect("persist embedding");
        knowledge::finalize_flat_index(&mut connection, "local", version.id)
            .expect("finalize flat index");
        knowledge::activate_document_version(&mut connection, "local", document.id, version.id)
            .expect("activate version");
        drop(connection);

        let pack = tauri::async_runtime::block_on(query_local_knowledge_from_path(
            path.clone(),
            "local",
            &EmptySecretStore,
            LocalKnowledgeQueryRequest {
                query: "Q355B 屈服强度".to_string(),
                knowledge_base_ids: vec![base],
                lexical_limit: 10,
                dense_limit: 10,
                candidate_limit: 5,
                rrf_k: 60,
                rerank_limit: 5,
            },
        ))
        .expect("query should fall back to FTS");

        assert_eq!(pack.evidence.len(), 1);
        assert_eq!(pack.configuration.dense_limit, 0);
        assert_eq!(
            pack.configuration.rerank_degradation,
            Some(RerankDegradationReason::UnsupportedCapability)
        );
        assert!(pack.evidence[0].chunk.text.contains("Q355B"));

        let _ = std::fs::remove_file(path);
    }
}
