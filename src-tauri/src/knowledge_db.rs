use crate::db::{current_workspace_id, with_conn, with_conn_mut, with_conn_ref, DbState};
use crate::providers::capabilities::{EmbeddingProvider, RerankProvider};
use crate::providers::profiles::{ProviderKind, ProviderProfileRecord};
use crate::providers::{
    configured_embedding_provider, configured_rerank_provider, SiliconFlowPlan,
};
use crate::rag::chunk::{chunk_document, ChunkPolicy};
use crate::rag::ingest::{ingest_file, IngestLimits};
use crate::rag::parse::{parse_document, ParseLimits};
use crate::rag::tasks::ContentStore;
use crate::storage::repositories::provider_profiles;
use crate::storage::repositories::settings;
use crate::storage::secrets::{SecretRef, SecretState, SecretValue};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use std::sync::Mutex;
use std::time::Instant;
use tauri::Manager;
use uuid::Uuid;

const CONFIG_KEY: &str = "knowledge.postgres";
const CONFIG_BACKUP_KEY: &str = "knowledge.postgres.backup";
const MIN_POSTGRES_VERSION: i64 = 140_000;
const PASSWORD_NAME: &str = "knowledge_postgres_password";
const PASSWORD_BACKUP_NAME: &str = "knowledge_postgres_password_backup";
const KNOWLEDGE_SECRET_ID: Uuid = Uuid::from_u128(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeDatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeDatabaseHealth {
    pub configured: bool,
    pub config: Option<KnowledgeDatabaseConfig>,
    pub connected: bool,
    pub vector_extension: bool,
    pub migration_version: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresKnowledgeHealth {
    pub knowledge_base_count: i64,
    pub document_count: i64,
    pub active_document_count: i64,
    pub version_count: i64,
    pub chunk_count: i64,
    pub indexed_chunk_count: i64,
    pub active_task_count: i64,
    pub processing_success_count: i64,
    pub processing_failure_count: i64,
    pub retrieval_count: i64,
    pub average_retrieval_duration_ms: Option<f64>,
    pub average_processing_duration_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresKnowledgeBaseRecord {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub library_type: String,
    pub tags: Vec<String>,
    pub visibility: String,
    pub embedding_model: String,
    pub chunk_strategy: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresKnowledgeBaseInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_library_type")]
    pub library_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default = "default_chunk_strategy")]
    pub chunk_strategy: String,
}

const MAX_KNOWLEDGE_BASE_NAME: usize = 200;
const MAX_KNOWLEDGE_BASE_DESCRIPTION: usize = 2_000;
const MAX_KNOWLEDGE_BASE_TAGS: usize = 50;
const MAX_KNOWLEDGE_BASE_TAG_LENGTH: usize = 100;
const MAX_EMBEDDING_MODEL_LENGTH: usize = 200;

/// Normalize and validate all fields that define a knowledge base.  Both the
/// Tauri command and the local HTTP API use this helper so they cannot drift.
pub(crate) fn normalize_knowledge_base_input(
    mut input: PostgresKnowledgeBaseInput,
) -> Result<PostgresKnowledgeBaseInput, String> {
    input.name = input.name.trim().to_string();
    if input.name.is_empty() {
        return Err("知识库名称不能为空".to_string());
    }
    if input.name.chars().count() > MAX_KNOWLEDGE_BASE_NAME {
        return Err("知识库名称不能超过 200 个字符".to_string());
    }

    input.description = input.description.trim().to_string();
    if input.description.chars().count() > MAX_KNOWLEDGE_BASE_DESCRIPTION {
        return Err("知识库描述不能超过 2000 个字符".to_string());
    }

    input.library_type = input.library_type.trim().to_ascii_lowercase();
    if !matches!(
        input.library_type.as_str(),
        "research" | "standard" | "experiment" | "other"
    ) {
        return Err("知识库类型无效".to_string());
    }

    input.visibility = input.visibility.trim().to_ascii_lowercase();
    validate_knowledge_visibility(&input.visibility)?;

    input.embedding_model = input.embedding_model.trim().to_string();
    if input.embedding_model.chars().count() > MAX_EMBEDDING_MODEL_LENGTH {
        return Err("Embedding 模型名称不能超过 200 个字符".to_string());
    }

    input.chunk_strategy = input.chunk_strategy.trim().to_ascii_lowercase();
    if !matches!(
        input.chunk_strategy.as_str(),
        "semantic" | "fixed" | "heading"
    ) {
        return Err("Chunk 策略无效".to_string());
    }

    let mut tags = Vec::with_capacity(input.tags.len());
    for tag in input.tags.drain(..) {
        let tag = tag.trim().to_string();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > MAX_KNOWLEDGE_BASE_TAG_LENGTH {
            return Err("知识库标签不能超过 100 个字符".to_string());
        }
        if !tags.iter().any(|existing| existing == &tag) {
            tags.push(tag);
        }
    }
    if tags.len() > MAX_KNOWLEDGE_BASE_TAGS {
        return Err("知识库标签不能超过 50 个".to_string());
    }
    input.tags = tags;
    Ok(input)
}

fn default_library_type() -> String {
    "research".to_string()
}
fn default_visibility() -> String {
    "private".to_string()
}
fn default_chunk_strategy() -> String {
    "semantic".to_string()
}

pub(crate) fn validate_knowledge_visibility(value: &str) -> Result<&str, String> {
    match value.trim() {
        "private" | "public" => Ok(value.trim()),
        _ => Err("单人客户端不支持项目或工作区共享范围".to_string()),
    }
}

/// Suna 是单人桌面客户端；本地知识库不提供成员或角色授权。
pub(crate) fn validate_local_knowledge_access(visibility: &str) -> Result<(), String> {
    validate_knowledge_visibility(visibility)?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresKnowledgeBaseMergeRequest {
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub mode: String,
    pub destination_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresKnowledgeBaseDeleteImpact {
    pub knowledge_base_id: String,
    pub name: String,
    pub document_count: u32,
    pub version_count: u32,
    pub chunk_count: u32,
    pub asset_count: u32,
    pub active_task_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresWikiPageRecord {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub slug: String,
    pub title: String,
    pub body_markdown: String,
    pub source_document_id: Option<Uuid>,
    pub revision: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresWikiPageInput {
    pub knowledge_base_id: Uuid,
    pub slug: String,
    pub title: String,
    pub body_markdown: String,
    pub source_document_id: Option<Uuid>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresWikiRevisionRecord {
    pub id: Uuid,
    pub page_id: Uuid,
    pub revision: i32,
    pub title: String,
    pub body_markdown: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresKnowledgeEdgeRecord {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub source_page_id: Option<Uuid>,
    pub target_page_id: Option<Uuid>,
    pub source_document_id: Option<Uuid>,
    pub relation: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresTagRecord {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresKnowledgeEdgeInput {
    pub knowledge_base_id: Uuid,
    pub source_page_id: Option<Uuid>,
    pub target_page_id: Option<Uuid>,
    pub relation: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresDocumentImportRequest {
    pub knowledge_base_id: Uuid,
    pub source_path: std::path::PathBuf,
    #[serde(default)]
    pub embedding_profile_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresDocumentImportResponse {
    pub task_id: Uuid,
    pub knowledge_base_id: Uuid,
    pub document_id: Uuid,
    pub version_id: Uuid,
    pub chunk_count: u32,
    pub asset_count: u32,
    pub duplicate_content: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresDocumentQueuedResponse {
    pub task_id: Uuid,
    pub knowledge_base_id: Uuid,
    pub state: String,
    pub progress: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresKnowledgeSearchRequest {
    pub knowledge_base_id: Uuid,
    pub query: String,
    pub limit: u32,
    #[serde(default)]
    pub filters: Option<PostgresKnowledgeSearchFilters>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostgresKnowledgeSearchFilters {
    pub document_type: Option<String>,
    pub material: Option<String>,
    pub process: Option<String>,
    pub property: Option<String>,
    pub year: Option<i32>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresKnowledgeSearchHit {
    pub knowledge_base_id: Uuid,
    pub chunk_id: Uuid,
    pub version_id: Uuid,
    pub document_id: Uuid,
    pub document_name: String,
    pub text: String,
    pub source_location: serde_json::Value,
    pub rank: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresKnowledgeChunkRecord {
    pub id: Uuid,
    pub document_id: Uuid,
    pub document_name: String,
    pub version_id: Uuid,
    pub ordinal: i32,
    pub title_path: String,
    pub text: String,
    pub source_location: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresCitation {
    pub audit_id: Uuid,
    pub citation_number: u32,
    pub source_state: String,
    pub hit: PostgresKnowledgeSearchHit,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresIngestionJobRecord {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub source_document_id: Option<Uuid>,
    pub state: String,
    pub progress: i32,
    pub attempts: i32,
    pub error_message: Option<String>,
    pub next_attempt_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresProcessingFailureRecord {
    pub id: Uuid,
    pub job_id: Uuid,
    pub error_code: String,
    pub error_message: String,
    pub details: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresDocumentRecord {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub display_name: String,
    pub source_kind: String,
    pub source_path: String,
    pub content_sha256: String,
    pub file_size: i64,
    pub metadata: serde_json::Value,
    pub active_version_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresDocumentVersionRecord {
    pub id: Uuid,
    pub document_id: Uuid,
    pub content_sha256: String,
    pub mime_type: String,
    pub parser: String,
    pub parser_version: String,
    pub chunk_policy_version: String,
    pub embedding_profile_id: String,
    pub embedding_model_id: String,
    pub embedding_dimension: i32,
    pub expected_asset_count: i64,
    pub expected_chunk_count: i64,
    pub manifest_sealed: bool,
    pub created_at: String,
    pub activated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresChunkEmbeddingInput {
    pub chunk_id: Uuid,
    pub model_id: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresVectorSearchRequest {
    pub knowledge_base_id: Uuid,
    pub embedding: Vec<f32>,
    pub limit: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresHybridSearchRequest {
    pub knowledge_base_id: Uuid,
    pub query: String,
    pub embedding: Vec<f32>,
    pub limit: u32,
    pub rrf_k: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresEmbeddingRequest {
    pub version_id: Uuid,
    pub embedding_profile_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresRerankRequest {
    pub query: String,
    pub rerank_profile_id: Uuid,
    pub hits: Vec<PostgresKnowledgeSearchHit>,
}

fn encode_pgvector(values: &[f32]) -> Result<String, String> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return Err("Embedding 向量为空或包含无效值".to_string());
    }
    Ok(format!(
        "[{}]",
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    ))
}

fn validate_embedding_batch(embeddings: &[PostgresChunkEmbeddingInput]) -> Result<i32, String> {
    let first = embeddings
        .first()
        .ok_or_else(|| "Embedding 向量不能为空".to_string())?;
    let dimension = i32::try_from(first.embedding.len()).map_err(|_| "Embedding 维度过大")?;
    if dimension <= 0 {
        return Err("Embedding 维度必须为正数".to_string());
    }
    for item in embeddings {
        if item.embedding.len() as i32 != dimension {
            return Err("同一批 Embedding 的维度必须一致".to_string());
        }
        encode_pgvector(&item.embedding)?;
    }
    Ok(dimension)
}

fn provider_credential(
    record: &ProviderProfileRecord,
    secrets: &SecretState,
) -> Result<SecretValue, String> {
    let name = record
        .profile
        .secret_ref
        .as_deref()
        .ok_or_else(|| "Embedding provider 凭据未配置".to_string())?;
    let reference = SecretRef::at_generation(record.profile.id, name, record.secret_generation)
        .map_err(|error| error.to_string())?;
    secrets
        .store()
        .get(&reference)
        .map_err(|error| error.to_string())
}

#[derive(Default)]
pub struct KnowledgeDatabaseState {
    pool: Mutex<Option<PgPool>>,
}

fn secret_ref() -> Result<SecretRef, String> {
    SecretRef::new(KNOWLEDGE_SECRET_ID, PASSWORD_NAME).map_err(|error| error.to_string())
}

fn password_backup_ref() -> Result<SecretRef, String> {
    SecretRef::new(KNOWLEDGE_SECRET_ID, PASSWORD_BACKUP_NAME).map_err(|error| error.to_string())
}

fn validate_config(config: &KnowledgeDatabaseConfig) -> Result<(), String> {
    if config.host.trim().is_empty()
        || config.database.trim().is_empty()
        || config.username.trim().is_empty()
    {
        return Err("PostgreSQL 地址、数据库名和用户名不能为空".to_string());
    }
    if config.port == 0 {
        return Err("PostgreSQL 端口无效".to_string());
    }
    Ok(())
}

async fn connect(config: &KnowledgeDatabaseConfig, password: &str) -> Result<PgPool, String> {
    validate_config(config)?;
    let options = sqlx::postgres::PgConnectOptions::new()
        .host(config.host.trim())
        .port(config.port)
        .database(config.database.trim())
        .username(config.username.trim())
        .password(password);
    PgPoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|error| format!("PostgreSQL 连接失败: {}", safe_error(&error.to_string())))
}

async fn check_vector(pool: &PgPool) -> Result<bool, String> {
    let row = sqlx::query(
        "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') AS installed",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("检查 pgvector 失败: {}", safe_error(&error.to_string())))?;
    Ok(row.try_get("installed").unwrap_or(false))
}

async fn prepare_pool(pool: &PgPool) -> Result<i64, String> {
    let server_version: i64 =
        sqlx::query_scalar("SELECT current_setting('server_version_num')::bigint")
            .fetch_one(pool)
            .await
            .map_err(|error| {
                format!(
                    "检查 PostgreSQL 版本失败: {}",
                    safe_error(&error.to_string())
                )
            })?;
    if server_version < MIN_POSTGRES_VERSION {
        return Err("知识库要求 PostgreSQL 14 或更高版本".to_string());
    }
    if !check_vector(pool).await? {
        return Err("PostgreSQL 未安装或未启用 pgvector 扩展".to_string());
    }
    sqlx::migrate!("migrations/knowledge")
        .run(pool)
        .await
        .map_err(|error| format!("知识库迁移失败: {}", safe_error(&error.to_string())))?;
    let version =
        sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .map_err(|error| {
                format!("读取知识库迁移版本失败: {}", safe_error(&error.to_string()))
            })?;
    recover_interrupted_ingestion_jobs(pool).await?;
    Ok(version)
}

async fn recover_interrupted_ingestion_jobs(pool: &PgPool) -> Result<u64, String> {
    let mut transaction = pool.begin().await.map_err(|error| {
        format!(
            "恢复 PostgreSQL 导入任务失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let rows = sqlx::query(
        "SELECT id, state
         FROM ingestion_jobs
         WHERE state IN ('queued', 'uploading', 'parsing', 'chunking',
                         'embedding', 'indexing', 'pending', 'running', 'retrying')
         FOR UPDATE",
    )
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "读取中断的 PostgreSQL 导入任务失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let mut recovered = 0_u64;
    for row in rows {
        let job_id: Uuid = row.try_get("id").map_err(|error| error.to_string())?;
        let previous_state: String = row.try_get("state").map_err(|error| error.to_string())?;
        let message = format!(
            "Suna 上次退出时导入任务停留在 {} 阶段，请重试",
            previous_state
        );
        sqlx::query(
            "UPDATE ingestion_jobs
             SET state = 'failed', error_message = $1,
                 next_attempt_at = NULL, updated_at = now()
             WHERE id = $2",
        )
        .bind(&message)
        .bind(job_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "更新中断的 PostgreSQL 导入任务失败: {}",
                safe_error(&error.to_string())
            )
        })?;
        sqlx::query(
            "UPDATE ingestion_attempts
             SET state = 'failed', error_message = $1, finished_at = now()
             WHERE job_id = $2 AND finished_at IS NULL",
        )
        .bind(&message)
        .bind(job_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "关闭中断的 PostgreSQL 导入尝试失败: {}",
                safe_error(&error.to_string())
            )
        })?;
        sqlx::query(
            "INSERT INTO processing_failures
                (id, job_id, error_code, error_message, details)
             VALUES ($1, $2, 'application_restart', $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(job_id)
        .bind(&message)
        .bind(serde_json::json!({
            "reason": "application_restart",
            "previous_state": previous_state,
            "recoverable": true
        }))
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "记录 PostgreSQL 导入恢复失败: {}",
                safe_error(&error.to_string())
            )
        })?;
        recovered += 1;
    }
    transaction.commit().await.map_err(|error| {
        format!(
            "提交 PostgreSQL 导入恢复失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    Ok(recovered)
}

fn safe_error(error: &str) -> String {
    error
        .replace("password", "credential")
        .replace("PASSWORD", "credential")
}

fn load_config(db: &tauri::State<'_, DbState>) -> Result<Option<KnowledgeDatabaseConfig>, String> {
    with_conn(db, |connection| {
        settings::get(connection, current_workspace_id(), CONFIG_KEY)?.map_or(Ok(None), |value| {
            serde_json::from_str(&value)
                .map(Some)
                .map_err(|error| error.to_string())
        })
    })
}

fn backup_config(
    db: &tauri::State<'_, DbState>,
    config: &KnowledgeDatabaseConfig,
) -> Result<(), String> {
    let value = serde_json::to_string(config).map_err(|error| error.to_string())?;
    with_conn_mut(db, |connection| {
        settings::set(
            connection,
            current_workspace_id(),
            CONFIG_BACKUP_KEY,
            &value,
        )
    })
}

fn restore_config_backup(
    db: &tauri::State<'_, DbState>,
    secrets: &SecretState,
) -> Result<(), String> {
    let store = secrets.store();
    let active_ref = secret_ref()?;
    let backup_ref = password_backup_ref()?;
    let previous_password = store.get(&backup_ref);
    with_conn_mut(db, |connection| {
        let backup = settings::get(connection, current_workspace_id(), CONFIG_BACKUP_KEY)?;
        if let Some(value) = backup {
            settings::set(connection, current_workspace_id(), CONFIG_KEY, &value)?;
        } else {
            connection
                .execute(
                    "DELETE FROM settings WHERE workspace_id = ?1 AND key = ?2",
                    rusqlite::params![current_workspace_id(), CONFIG_KEY],
                )
                .map_err(|error| error.to_string())?;
        }
        connection
            .execute(
                "DELETE FROM settings WHERE workspace_id = ?1 AND key = ?2",
                rusqlite::params![current_workspace_id(), CONFIG_BACKUP_KEY],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    })
    .and_then(|()| match previous_password {
        Ok(password) => store
            .set(&active_ref, &password)
            .map_err(|error| error.to_string()),
        Err(error) if error.is_not_found() => store
            .delete(&active_ref)
            .or_else(|delete_error| {
                if delete_error.is_not_found() {
                    Ok(())
                } else {
                    Err(delete_error)
                }
            })
            .map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    })
    .and_then(|()| {
        store
            .delete(&backup_ref)
            .or_else(|error| {
                if error.is_not_found() {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|error| error.to_string())
    })
}

fn clear_config_backup(
    db: &tauri::State<'_, DbState>,
    secrets: &SecretState,
) -> Result<(), String> {
    with_conn_mut(db, |connection| {
        connection
            .execute(
                "DELETE FROM settings WHERE workspace_id = ?1 AND key = ?2",
                rusqlite::params![current_workspace_id(), CONFIG_BACKUP_KEY],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    })?;
    let backup_ref = password_backup_ref()?;
    secrets
        .store()
        .delete(&backup_ref)
        .or_else(|error| {
            if error.is_not_found() {
                Ok(())
            } else {
                Err(error)
            }
        })
        .map_err(|error| error.to_string())
}

fn read_password(secrets: &tauri::State<'_, SecretState>) -> Result<SecretValue, String> {
    secrets
        .store()
        .get(&secret_ref()?)
        .map_err(|error| format!("PostgreSQL 密码未配置: {error}"))
}

fn active_pool(state: &tauri::State<'_, KnowledgeDatabaseState>) -> Result<PgPool, String> {
    state
        .pool
        .lock()
        .map_err(|_| "知识库状态已损坏")?
        .clone()
        .ok_or_else(|| "PostgreSQL 知识库尚未初始化".to_string())
}

async fn ensure_private_knowledge_base(pool: &PgPool, id: Uuid) -> Result<(), String> {
    let visibility = sqlx::query_scalar::<_, String>(
        "SELECT visibility FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("检查知识库写入权限失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "PostgreSQL 知识库不存在".to_string())?;
    if visibility != "private" {
        return Err("公共知识库为只读，单人客户端不能修改".to_string());
    }
    Ok(())
}

pub(crate) fn pool_for_query(state: &KnowledgeDatabaseState) -> Result<PgPool, String> {
    state
        .pool
        .lock()
        .map_err(|_| "知识库状态已损坏".to_string())?
        .clone()
        .ok_or_else(|| "PostgreSQL 知识库尚未初始化".to_string())
}

#[tauri::command]
pub async fn import_postgres_document(
    app: tauri::AppHandle,
    state: tauri::State<'_, KnowledgeDatabaseState>,
    request: PostgresDocumentImportRequest,
) -> Result<PostgresDocumentQueuedResponse, String> {
    queue_postgres_document_import(&app, &state, request).await
}

async fn create_postgres_import_job(
    pool: &PgPool,
    knowledge_base_id: Uuid,
) -> Result<(Uuid, Uuid), String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let exists: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM knowledge_bases
         WHERE id = $1 AND visibility = 'private' AND deleted_at IS NULL
         FOR KEY SHARE",
    )
    .bind(knowledge_base_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "检查 PostgreSQL 知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    if exists.is_none() {
        return Err("PostgreSQL 知识库不存在".to_string());
    }
    let job_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ingestion_jobs
            (id, knowledge_base_id, state, progress, attempts)
         VALUES ($1, $2, 'queued', 8, 1)",
    )
    .bind(job_id)
    .bind(knowledge_base_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "创建 PostgreSQL 导入任务失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    sqlx::query(
        "INSERT INTO ingestion_attempts (id, job_id, state, started_at)
         VALUES ($1, $2, 'running', now())",
    )
    .bind(attempt_id)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "创建 PostgreSQL 导入尝试失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok((job_id, attempt_id))
}

async fn queue_postgres_document_import(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    request: PostgresDocumentImportRequest,
) -> Result<PostgresDocumentQueuedResponse, String> {
    let pool = pool_for_query(state)?;
    let (job_id, attempt_id) = create_postgres_import_job(&pool, request.knowledge_base_id).await?;
    let knowledge_base_id = request.knowledge_base_id;
    spawn_postgres_import_task(app, pool, request, job_id, attempt_id);
    Ok(PostgresDocumentQueuedResponse {
        task_id: job_id,
        knowledge_base_id,
        state: "queued".to_string(),
        progress: 8,
    })
}

fn spawn_postgres_import_task(
    app: &tauri::AppHandle,
    pool: PgPool,
    request: PostgresDocumentImportRequest,
    job_id: Uuid,
    attempt_id: Uuid,
) {
    let app = app.clone();
    tokio::spawn(async move {
        let task_state = KnowledgeDatabaseState {
            pool: Mutex::new(Some(pool.clone())),
        };
        if let Err(error) = import_postgres_document_inner_with_job(
            &app,
            &task_state,
            request,
            Some((job_id, attempt_id)),
        )
        .await
        {
            let _ = record_postgres_ingestion_failure(
                &pool,
                job_id,
                attempt_id,
                "background_import_failed",
                &error,
            )
            .await;
        }
    });
}

async fn queue_postgres_document_reparse(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    document_id: Uuid,
) -> Result<PostgresDocumentQueuedResponse, String> {
    let pool = pool_for_query(state)?;
    let (source_path, knowledge_base_id): (String, Uuid) = sqlx::query_as(
        "SELECT COALESCE(NULLIF(storage_path, ''), source_path), knowledge_base_id
         FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("读取文档路径失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "文档不存在".to_string())?;
    ensure_private_knowledge_base(&pool, knowledge_base_id).await?;
    let source_path = authorize_postgres_content_file(app, &source_path)?;
    let request = PostgresDocumentImportRequest {
        knowledge_base_id,
        source_path,
        embedding_profile_id: None,
    };
    let (job_id, attempt_id) = create_postgres_import_job(&pool, knowledge_base_id).await?;
    spawn_postgres_import_task(app, pool, request, job_id, attempt_id);
    Ok(PostgresDocumentQueuedResponse {
        task_id: job_id,
        knowledge_base_id,
        state: "queued".to_string(),
        progress: 8,
    })
}

async fn import_postgres_document_inner_with_job(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    request: PostgresDocumentImportRequest,
    existing_job: Option<(Uuid, Uuid)>,
) -> Result<PostgresDocumentImportResponse, String> {
    let pool = pool_for_query(state)?;
    let embedding_profile_id = match request.embedding_profile_id {
        Some(id) => Some(id),
        None => {
            let db = app.state::<DbState>();
            with_conn_ref(db.inner(), |connection| {
                provider_profiles::get_default_record(
                    connection,
                    current_workspace_id(),
                    crate::providers::profiles::ProviderCapability::Embedding,
                )
                .map(|record| record.map(|record| record.profile.id))
            })?
        }
    };
    let embedding_model_id = if let Some(profile_id) = embedding_profile_id {
        let db = app.state::<DbState>();
        with_conn_ref(db.inner(), |connection| {
            provider_profiles::get_record(connection, current_workspace_id(), profile_id)
                .map(|record| record.and_then(|record| record.profile.model_id))
        })?
        .unwrap_or_default()
    } else {
        String::new()
    };
    let (job_id, attempt_id) = match existing_job {
        Some(job) => job,
        None => create_postgres_import_job(&pool, request.knowledge_base_id).await?,
    };
    update_ingestion_stage_or_fail(&pool, job_id, attempt_id, "uploading").await?;
    let content_root = match crate::db::database_path(&app)
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
    {
        Some(path) => path,
        None => {
            let error = "解析本地内容目录失败";
            record_postgres_ingestion_failure(
                &pool,
                job_id,
                attempt_id,
                "storage_path_failed",
                error,
            )
            .await?;
            return Err(error.to_string());
        }
    };
    let source = match ingest_file(&request.source_path, &content_root, IngestLimits::default()) {
        Ok(source) => source,
        Err(error) => {
            record_postgres_ingestion_failure(
                &pool,
                job_id,
                attempt_id,
                "ingest_failed",
                &error.to_string(),
            )
            .await?;
            return Err(error.to_string());
        }
    };
    let display_name = match request.source_path.file_name() {
        Some(value) => value.to_string_lossy().into_owned(),
        None => {
            let error = "源文件名不能为空";
            record_postgres_ingestion_failure(
                &pool,
                job_id,
                attempt_id,
                "source_name_failed",
                error,
            )
            .await?;
            return Err(error.to_string());
        }
    };
    update_ingestion_stage_or_fail(&pool, job_id, attempt_id, "parsing").await?;
    let parsed = match parse_document(&source.stored_path, source.format, ParseLimits::default()) {
        Ok(parsed) => parsed,
        Err(error) => {
            record_postgres_ingestion_failure(
                &pool,
                job_id,
                attempt_id,
                "parse_failed",
                &error.to_string(),
            )
            .await?;
            return Err(error.to_string());
        }
    };
    update_ingestion_stage_or_fail(&pool, job_id, attempt_id, "chunking").await?;
    let chunks = match chunk_document(&parsed, &ChunkPolicy::default()) {
        Ok(chunks) => chunks,
        Err(error) => {
            record_postgres_ingestion_failure(
                &pool,
                job_id,
                attempt_id,
                "chunk_failed",
                &error.to_string(),
            )
            .await?;
            return Err(error.to_string());
        }
    };
    update_ingestion_stage_or_fail(&pool, job_id, attempt_id, "indexing").await?;
    let import_failure_guard = ImportFailureGuard::new(&pool, job_id, attempt_id);
    let asset_store = ContentStore::new(content_root);
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let hash_document_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM source_documents
         WHERE knowledge_base_id = $1 AND content_sha256 = $2 AND deleted_at IS NULL",
    )
    .bind(request.knowledge_base_id)
    .bind(&source.content_sha256)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("检查文档 hash 失败: {}", safe_error(&error.to_string())))?;
    if let Some(document_id) = hash_document_id {
        let version_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM document_versions
             WHERE document_id = $1 AND content_sha256 = $2
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(document_id)
        .bind(&source.content_sha256)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| format!("读取已有文档版本失败: {}", safe_error(&error.to_string())))?;
        sqlx::query(
            "UPDATE ingestion_jobs
             SET source_document_id = $1, state = 'completed', progress = 100, updated_at = now()
             WHERE id = $2",
        )
        .bind(document_id)
        .bind(job_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "更新重复文档导入任务失败: {}",
                safe_error(&error.to_string())
            )
        })?;
        sqlx::query(
            "UPDATE ingestion_attempts SET state = 'completed', finished_at = now()
             WHERE id = $1 AND job_id = $2",
        )
        .bind(attempt_id)
        .bind(job_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "更新重复文档导入尝试失败: {}",
                safe_error(&error.to_string())
            )
        })?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        import_failure_guard.disarm();
        return Ok(PostgresDocumentImportResponse {
            task_id: job_id,
            knowledge_base_id: request.knowledge_base_id,
            document_id,
            version_id,
            chunk_count: 0,
            asset_count: 0,
            duplicate_content: true,
        });
    }
    let source_path = request.source_path.to_string_lossy().to_string();
    let storage_path = source.stored_path.to_string_lossy().to_string();
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM source_documents
         WHERE knowledge_base_id = $1 AND source_path = $2 AND deleted_at IS NULL
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(request.knowledge_base_id)
    .bind(&source_path)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("检查已有源文档失败: {}", safe_error(&error.to_string())))?
    .unwrap_or_else(Uuid::new_v4);
    let existing_document: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM source_documents WHERE id = $1)")
            .bind(document_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
    if existing_document {
        let existing_version_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM document_versions
             WHERE document_id = $1 AND content_sha256 = $2",
        )
        .bind(document_id)
        .bind(&source.content_sha256)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "检查 PostgreSQL 历史版本失败: {}",
                safe_error(&error.to_string())
            )
        })?;
        if let Some(version_id) = existing_version_id {
            sqlx::query(
                "UPDATE source_documents SET display_name = $1, source_kind = $2,
                        content_sha256 = $3, storage_path = $4, updated_at = now()
                 WHERE id = $5 AND deleted_at IS NULL",
            )
            .bind(&display_name)
            .bind(source.format.as_str())
            .bind(&source.content_sha256)
            .bind(&storage_path)
            .bind(document_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                format!(
                    "更新 PostgreSQL 源文档失败: {}",
                    safe_error(&error.to_string())
                )
            })?;
            sqlx::query(
                "UPDATE document_versions SET activated_at = NULL
                 WHERE document_id = $1 AND activated_at IS NOT NULL",
            )
            .bind(document_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("停用当前文档版本失败: {}", safe_error(&error.to_string())))?;
            sqlx::query("UPDATE document_versions SET activated_at = now() WHERE id = $1")
                .bind(version_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    format!(
                        "重新激活 PostgreSQL 历史版本失败: {}",
                        safe_error(&error.to_string())
                    )
                })?;
            sqlx::query(
                "UPDATE ingestion_jobs
                 SET source_document_id = $1, state = 'completed', progress = 100, updated_at = now()
                 WHERE id = $2",
            )
            .bind(document_id)
            .bind(job_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE ingestion_attempts SET state = 'completed', finished_at = now()
                 WHERE id = $1 AND job_id = $2",
            )
            .bind(attempt_id)
            .bind(job_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
            transaction
                .commit()
                .await
                .map_err(|error| error.to_string())?;
            import_failure_guard.disarm();
            return Ok(PostgresDocumentImportResponse {
                task_id: job_id,
                knowledge_base_id: request.knowledge_base_id,
                document_id,
                version_id,
                chunk_count: 0,
                asset_count: 0,
                duplicate_content: true,
            });
        }
    }
    if existing_document {
        sqlx::query(
            "UPDATE source_documents SET display_name = $1, source_kind = $2,
                    storage_path = $3, content_sha256 = $4, updated_at = now()
             WHERE id = $5 AND deleted_at IS NULL",
        )
        .bind(&display_name)
        .bind(source.format.as_str())
        .bind(&storage_path)
        .bind(&source.content_sha256)
        .bind(document_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "更新 PostgreSQL 源文档失败: {}",
                safe_error(&error.to_string())
            )
        })?;
        sqlx::query("UPDATE document_versions SET activated_at = NULL WHERE document_id = $1 AND activated_at IS NOT NULL")
            .bind(document_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("停用旧文档版本失败: {}", safe_error(&error.to_string())))?;
    } else {
        sqlx::query(
            "INSERT INTO source_documents
                (id, knowledge_base_id, display_name, source_kind, source_path,
                 storage_path, content_sha256, file_size)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(document_id)
        .bind(request.knowledge_base_id)
        .bind(&display_name)
        .bind(source.format.as_str())
        .bind(&source_path)
        .bind(&storage_path)
        .bind(&source.content_sha256)
        .bind(source.byte_len as i64)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "写入 PostgreSQL 源文档失败: {}",
                safe_error(&error.to_string())
            )
        })?;
    }
    let version_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO document_versions
            (id, document_id, content_sha256, mime_type, parser, parser_version,
             chunk_policy_version, embedding_profile_id, embedding_model_id,
             expected_asset_count, expected_chunk_count, manifest_sealed, activated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, FALSE, now())",
    )
    .bind(version_id)
    .bind(document_id)
    .bind(&source.content_sha256)
    .bind(&source.mime_type)
    .bind("suna")
    .bind("v1")
    .bind(ChunkPolicy::default().version)
    .bind(
        embedding_profile_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
    )
    .bind(&embedding_model_id)
    .bind(i64::try_from(parsed.assets.len()).map_err(|_| "资产数量过大")?)
    .bind(i64::try_from(chunks.len()).map_err(|_| "Chunk 数量过大")?)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "写入 PostgreSQL 文档版本失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    for asset in &parsed.assets {
        let object = asset_store
            .put(&asset.bytes)
            .map_err(|error| format!("保存 PostgreSQL 文档资产失败: {error}"))?;
        sqlx::query(
            "INSERT INTO document_assets
                (id, version_id, asset_kind, local_path, content_sha256, mime_type, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(version_id)
        .bind(&asset.kind)
        .bind(object.storage_key())
        .bind(object.sha256())
        .bind(&asset.media_type)
        .bind(
            serde_json::json!({ "original_name": asset.original_name, "location": asset.location }),
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "写入 PostgreSQL 文档资产失败: {}",
                safe_error(&error.to_string())
            )
        })?;
    }
    let mut chunk_ids = Vec::with_capacity(chunks.len());
    let mut parent_by_location: Vec<(serde_json::Value, Uuid)> = Vec::new();
    for chunk in &chunks {
        let location =
            serde_json::to_value(&chunk.source_location).map_err(|error| error.to_string())?;
        let chunk_id = Uuid::new_v4();
        let parent_id = parent_by_location
            .iter()
            .find(|(known, _)| *known == location)
            .map(|(_, id)| *id);
        if parent_id.is_none() {
            parent_by_location.push((location.clone(), chunk_id));
        }
        chunk_ids.push((chunk_id, parent_id));
    }
    for (chunk, (chunk_id, parent_id)) in chunks.iter().zip(chunk_ids) {
        let location =
            serde_json::to_value(&chunk.source_location).map_err(|error| error.to_string())?;
        sqlx::query(
            "INSERT INTO document_chunks
                (id, version_id, parent_id, ordinal, title_path, text, source_location)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(chunk_id)
        .bind(version_id)
        .bind(parent_id)
        .bind(chunk.ordinal as i32)
        .bind(source_location_title_path(&chunk.source_location))
        .bind(&chunk.text)
        .bind(location)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "写入 PostgreSQL Chunk 失败: {}",
                safe_error(&error.to_string())
            )
        })?;
    }
    sqlx::query(
        "UPDATE ingestion_jobs
         SET source_document_id = $1, state = 'completed', progress = 100, updated_at = now()
         WHERE id = $2",
    )
    .bind(document_id)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "更新 PostgreSQL 导入任务失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    sqlx::query(
        "UPDATE ingestion_attempts SET state = 'completed', finished_at = now()
         WHERE id = $1 AND job_id = $2",
    )
    .bind(attempt_id)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "写入 PostgreSQL 导入尝试失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    import_failure_guard.disarm();
    if let Some(embedding_profile_id) = embedding_profile_id {
        update_ingestion_stage_or_fail(&pool, job_id, attempt_id, "embedding").await?;
        let db = app.state::<DbState>();
        let secrets = app.state::<SecretState>();
        match embed_postgres_document_inner(
            db.inner(),
            secrets.inner(),
            state,
            PostgresEmbeddingRequest {
                version_id,
                embedding_profile_id,
            },
        )
        .await
        {
            Ok(indexed_chunks) => {
                let manifest = sqlx::query(
                    "SELECT MIN(model_id) AS model_id, MIN(dimension) AS dimension,
                            COUNT(*)::bigint AS indexed_count
                     FROM chunk_embeddings e
                     JOIN document_chunks c ON c.id = e.chunk_id
                     WHERE c.version_id = $1",
                )
                .bind(version_id)
                .fetch_one(&pool)
                .await
                .map_err(|error| {
                    format!(
                        "读取 Embedding manifest 失败: {}",
                        safe_error(&error.to_string())
                    )
                })?;
                let model_id: String = manifest
                    .try_get("model_id")
                    .map_err(|error| error.to_string())?;
                let dimension: i32 = manifest
                    .try_get("dimension")
                    .map_err(|error| error.to_string())?;
                let indexed_count: i64 = manifest
                    .try_get("indexed_count")
                    .map_err(|error| error.to_string())?;
                if indexed_count != i64::from(indexed_chunks) || dimension <= 0 {
                    let error = "Embedding manifest 与 Chunk 数量不一致".to_string();
                    record_postgres_ingestion_failure(
                        &pool,
                        job_id,
                        attempt_id,
                        "embedding_manifest_incomplete",
                        &error,
                    )
                    .await?;
                    return Err(error);
                }
                sqlx::query(
                    "UPDATE document_versions
                     SET embedding_model_id = $1, embedding_dimension = $2,
                         manifest_sealed = TRUE
                     WHERE id = $3",
                )
                .bind(model_id)
                .bind(dimension)
                .bind(version_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    format!(
                        "封存 Embedding manifest 失败: {}",
                        safe_error(&error.to_string())
                    )
                })?;
                update_ingestion_stage_or_fail(&pool, job_id, attempt_id, "completed").await?;
            }
            Err(error) => {
                record_postgres_ingestion_failure(
                    &pool,
                    job_id,
                    attempt_id,
                    "embedding_failed",
                    &error,
                )
                .await?;
                return Err(error);
            }
        }
    } else {
        update_ingestion_stage_or_fail(&pool, job_id, attempt_id, "completed").await?;
    }
    Ok(PostgresDocumentImportResponse {
        task_id: job_id,
        knowledge_base_id: request.knowledge_base_id,
        document_id,
        version_id,
        chunk_count: chunks.len() as u32,
        asset_count: parsed.assets.len() as u32,
        duplicate_content: false,
    })
}

/// Internal adapter used by the loopback Knowledge REST API. It keeps the
/// parser/chunker/task pipeline identical to the Tauri command path.
pub(crate) async fn import_postgres_document_for_api(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    request: PostgresDocumentImportRequest,
) -> Result<PostgresDocumentQueuedResponse, String> {
    queue_postgres_document_import(app, state, request).await
}

#[tauri::command]
pub async fn import_postgres_directory(
    app: tauri::AppHandle,
    state: tauri::State<'_, KnowledgeDatabaseState>,
    knowledge_base_id: Uuid,
    directory_path: std::path::PathBuf,
) -> Result<Vec<PostgresDocumentQueuedResponse>, String> {
    if !directory_path.is_dir() {
        return Err("导入路径不是文件夹".to_string());
    }
    let pool = active_pool(&state)?;
    ensure_private_knowledge_base(&pool, knowledge_base_id).await?;
    let mut pending = vec![directory_path];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path).map_err(|error| format!("读取文件夹失败: {error}"))?
        {
            let entry = entry.map_err(|error| format!("读取文件夹条目失败: {error}"))?;
            let child = entry.path();
            if child.is_dir() {
                pending.push(child);
            } else if child.is_file() {
                let supported = child
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| {
                        matches!(
                            value.to_ascii_lowercase().as_str(),
                            "pdf"
                                | "docx"
                                | "xls"
                                | "xlsx"
                                | "csv"
                                | "txt"
                                | "md"
                                | "markdown"
                                | "json"
                                | "html"
                                | "htm"
                        )
                    })
                    .unwrap_or(false);
                if supported {
                    files.push(child);
                }
            }
        }
    }
    let mut results = Vec::new();
    for source_path in files {
        match queue_postgres_document_import(
            &app,
            &state,
            PostgresDocumentImportRequest {
                knowledge_base_id,
                source_path,
                embedding_profile_id: None,
            },
        )
        .await
        {
            Ok(response) => results.push(response),
            Err(_) => {
                // Each file owns its task and failure record; continue the
                // folder batch so one bad document cannot hide the rest.
            }
        }
    }
    Ok(results)
}

async fn update_ingestion_stage(pool: &PgPool, job_id: Uuid, stage: &str) -> Result<(), String> {
    sqlx::query(
        "UPDATE ingestion_jobs
         SET state = $1,
             progress = CASE $1
                 WHEN 'queued' THEN 8
                 WHEN 'uploading' THEN 20
                 WHEN 'parsing' THEN 42
                 WHEN 'chunking' THEN 62
                 WHEN 'embedding' THEN 80
                 WHEN 'indexing' THEN 92
                 WHEN 'running' THEN 55
                 WHEN 'completed' THEN 100
                 WHEN 'failed' THEN 100
                 WHEN 'cancelled' THEN 100
                 ELSE progress
             END,
             updated_at = now()
         WHERE id = $2",
    )
    .bind(stage)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(|error| format!("更新导入阶段失败: {}", safe_error(&error.to_string())))?;
    Ok(())
}

async fn update_ingestion_stage_or_fail(
    pool: &PgPool,
    job_id: Uuid,
    attempt_id: Uuid,
    stage: &str,
) -> Result<(), String> {
    match update_ingestion_stage(pool, job_id, stage).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let record_error = record_postgres_ingestion_failure(
                pool,
                job_id,
                attempt_id,
                "stage_update_failed",
                &error,
            )
            .await;
            match record_error {
                Ok(()) => Err(error),
                Err(record_error) => Err(format!("{error}; {record_error}")),
            }
        }
    }
}

/// Covers unexpected errors after the task has entered indexing.  The normal
/// parser/chunker branches record their exact error at the point of failure;
/// this guard prevents storage or transaction errors from leaving a job stuck
/// in an active state.
struct ImportFailureGuard {
    pool: PgPool,
    job_id: Uuid,
    attempt_id: Uuid,
    active: bool,
}

impl ImportFailureGuard {
    fn new(pool: &PgPool, job_id: Uuid, attempt_id: Uuid) -> Self {
        Self {
            pool: pool.clone(),
            job_id,
            attempt_id,
            active: true,
        }
    }

    fn disarm(mut self) {
        self.active = false;
    }
}

impl Drop for ImportFailureGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let pool = self.pool.clone();
        let job_id = self.job_id;
        let attempt_id = self.attempt_id;
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = record_postgres_ingestion_failure(
                    &pool,
                    job_id,
                    attempt_id,
                    "indexing_failed",
                    "文档索引阶段失败",
                )
                .await;
            });
        }
    }
}

async fn record_postgres_ingestion_failure(
    pool: &PgPool,
    job_id: Uuid,
    attempt_id: Uuid,
    error_code: &str,
    error_message: &str,
) -> Result<(), String> {
    let mut transaction = pool.begin().await.map_err(|error| {
        format!(
            "记录导入失败时开启事务失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let context = sqlx::query(
        "SELECT knowledge_base_id, source_document_id
         FROM ingestion_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("读取导入失败上下文失败: {}", safe_error(&error.to_string())))?;
    let knowledge_base_id = context
        .as_ref()
        .and_then(|row| row.try_get::<Uuid, _>("knowledge_base_id").ok());
    let document_id = context
        .as_ref()
        .and_then(|row| row.try_get::<Option<Uuid>, _>("source_document_id").ok())
        .flatten();
    sqlx::query(
        "UPDATE ingestion_jobs
         SET state = 'failed', progress = 100, error_message = $1, updated_at = now()
         WHERE id = $2",
    )
    .bind(error_message)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("记录导入失败状态失败: {}", safe_error(&error.to_string())))?;
    sqlx::query(
        "UPDATE ingestion_attempts
         SET state = 'failed', error_message = $1, finished_at = now()
         WHERE id = $2 AND job_id = $3",
    )
    .bind(error_message)
    .bind(attempt_id)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("记录导入失败尝试失败: {}", safe_error(&error.to_string())))?;
    sqlx::query(
        "INSERT INTO processing_failures
            (id, job_id, error_code, error_message, details)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(job_id)
    .bind(error_code)
    .bind(error_message)
    .bind(serde_json::json!({
        "stage": error_code,
        "task_id": job_id,
        "knowledge_base_id": knowledge_base_id,
        "document_id": document_id,
        "operation": "document_import",
        "status": "failed",
        "error": error_message
    }))
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("记录处理失败详情失败: {}", safe_error(&error.to_string())))?;
    transaction
        .commit()
        .await
        .map_err(|error| format!("提交导入失败记录失败: {}", safe_error(&error.to_string())))
}

fn source_location_title_path(location: &crate::rag::model::SourceLocation) -> String {
    match location {
        crate::rag::model::SourceLocation::Heading { path } => path.join(" / "),
        crate::rag::model::SourceLocation::PdfPage { page, .. } => format!("PDF page {page}"),
        crate::rag::model::SourceLocation::SheetRange { sheet, range } => {
            format!("{sheet}!{range}")
        }
        crate::rag::model::SourceLocation::TextOffsets { start, end } => {
            format!("offsets {start}-{end}")
        }
    }
}

#[tauri::command]
pub async fn search_postgres_knowledge(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    request: PostgresKnowledgeSearchRequest,
) -> Result<Vec<PostgresKnowledgeSearchHit>, String> {
    let started_at = Instant::now();
    let query = request.query.trim();
    if query.is_empty() {
        return Err("检索内容不能为空".to_string());
    }
    let limit = request.limit.clamp(1, 100);
    let filters = request.filters.unwrap_or_default();
    let document_type = filters
        .document_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let material = filters
        .material
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let process = filters
        .process
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let property = filters
        .property
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let tag = filters
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT c.id, c.version_id, d.id AS document_id, d.knowledge_base_id, d.display_name,
                c.text, c.source_location,
                ts_rank_cd(c.search_vector, plainto_tsquery('simple', $2))::real AS rank
         FROM document_chunks c
         JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
         JOIN source_documents d ON d.id = v.document_id
         JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE d.knowledge_base_id = $1
           AND d.deleted_at IS NULL
           AND c.search_vector @@ plainto_tsquery('simple', $2)
           AND ($3::text IS NULL OR d.source_kind = $3)
           AND ($4::text IS NULL OR lower(coalesce(d.metadata ->> 'material', '')) LIKE lower('%' || $4 || '%'))
           AND ($5::text IS NULL OR lower(coalesce(d.metadata ->> 'process', '')) LIKE lower('%' || $5 || '%'))
           AND ($6::text IS NULL OR lower(coalesce(d.metadata ->> 'property', '')) LIKE lower('%' || $6 || '%'))
           AND ($7::int IS NULL OR d.metadata ->> 'year' = $7::text)
           AND ($8::text IS NULL OR (d.metadata -> 'tags') ? $8 OR lower(coalesce(d.metadata ->> 'tags', '')) LIKE lower('%' || $8 || '%'))
         ORDER BY rank DESC, c.ordinal
         LIMIT $9",
    )
    .bind(request.knowledge_base_id)
    .bind(query)
    .bind(document_type)
    .bind(material)
    .bind(process)
    .bind(property)
    .bind(filters.year)
    .bind(tag)
    .bind(i64::from(limit))
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "PostgreSQL 全文检索失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let hits = rows
        .into_iter()
        .map(|row| {
            Ok(PostgresKnowledgeSearchHit {
                knowledge_base_id: row
                    .try_get("knowledge_base_id")
                    .map_err(|error| error.to_string())?,
                chunk_id: row.try_get("id").map_err(|error| error.to_string())?,
                version_id: row
                    .try_get("version_id")
                    .map_err(|error| error.to_string())?,
                document_id: row
                    .try_get("document_id")
                    .map_err(|error| error.to_string())?,
                document_name: row
                    .try_get("display_name")
                    .map_err(|error| error.to_string())?,
                text: row.try_get("text").map_err(|error| error.to_string())?,
                source_location: row
                    .try_get("source_location")
                    .map_err(|error| error.to_string())?,
                rank: row.try_get("rank").map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    sqlx::query(
        "INSERT INTO retrieval_audits (id, knowledge_base_id, query, configuration, evidence, duration_ms)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(request.knowledge_base_id)
    .bind(query)
    .bind(serde_json::json!({ "mode": "full_text", "limit": limit, "filters": filters }))
    .bind(serde_json::to_value(&hits).map_err(|error| error.to_string())?)
    .bind(i64::try_from(started_at.elapsed().as_millis()).unwrap_or(i64::MAX))
    .execute(&pool)
    .await
    .map_err(|error| format!("写入检索审计失败: {}", safe_error(&error.to_string())))?;
    Ok(hits)
}

#[tauri::command]
pub async fn store_postgres_chunk_embeddings(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    model_id: String,
    embeddings: Vec<PostgresChunkEmbeddingInput>,
) -> Result<u32, String> {
    if model_id.trim().is_empty() || embeddings.is_empty() {
        return Err("Embedding 模型和向量不能为空".to_string());
    }
    let dimension = validate_embedding_batch(&embeddings)?;
    let pool = active_pool(&state)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    for item in &embeddings {
        let vector = encode_pgvector(&item.embedding)?;
        let scope = sqlx::query(
            "SELECT d.knowledge_base_id, d.id AS document_id, c.source_location,
                    kb.visibility
             FROM document_chunks c
             JOIN document_versions v ON v.id = c.version_id
             JOIN source_documents d ON d.id = v.document_id
             JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id
             WHERE c.id = $1 AND d.deleted_at IS NULL AND kb.deleted_at IS NULL
             FOR KEY SHARE",
        )
        .bind(item.chunk_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "检查 Embedding Chunk 权限失败: {}",
                safe_error(&error.to_string())
            )
        })?
        .ok_or_else(|| "Embedding Chunk 不存在".to_string())?;
        let visibility: String = scope
            .try_get("visibility")
            .map_err(|error| error.to_string())?;
        if visibility != "private" {
            return Err("公共知识库为只读，单人客户端不能写入 Embedding".to_string());
        }
        let knowledge_base_id: Uuid = scope
            .try_get("knowledge_base_id")
            .map_err(|error| error.to_string())?;
        let document_id: Uuid = scope
            .try_get("document_id")
            .map_err(|error| error.to_string())?;
        let source_location: serde_json::Value = scope
            .try_get("source_location")
            .map_err(|error| error.to_string())?;
        sqlx::query(
            "INSERT INTO chunk_embeddings
                (chunk_id, knowledge_base_id, document_id, model_id, dimension, embedding, metadata)
             VALUES ($1, $2, $3, $4, $5, $6::vector, $7)
             ON CONFLICT (chunk_id) DO UPDATE SET
                knowledge_base_id = EXCLUDED.knowledge_base_id,
                document_id = EXCLUDED.document_id,
                model_id = EXCLUDED.model_id,
                dimension = EXCLUDED.dimension,
                embedding = EXCLUDED.embedding,
                metadata = EXCLUDED.metadata,
                created_at = now()",
        )
        .bind(item.chunk_id)
        .bind(knowledge_base_id)
        .bind(document_id)
        .bind(model_id.trim())
        .bind(dimension)
        .bind(vector)
        .bind(serde_json::json!({ "source_location": source_location }))
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("写入 pgvector 失败: {}", safe_error(&error.to_string())))?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(embeddings.len() as u32)
}

#[tauri::command]
pub async fn embed_postgres_document(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    state: tauri::State<'_, KnowledgeDatabaseState>,
    request: PostgresEmbeddingRequest,
) -> Result<u32, String> {
    embed_postgres_document_inner(db.inner(), secrets.inner(), state.inner(), request).await
}

pub(crate) async fn embed_postgres_document_for_api(
    db: &DbState,
    secrets: &SecretState,
    state: &KnowledgeDatabaseState,
    request: PostgresEmbeddingRequest,
) -> Result<u32, String> {
    embed_postgres_document_inner(db, secrets, state, request).await
}

async fn embed_postgres_document_inner(
    db: &DbState,
    secrets: &SecretState,
    state: &KnowledgeDatabaseState,
    request: PostgresEmbeddingRequest,
) -> Result<u32, String> {
    let record = with_conn_ref(db, |connection| {
        provider_profiles::get_record(
            connection,
            current_workspace_id(),
            request.embedding_profile_id,
        )
    })?
    .ok_or_else(|| "Embedding provider 不存在".to_string())?;
    if record.profile.kind != ProviderKind::SiliconFlow || !record.profile.enabled {
        return Err("当前 Embedding provider 不可用".to_string());
    }
    let model = record
        .profile
        .model_id
        .clone()
        .ok_or_else(|| "Embedding 模型未配置".to_string())?;
    let credential = provider_credential(&record, &secrets)?;
    let provider = configured_embedding_provider(
        record.profile.clone(),
        Some(credential),
        SiliconFlowPlan::Free,
        Some(model),
    )
    .map_err(|error| error.to_string())?;
    let pool = pool_for_query(state)?;
    let knowledge_base_id: Uuid = sqlx::query_scalar(
        "SELECT d.knowledge_base_id
         FROM document_versions v
         JOIN source_documents d ON d.id = v.document_id
         WHERE v.id = $1 AND d.deleted_at IS NULL",
    )
    .bind(request.version_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 Embedding 所属知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .ok_or_else(|| "Embedding 版本不存在".to_string())?;
    ensure_private_knowledge_base(&pool, knowledge_base_id).await?;
    let rows = sqlx::query(
        "SELECT id, text, source_location, version_id
         FROM document_chunks
         WHERE version_id = $1 ORDER BY ordinal",
    )
    .bind(request.version_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取待向量化 Chunk 失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    if rows.is_empty() {
        return Ok(0);
    }
    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM document_versions WHERE id = $1")
            .bind(request.version_id)
            .fetch_one(&pool)
            .await
            .map_err(|error| {
                format!(
                    "读取 Embedding 文档归属失败: {}",
                    safe_error(&error.to_string())
                )
            })?;
    let chunk_ids = rows
        .iter()
        .map(|row| {
            row.try_get::<Uuid, _>("id")
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let texts = rows
        .iter()
        .map(|row| {
            row.try_get::<String, _>("text")
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let response = provider
        .embed(texts)
        .await
        .map_err(|error| format!("Embedding provider 调用失败: {}", error))?;
    if response.vectors.len() != chunk_ids.len() {
        return Err("Embedding provider 返回数量与 Chunk 数量不一致".to_string());
    }
    if response.model_id.trim().is_empty() {
        return Err("Embedding provider 未返回模型标识".to_string());
    }
    let inputs = chunk_ids
        .into_iter()
        .zip(response.vectors)
        .map(|(chunk_id, embedding)| PostgresChunkEmbeddingInput {
            chunk_id,
            model_id: response.model_id.clone(),
            embedding,
        })
        .collect::<Vec<_>>();
    let dimension = validate_embedding_batch(&inputs)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    for (item, row) in inputs.iter().zip(rows.iter()) {
        let vector = encode_pgvector(&item.embedding)?;
        let source_location: serde_json::Value = row
            .try_get("source_location")
            .map_err(|error| error.to_string())?;
        sqlx::query(
            "INSERT INTO chunk_embeddings
                (chunk_id, knowledge_base_id, document_id, model_id, dimension, embedding, metadata)
             VALUES ($1, $2, $3, $4, $5, $6::vector, $7)
             ON CONFLICT (chunk_id) DO UPDATE SET
                knowledge_base_id = EXCLUDED.knowledge_base_id,
                document_id = EXCLUDED.document_id,
                model_id = EXCLUDED.model_id,
                dimension = EXCLUDED.dimension,
                embedding = EXCLUDED.embedding,
                metadata = EXCLUDED.metadata,
                created_at = now()",
        )
        .bind(item.chunk_id)
        .bind(knowledge_base_id)
        .bind(document_id)
        .bind(&item.model_id)
        .bind(dimension)
        .bind(vector)
        .bind(serde_json::json!({ "source_location": source_location }))
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("写入 pgvector 失败: {}", safe_error(&error.to_string())))?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(inputs.len() as u32)
}

#[tauri::command]
pub async fn search_postgres_vectors(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    request: PostgresVectorSearchRequest,
) -> Result<Vec<PostgresKnowledgeSearchHit>, String> {
    let vector = encode_pgvector(&request.embedding)?;
    let limit = request.limit.clamp(1, 100);
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT c.id, c.version_id, d.id AS document_id, d.knowledge_base_id, d.display_name,
                c.text, c.source_location,
                (1 - (e.embedding <=> $2::vector))::real AS rank
         FROM chunk_embeddings e
         JOIN document_chunks c ON c.id = e.chunk_id
         JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
         JOIN source_documents d ON d.id = v.document_id
         JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL AND v.activated_at IS NOT NULL
           AND e.knowledge_base_id = d.knowledge_base_id
           AND e.dimension = $3
           AND e.model_id = v.embedding_model_id
           AND v.manifest_sealed = TRUE
           AND v.id = (SELECT av.id FROM document_versions av WHERE av.document_id = d.id AND av.activated_at IS NOT NULL ORDER BY av.activated_at DESC LIMIT 1)
         ORDER BY e.embedding <=> $2::vector, c.ordinal
         LIMIT $4",
    )
    .bind(request.knowledge_base_id)
    .bind(vector)
    .bind(i32::try_from(request.embedding.len()).map_err(|_| "Embedding 维度过大")?)
    .bind(i64::from(limit))
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "PostgreSQL 向量检索失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresKnowledgeSearchHit {
                knowledge_base_id: row
                    .try_get("knowledge_base_id")
                    .map_err(|error| error.to_string())?,
                chunk_id: row.try_get("id").map_err(|error| error.to_string())?,
                version_id: row
                    .try_get("version_id")
                    .map_err(|error| error.to_string())?,
                document_id: row
                    .try_get("document_id")
                    .map_err(|error| error.to_string())?,
                document_name: row
                    .try_get("display_name")
                    .map_err(|error| error.to_string())?,
                text: row.try_get("text").map_err(|error| error.to_string())?,
                source_location: row
                    .try_get("source_location")
                    .map_err(|error| error.to_string())?,
                rank: row.try_get("rank").map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()
}

#[tauri::command]
pub async fn search_postgres_hybrid(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    request: PostgresHybridSearchRequest,
) -> Result<Vec<PostgresKnowledgeSearchHit>, String> {
    let started_at = Instant::now();
    let query = request.query.trim();
    if query.is_empty() {
        return Err("检索内容不能为空".to_string());
    }
    let vector = encode_pgvector(&request.embedding)?;
    let limit = request.limit.clamp(1, 100);
    let rrf_k = request.rrf_k.clamp(1, 10_000);
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "WITH lexical AS (
            SELECT c.id,
                   ROW_NUMBER() OVER (
                     ORDER BY ts_rank_cd(c.search_vector, plainto_tsquery('simple', $2)) DESC,
                              c.ordinal
                   ) AS position
            FROM document_chunks c
            JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
            JOIN source_documents d ON d.id = v.document_id
            JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id AND kb.deleted_at IS NULL
            WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
              AND c.search_vector @@ plainto_tsquery('simple', $2)
            ORDER BY position
            LIMIT $4
         ), dense AS (
            SELECT c.id,
                   ROW_NUMBER() OVER (ORDER BY e.embedding <=> $3::vector, c.ordinal) AS position
            FROM chunk_embeddings e
            JOIN document_chunks c ON c.id = e.chunk_id
            JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
            JOIN source_documents d ON d.id = v.document_id
            JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id AND kb.deleted_at IS NULL
            WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
              AND e.knowledge_base_id = d.knowledge_base_id
              AND e.dimension = $6
              AND e.model_id = v.embedding_model_id
              AND v.manifest_sealed = TRUE
            ORDER BY position
            LIMIT $4
         ), scored AS (
            SELECT COALESCE(lexical.id, dense.id) AS id,
                   COALESCE(1.0 / ($5::double precision + lexical.position), 0.0)
                     + COALESCE(1.0 / ($5::double precision + dense.position), 0.0) AS rank
            FROM lexical FULL OUTER JOIN dense ON dense.id = lexical.id
         )
         SELECT c.id, c.version_id, d.id AS document_id, d.knowledge_base_id, d.display_name,
                c.text, c.source_location, scored.rank::real AS rank
         FROM scored
         JOIN document_chunks c ON c.id = scored.id
         JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
         JOIN source_documents d ON d.id = v.document_id
         ORDER BY scored.rank DESC, c.ordinal
         LIMIT $4",
    )
    .bind(request.knowledge_base_id)
    .bind(query)
    .bind(vector)
    .bind(i64::from(limit))
    .bind(rrf_k as f64)
    .bind(i32::try_from(request.embedding.len()).map_err(|_| "Embedding 维度过大")?)
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "PostgreSQL 混合检索失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let hits = rows
        .into_iter()
        .map(|row| {
            Ok(PostgresKnowledgeSearchHit {
                knowledge_base_id: row
                    .try_get("knowledge_base_id")
                    .map_err(|error| error.to_string())?,
                chunk_id: row.try_get("id").map_err(|error| error.to_string())?,
                version_id: row
                    .try_get("version_id")
                    .map_err(|error| error.to_string())?,
                document_id: row
                    .try_get("document_id")
                    .map_err(|error| error.to_string())?,
                document_name: row
                    .try_get("display_name")
                    .map_err(|error| error.to_string())?,
                text: row.try_get("text").map_err(|error| error.to_string())?,
                source_location: row
                    .try_get("source_location")
                    .map_err(|error| error.to_string())?,
                rank: row.try_get("rank").map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    sqlx::query(
        "INSERT INTO retrieval_audits (id, knowledge_base_id, query, configuration, evidence, duration_ms)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(request.knowledge_base_id)
    .bind(query)
    .bind(serde_json::json!({ "mode": "hybrid", "limit": limit, "rrf_k": rrf_k }))
    .bind(serde_json::to_value(&hits).map_err(|error| error.to_string())?)
    .bind(i64::try_from(started_at.elapsed().as_millis()).unwrap_or(i64::MAX))
    .execute(&pool)
    .await
    .map_err(|error| format!("写入混合检索审计失败: {}", safe_error(&error.to_string())))?;
    Ok(hits)
}

pub(crate) async fn search_postgres_hybrid_pool(
    pool: &PgPool,
    knowledge_base_id: Uuid,
    query: &str,
    embedding: &[f32],
    limit: usize,
    rrf_k: u32,
    filters: Option<&PostgresKnowledgeSearchFilters>,
) -> Result<Vec<PostgresKnowledgeSearchHit>, String> {
    let vector = encode_pgvector(embedding)?;
    let limit = limit.clamp(1, 100);
    let rrf_k = rrf_k.clamp(1, 10_000);
    let filters = filters.cloned().unwrap_or_default();
    let document_type = filters
        .document_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let material = filters
        .material
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let process = filters
        .process
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let property = filters
        .property
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let tag = filters
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let year = filters.year;
    let rows = sqlx::query(
        "WITH lexical AS (
            SELECT c.id, ROW_NUMBER() OVER (
              ORDER BY ts_rank_cd(c.search_vector, plainto_tsquery('simple', $2)) DESC, c.ordinal
            ) AS position
            FROM document_chunks c
            JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
            JOIN source_documents d ON d.id = v.document_id
            JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id AND kb.deleted_at IS NULL
            WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
              AND ($7::text IS NULL OR d.source_kind = $7)
              AND ($8::text IS NULL OR lower(coalesce(d.metadata ->> 'material', '')) LIKE lower('%' || $8 || '%'))
              AND ($9::text IS NULL OR lower(coalesce(d.metadata ->> 'process', '')) LIKE lower('%' || $9 || '%'))
              AND ($10::text IS NULL OR lower(coalesce(d.metadata ->> 'property', '')) LIKE lower('%' || $10 || '%'))
              AND ($11::text IS NULL OR (d.metadata -> 'tags') ? $11 OR lower(coalesce(d.metadata ->> 'tags', '')) LIKE lower('%' || $11 || '%'))
              AND ($12::int IS NULL OR d.metadata ->> 'year' = $12::text)
              AND c.search_vector @@ plainto_tsquery('simple', $2)
            ORDER BY position
            LIMIT $4
         ), dense AS (
            SELECT c.id, ROW_NUMBER() OVER (ORDER BY e.embedding <=> $3::vector, c.ordinal) AS position
            FROM chunk_embeddings e
            JOIN document_chunks c ON c.id = e.chunk_id
            JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
            JOIN source_documents d ON d.id = v.document_id
            JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id AND kb.deleted_at IS NULL
            WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
              AND ($7::text IS NULL OR d.source_kind = $7)
              AND ($8::text IS NULL OR lower(coalesce(d.metadata ->> 'material', '')) LIKE lower('%' || $8 || '%'))
              AND ($9::text IS NULL OR lower(coalesce(d.metadata ->> 'process', '')) LIKE lower('%' || $9 || '%'))
              AND ($10::text IS NULL OR lower(coalesce(d.metadata ->> 'property', '')) LIKE lower('%' || $10 || '%'))
              AND ($11::text IS NULL OR (d.metadata -> 'tags') ? $11 OR lower(coalesce(d.metadata ->> 'tags', '')) LIKE lower('%' || $11 || '%'))
              AND ($12::int IS NULL OR d.metadata ->> 'year' = $12::text)
              AND e.knowledge_base_id = d.knowledge_base_id
              AND e.dimension = $6
              AND e.model_id = v.embedding_model_id
              AND v.manifest_sealed = TRUE
            ORDER BY position
            LIMIT $4
         ), scored AS (
            SELECT COALESCE(lexical.id, dense.id) AS id,
              COALESCE(1.0 / ($5::double precision + lexical.position), 0.0)
              + COALESCE(1.0 / ($5::double precision + dense.position), 0.0) AS rank
            FROM lexical FULL OUTER JOIN dense ON dense.id = lexical.id
         )
         SELECT c.id, c.version_id, d.id AS document_id, d.knowledge_base_id, d.display_name,
                c.text, c.source_location, scored.rank::real AS rank
         FROM scored
         JOIN document_chunks c ON c.id = scored.id
         JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
         JOIN source_documents d ON d.id = v.document_id
         ORDER BY scored.rank DESC, c.ordinal LIMIT $4",
    )
    .bind(knowledge_base_id)
    .bind(query)
    .bind(vector)
    .bind(i64::try_from(limit).map_err(|_| "检索数量无效")?)
    .bind(rrf_k as f64)
    .bind(i32::try_from(embedding.len()).map_err(|_| "Embedding 维度过大")?)
    .bind(document_type)
    .bind(material)
    .bind(process)
    .bind(property)
    .bind(tag)
    .bind(year)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("PostgreSQL 混合检索失败: {}", safe_error(&error.to_string())))?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresKnowledgeSearchHit {
                knowledge_base_id: row
                    .try_get("knowledge_base_id")
                    .map_err(|error| error.to_string())?,
                chunk_id: row.try_get("id").map_err(|error| error.to_string())?,
                version_id: row
                    .try_get("version_id")
                    .map_err(|error| error.to_string())?,
                document_id: row
                    .try_get("document_id")
                    .map_err(|error| error.to_string())?,
                document_name: row
                    .try_get("display_name")
                    .map_err(|error| error.to_string())?,
                text: row.try_get("text").map_err(|error| error.to_string())?,
                source_location: row
                    .try_get("source_location")
                    .map_err(|error| error.to_string())?,
                rank: row.try_get("rank").map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn resolve_postgres_citation(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    audit_id: Uuid,
    citation_number: u32,
) -> Result<Option<PostgresCitation>, String> {
    if citation_number == 0 {
        return Err("引用序号必须从 1 开始".to_string());
    }
    let pool = active_pool(&state)?;
    let evidence: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT evidence -> ($2 - 1) FROM retrieval_audits WHERE id = $1")
            .bind(audit_id)
            .bind(i64::from(citation_number))
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!(
                    "读取 PostgreSQL citation 失败: {}",
                    safe_error(&error.to_string())
                )
            })?;
    let Some(evidence) = evidence else {
        return Ok(None);
    };
    if evidence.is_null() {
        return Ok(None);
    }
    let hit: PostgresKnowledgeSearchHit = serde_json::from_value(evidence.clone())
        .or_else(|_| {
            evidence
                .get("chunk")
                .cloned()
                .ok_or_else(|| {
                    serde_json::Error::io(std::io::Error::other("citation chunk missing"))
                })
                .and_then(serde_json::from_value)
        })
        .map_err(|error| format!("解析 citation 失败: {error}"))?;
    let source_state: String = sqlx::query_scalar(
        "SELECT CASE
            WHEN d.deleted_at IS NOT NULL THEN 'deleted'
            WHEN v.activated_at IS NULL THEN 'inactive'
            ELSE 'active'
          END
         FROM source_documents d
         JOIN document_versions v ON v.document_id = d.id
         WHERE d.id = $1 AND v.id = $2",
    )
    .bind(hit.document_id)
    .bind(hit.version_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 citation 来源状态失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .unwrap_or_else(|| "deleted".to_string());
    Ok(Some(PostgresCitation {
        audit_id,
        citation_number,
        source_state,
        hit,
    }))
}

#[tauri::command]
pub async fn rerank_postgres_knowledge(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    request: PostgresRerankRequest,
) -> Result<Vec<PostgresKnowledgeSearchHit>, String> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err("Reranker 查询不能为空".to_string());
    }
    if request.hits.is_empty() {
        return Ok(Vec::new());
    }
    let record = with_conn(&db, |connection| {
        provider_profiles::get_record(
            connection,
            current_workspace_id(),
            request.rerank_profile_id,
        )
    })?
    .ok_or_else(|| "Reranker provider 不存在".to_string())?;
    if record.profile.kind != ProviderKind::SiliconFlow || !record.profile.enabled {
        return Err("当前 Reranker provider 不可用".to_string());
    }
    let model = record
        .profile
        .model_id
        .clone()
        .ok_or_else(|| "Reranker 模型未配置".to_string())?;
    let provider = configured_rerank_provider(
        record.profile.clone(),
        Some(provider_credential(&record, &secrets)?),
        SiliconFlowPlan::Free,
        Some(model),
    )
    .map_err(|error| error.to_string())?;
    let documents = request
        .hits
        .iter()
        .map(|hit| crate::providers::capabilities::RerankDocument {
            id: hit.chunk_id.to_string(),
            text: hit.text.clone(),
        })
        .collect();
    let scores = provider
        .rerank(query.to_string(), documents)
        .await
        .map_err(|error| format!("Reranker 调用失败: {error}"))?;
    let mut by_id = request
        .hits
        .into_iter()
        .map(|hit| (hit.chunk_id.to_string(), hit))
        .collect::<std::collections::HashMap<_, _>>();
    let mut output = Vec::with_capacity(scores.len());
    for score in scores {
        if let Some(mut hit) = by_id.remove(&score.id) {
            hit.rank = score.score;
            output.push(hit);
        }
    }
    Ok(output)
}

#[tauri::command]
pub async fn list_postgres_ingestion_jobs(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    knowledge_base_id: Uuid,
) -> Result<Vec<PostgresIngestionJobRecord>, String> {
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT j.id, j.knowledge_base_id, j.source_document_id, j.state, j.progress, j.attempts,
                j.error_message, j.next_attempt_at::text, j.created_at::text, j.updated_at::text
         FROM ingestion_jobs j
         JOIN knowledge_bases kb ON kb.id = j.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE j.knowledge_base_id = $1
         ORDER BY j.created_at DESC, j.id LIMIT 200",
    )
    .bind(knowledge_base_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL 导入任务失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresIngestionJobRecord {
                id: row.try_get("id").map_err(|error| error.to_string())?,
                knowledge_base_id: row
                    .try_get("knowledge_base_id")
                    .map_err(|error| error.to_string())?,
                source_document_id: row
                    .try_get("source_document_id")
                    .map_err(|error| error.to_string())?,
                state: row.try_get("state").map_err(|error| error.to_string())?,
                progress: row.try_get("progress").map_err(|error| error.to_string())?,
                attempts: row.try_get("attempts").map_err(|error| error.to_string())?,
                error_message: row
                    .try_get("error_message")
                    .map_err(|error| error.to_string())?,
                next_attempt_at: row
                    .try_get("next_attempt_at")
                    .map_err(|error| error.to_string())?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|error| error.to_string())?,
                updated_at: row
                    .try_get("updated_at")
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn list_postgres_processing_failures(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    job_id: Uuid,
) -> Result<Vec<PostgresProcessingFailureRecord>, String> {
    let pool = active_pool(&state)?;
    sqlx::query(
        "SELECT f.id, f.job_id, f.error_code, f.error_message, f.details, f.created_at::text
         FROM processing_failures f
         JOIN ingestion_jobs j ON j.id = f.job_id
         JOIN knowledge_bases kb ON kb.id = j.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE f.job_id = $1 ORDER BY f.created_at DESC, f.id",
    )
    .bind(job_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("读取导入失败记录失败: {}", safe_error(&error.to_string())))?
    .into_iter()
    .map(|row| {
        Ok(PostgresProcessingFailureRecord {
            id: row.try_get("id").map_err(|error| error.to_string())?,
            job_id: row.try_get("job_id").map_err(|error| error.to_string())?,
            error_code: row
                .try_get("error_code")
                .map_err(|error| error.to_string())?,
            error_message: row
                .try_get("error_message")
                .map_err(|error| error.to_string())?,
            details: row.try_get("details").map_err(|error| error.to_string())?,
            created_at: row
                .try_get("created_at")
                .map_err(|error| error.to_string())?,
        })
    })
    .collect()
}

#[tauri::command]
pub async fn retry_postgres_ingestion_job(
    app: tauri::AppHandle,
    state: tauri::State<'_, KnowledgeDatabaseState>,
    job_id: Uuid,
) -> Result<PostgresIngestionJobRecord, String> {
    retry_postgres_ingestion_job_for_api(&app, state.inner(), job_id).await
}

pub(crate) async fn retry_postgres_ingestion_job_for_api(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    job_id: Uuid,
) -> Result<PostgresIngestionJobRecord, String> {
    let pool = pool_for_query(state)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let current = sqlx::query(
        "SELECT id, knowledge_base_id, source_document_id, state, progress, attempts,
                error_message, next_attempt_at::text, created_at::text, updated_at::text
         FROM ingestion_jobs WHERE id = $1 FOR UPDATE",
    )
    .bind(job_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("读取导入任务失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "导入任务不存在".to_string())?;
    let attempts: i32 = current
        .try_get("attempts")
        .map_err(|error| error.to_string())?;
    let source_document_id: Option<Uuid> = current
        .try_get("source_document_id")
        .map_err(|error| error.to_string())?;
    let knowledge_base_id: Uuid = current
        .try_get("knowledge_base_id")
        .map_err(|error| error.to_string())?;
    let visibility: Option<String> = sqlx::query_scalar(
        "SELECT visibility FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(knowledge_base_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("检查导入任务权限失败: {}", safe_error(&error.to_string())))?;
    if visibility.as_deref() != Some("private") {
        return Err("公共知识库为只读，单人客户端不能重试导入任务".to_string());
    }
    let source_path: Option<String> = if let Some(document_id) = source_document_id {
        sqlx::query_scalar(
            "SELECT source_path FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(document_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| safe_error(&error.to_string()))?
    } else {
        None
    };
    let next_attempts = attempts.saturating_add(1);
    let state_name = if next_attempts >= 3 {
        "quarantined"
    } else {
        "retrying"
    };
    sqlx::query(
        "UPDATE ingestion_jobs
         SET state = $1, progress = CASE WHEN $1 = 'quarantined' THEN 100 ELSE 30 END, attempts = $2,
             next_attempt_at = CASE WHEN $1 = 'quarantined' THEN NULL
                                    ELSE now() + (power(2::double precision, $2::double precision) * interval '1 minute') END,
             updated_at = now()
         WHERE id = $3",
    )
    .bind(state_name)
    .bind(next_attempts)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("更新导入重试状态失败: {}", safe_error(&error.to_string())))?;
    let retry_attempt_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ingestion_attempts (id, job_id, state, error_message)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(retry_attempt_id)
    .bind(job_id)
    .bind(state_name)
    .bind(
        current
            .try_get::<Option<String>, _>("error_message")
            .map_err(|error| error.to_string())?,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("记录重试尝试失败: {}", safe_error(&error.to_string())))?;
    let row = sqlx::query(
        "SELECT id, knowledge_base_id, source_document_id, state, progress, attempts,
                error_message, next_attempt_at::text, created_at::text, updated_at::text
         FROM ingestion_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    if state_name == "retrying" {
        if let (Some(source_document_id), Some(source_path)) = (source_document_id, source_path) {
            let pool_for_retry = pool_for_query(state)?;
            let retry_state = KnowledgeDatabaseState {
                pool: Mutex::new(Some(pool_for_retry.clone())),
            };
            let app_for_retry = (*app).clone();
            tokio::spawn(async move {
                let result = import_postgres_document_inner_with_job(
                    &app_for_retry,
                    &retry_state,
                    PostgresDocumentImportRequest {
                        knowledge_base_id,
                        source_path: source_path.into(),
                        embedding_profile_id: None,
                    },
                    Some((job_id, retry_attempt_id)),
                )
                .await;
                let (final_state, error_message) = match result {
                    Ok(_) => ("completed", None),
                    Err(error) => ("failed", Some(error)),
                };
                let _ = sqlx::query(
                    "UPDATE ingestion_jobs
                     SET state = $1, progress = 100, error_message = $2, next_attempt_at = NULL, updated_at = now()
                     WHERE id = $3",
                )
                .bind(final_state)
                .bind(&error_message)
                .bind(job_id)
                .execute(&pool_for_retry)
                .await;
                let _ = sqlx::query(
                    "UPDATE ingestion_attempts
                     SET state = $1, error_message = $2, finished_at = now()
                     WHERE id = $3 AND job_id = $4",
                )
                .bind(final_state)
                .bind(&error_message)
                .bind(retry_attempt_id)
                .bind(job_id)
                .execute(&pool_for_retry)
                .await;
                if let Some(error) = error_message {
                    let _ = sqlx::query(
                        "INSERT INTO processing_failures
                            (id, job_id, error_code, error_message, details)
                         VALUES ($1, $2, 'retry_failed', $3, $4)",
                    )
                    .bind(Uuid::new_v4())
                    .bind(job_id)
                    .bind(&error)
                    .bind(serde_json::json!({
                        "stage": "retry",
                        "task_id": job_id,
                        "knowledge_base_id": knowledge_base_id,
                        "document_id": source_document_id,
                        "operation": "document_import_retry",
                        "status": "failed",
                        "error": error
                    }))
                    .execute(&pool_for_retry)
                    .await;
                }
            });
        } else {
            let pool_for_retry = pool_for_query(state)?;
            sqlx::query(
                "UPDATE ingestion_jobs
                 SET state = 'failed', error_message = '源文件已不存在，无法重试',
                     next_attempt_at = NULL, updated_at = now()
                 WHERE id = $1",
            )
            .bind(job_id)
            .execute(&pool_for_retry)
            .await
            .map_err(|error| format!("更新不可重试任务失败: {}", safe_error(&error.to_string())))?;
            sqlx::query(
                "UPDATE ingestion_attempts
                 SET state = 'failed', error_message = '源文件已不存在，无法重试', finished_at = now()
                 WHERE id = $1 AND job_id = $2",
            )
            .bind(retry_attempt_id)
            .bind(job_id)
            .execute(&pool_for_retry)
            .await
            .map_err(|error| format!("记录不可重试尝试失败: {}", safe_error(&error.to_string())))?;
        }
    }
    Ok(PostgresIngestionJobRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        knowledge_base_id: row
            .try_get("knowledge_base_id")
            .map_err(|error| error.to_string())?,
        source_document_id: row
            .try_get("source_document_id")
            .map_err(|error| error.to_string())?,
        state: row.try_get("state").map_err(|error| error.to_string())?,
        progress: row.try_get("progress").map_err(|error| error.to_string())?,
        attempts: row.try_get("attempts").map_err(|error| error.to_string())?,
        error_message: row
            .try_get("error_message")
            .map_err(|error| error.to_string())?,
        next_attempt_at: row
            .try_get("next_attempt_at")
            .map_err(|error| error.to_string())?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn cancel_postgres_ingestion_job(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    job_id: Uuid,
) -> Result<PostgresIngestionJobRecord, String> {
    let pool = active_pool(&state)?;
    let knowledge_base_id: Uuid =
        sqlx::query_scalar("SELECT knowledge_base_id FROM ingestion_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!(
                    "读取导入任务所属知识库失败: {}",
                    safe_error(&error.to_string())
                )
            })?
            .ok_or_else(|| "任务不存在或已结束".to_string())?;
    ensure_private_knowledge_base(&pool, knowledge_base_id).await?;
    let row = sqlx::query(
        "UPDATE ingestion_jobs SET state = 'cancelled', progress = 100, updated_at = now()
         WHERE id = $1 AND state IN ('queued', 'uploading', 'parsing', 'chunking', 'embedding', 'indexing', 'pending', 'running', 'retrying')
         RETURNING id, knowledge_base_id, source_document_id, state, progress, attempts,
                   error_message, next_attempt_at::text, created_at::text, updated_at::text",
    )
    .bind(job_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("取消导入任务失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "任务不存在或已结束".to_string())?;
    Ok(PostgresIngestionJobRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        knowledge_base_id: row
            .try_get("knowledge_base_id")
            .map_err(|error| error.to_string())?,
        source_document_id: row
            .try_get("source_document_id")
            .map_err(|error| error.to_string())?,
        state: row.try_get("state").map_err(|error| error.to_string())?,
        progress: row.try_get("progress").map_err(|error| error.to_string())?,
        attempts: row.try_get("attempts").map_err(|error| error.to_string())?,
        error_message: row
            .try_get("error_message")
            .map_err(|error| error.to_string())?,
        next_attempt_at: row
            .try_get("next_attempt_at")
            .map_err(|error| error.to_string())?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn list_postgres_documents(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    knowledge_base_id: Uuid,
) -> Result<Vec<PostgresDocumentRecord>, String> {
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT d.id, d.knowledge_base_id, d.display_name, d.source_kind,
                d.source_path, d.content_sha256, d.file_size, d.metadata,
                (SELECT av.id FROM document_versions av WHERE av.document_id = d.id AND av.activated_at IS NOT NULL ORDER BY av.activated_at DESC LIMIT 1) AS active_version_id,
                d.created_at::text, d.updated_at::text
         FROM source_documents d
         JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
         ORDER BY d.updated_at DESC, d.id
         LIMIT 200",
    )
    .bind(knowledge_base_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL 文档失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresDocumentRecord {
                id: row.try_get("id").map_err(|error| error.to_string())?,
                knowledge_base_id: row
                    .try_get("knowledge_base_id")
                    .map_err(|error| error.to_string())?,
                display_name: row
                    .try_get("display_name")
                    .map_err(|error| error.to_string())?,
                source_kind: row
                    .try_get("source_kind")
                    .map_err(|error| error.to_string())?,
                source_path: row
                    .try_get("source_path")
                    .map_err(|error| error.to_string())?,
                content_sha256: row
                    .try_get("content_sha256")
                    .map_err(|error| error.to_string())?,
                file_size: row.try_get("file_size").unwrap_or_default(),
                metadata: row
                    .try_get("metadata")
                    .unwrap_or_else(|_| serde_json::json!({})),
                active_version_id: row
                    .try_get("active_version_id")
                    .map_err(|error| error.to_string())?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|error| error.to_string())?,
                updated_at: row
                    .try_get("updated_at")
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn list_postgres_knowledge_chunks(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    knowledge_base_id: Uuid,
) -> Result<Vec<PostgresKnowledgeChunkRecord>, String> {
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT c.id, d.id AS document_id, d.display_name, v.id AS version_id,
                c.ordinal, c.title_path, c.text, c.source_location
         FROM document_chunks c
         JOIN document_versions v ON v.id = c.version_id
         JOIN source_documents d ON d.id = v.document_id
         JOIN knowledge_bases kb ON kb.id = d.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
         ORDER BY d.updated_at DESC, c.ordinal, c.id LIMIT 500",
    )
    .bind(knowledge_base_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("读取知识片段失败: {}", safe_error(&error.to_string())))?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresKnowledgeChunkRecord {
                id: row.try_get("id").map_err(|error| error.to_string())?,
                document_id: row
                    .try_get("document_id")
                    .map_err(|error| error.to_string())?,
                document_name: row
                    .try_get("display_name")
                    .map_err(|error| error.to_string())?,
                version_id: row
                    .try_get("version_id")
                    .map_err(|error| error.to_string())?,
                ordinal: row.try_get("ordinal").map_err(|error| error.to_string())?,
                title_path: row
                    .try_get("title_path")
                    .map_err(|error| error.to_string())?,
                text: row.try_get("text").map_err(|error| error.to_string())?,
                source_location: row
                    .try_get("source_location")
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

fn authorize_postgres_content_file(
    app: &tauri::AppHandle,
    stored_path: &str,
) -> Result<std::path::PathBuf, String> {
    let database = crate::db::database_path(app)?;
    let root = database
        .parent()
        .ok_or_else(|| "解析本地内容目录失败".to_string())?;
    authorize_postgres_content_path(root, std::path::Path::new(stored_path))
}

fn authorize_postgres_content_path(
    root: &std::path::Path,
    stored_path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let roots = crate::permissions::path::AuthorizedRoots::new(vec![root.to_path_buf()])
        .map_err(|error| format!("内容路径授权失败: {error}"))?;
    let authorized = roots
        .authorize(stored_path)
        .map_err(|error| format!("内容路径未授权: {error}"))?;
    let file = std::fs::File::open(authorized.canonical_path())
        .map_err(|error| format!("读取内容文件失败: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("读取内容文件元数据失败: {error}"))?;
    if !metadata.is_file() {
        return Err("内容路径必须指向文件".to_string());
    }
    #[cfg(windows)]
    roots
        .verify_opened_file(&authorized, &file)
        .map_err(|error| format!("内容文件已发生变化: {error}"))?;
    Ok(authorized.canonical_path().to_path_buf())
}

#[tauri::command]
pub async fn get_postgres_document_preview(
    app: tauri::AppHandle,
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
) -> Result<serde_json::Value, String> {
    let pool = active_pool(&state)?;
    let row = sqlx::query(
        "SELECT d.display_name, d.source_path, d.metadata, v.parser, v.mime_type, v.created_at::text AS updated_at, v.id AS version_id
         FROM source_documents d
         JOIN document_versions v ON v.document_id = d.id
         WHERE d.id = $1 AND d.deleted_at IS NULL AND v.activated_at IS NOT NULL
         ORDER BY v.activated_at DESC, v.id DESC LIMIT 1",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL 文档预览失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let Some(row) = row else {
        return Ok(serde_json::json!({
            "processed": false,
            "document_name": null,
            "source_path": null,
            "mime_type": null,
            "updated_at": null,
            "source": null,
            "content": null,
            "blocks": [],
            "pages": [],
            "raw_data_url": null,
            "raw_sheets": []
        }));
    };
    let document_name: String = row
        .try_get("display_name")
        .map_err(|error| error.to_string())?;
    let source_path: String = row.try_get("source_path").unwrap_or_default();
    let metadata: serde_json::Value = row
        .try_get("metadata")
        .unwrap_or_else(|_| serde_json::json!({}));
    let parser: String = row.try_get("parser").map_err(|error| error.to_string())?;
    let mime_type: String = row
        .try_get("mime_type")
        .map_err(|error| error.to_string())?;
    let updated_at: String = row
        .try_get("updated_at")
        .map_err(|error| error.to_string())?;
    let version_id: Uuid = row
        .try_get("version_id")
        .map_err(|error| error.to_string())?;
    let storage_path: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(NULLIF(storage_path, ''), source_path)
         FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL 预览文件路径失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let chunks =
        sqlx::query("SELECT ordinal, text, source_location FROM document_chunks WHERE version_id = $1 ORDER BY ordinal, id")
            .bind(version_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!(
                    "读取 PostgreSQL 文档分块失败: {}",
                    safe_error(&error.to_string())
                )
            })?
            .into_iter()
            .map(|row| {
                let ordinal = row.try_get::<i32, _>("ordinal").map_err(|error| error.to_string())?;
                let content = row.try_get::<String, _>("text").map_err(|error| error.to_string())?;
                let source_location = row.try_get::<serde_json::Value, _>("source_location").map_err(|error| error.to_string())?;
                Ok::<_, String>((ordinal, content, source_location))
            })
            .collect::<Result<Vec<_>, _>>()?;
    let content = (!chunks.is_empty()).then(|| {
        chunks
            .iter()
            .map(|(_, content, _)| content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    });
    let mut pages = std::collections::BTreeMap::<u64, Vec<String>>::new();
    for (_, text, location) in &chunks {
        if location.get("kind").and_then(serde_json::Value::as_str) == Some("pdf_page") {
            if let Some(page) = location.get("page").and_then(serde_json::Value::as_u64) {
                pages.entry(page).or_default().push(text.clone());
            }
        }
    }
    let chunk_pages = pages
        .into_iter()
        .map(|(page, blocks)| serde_json::json!({ "page": page, "content": blocks.join("\n\n") }))
        .collect::<Vec<_>>();
    let parsed_source = storage_path
        .as_deref()
        .and_then(|path| authorize_postgres_content_file(&app, path).ok())
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| {
            crate::rag::ingest::SourceFormat::from_mime_type(&mime_type)
                .map(|format| (bytes, format))
        })
        .and_then(|(bytes, format)| {
            crate::rag::parse::parse_document_bytes(
                &bytes,
                format,
                crate::rag::parse::ParseLimits::default(),
            )
            .ok()
        });
    let raw_sheets = parsed_source
        .as_ref()
        .map(|document| render_postgres_preview_sheets(document.clone()))
        .unwrap_or_default();
    let pages = parsed_source
        .as_ref()
        .map(render_postgres_preview_pages)
        .filter(|pages| !pages.is_empty())
        .unwrap_or(chunk_pages);
    let structured_blocks = parsed_source
        .map(render_postgres_preview_blocks)
        .unwrap_or_default();
    Ok(serde_json::json!({
        "processed": true,
        "document_name": document_name,
        "source_path": source_path,
        "mime_type": mime_type,
        "updated_at": updated_at,
        "source": parser,
        "content": content,
        "blocks": chunks.into_iter().map(|(ordinal, content, source_location)| serde_json::json!({ "ordinal": ordinal, "content": content, "source_location": source_location })).collect::<Vec<_>>(),
        "structured_blocks": structured_blocks,
        "pages": pages,
        "raw_data_url": null,
        "raw_sheets": raw_sheets,
        "metadata": metadata
    }))
}

#[tauri::command]
pub async fn update_postgres_document_metadata(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
    metadata: serde_json::Value,
) -> Result<(), String> {
    if !metadata.is_object() {
        return Err("文档元数据必须是 JSON 对象".to_string());
    }
    let pool = active_pool(&state)?;
    let base_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT knowledge_base_id FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("读取文档所属知识库失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "PostgreSQL 文档不存在".to_string())?;
    ensure_private_knowledge_base(&pool, base_id).await?;
    let changed = sqlx::query(
        "UPDATE source_documents SET metadata = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .bind(metadata)
    .execute(&pool)
    .await
    .map_err(|error| format!("更新文档元数据失败: {}", safe_error(&error.to_string())))?;
    if changed.rows_affected() == 0 {
        return Err("PostgreSQL 文档不存在".to_string());
    }
    Ok(())
}

fn render_postgres_preview_sheets(
    document: crate::rag::parse::ParsedDocument,
) -> Vec<serde_json::Value> {
    document
        .blocks
        .into_iter()
        .filter_map(|block| {
            let crate::rag::parse::DocumentBlock::Table {
                rows,
                location: crate::rag::model::SourceLocation::SheetRange { sheet, .. },
            } = block
            else {
                return None;
            };
            let truncated = rows.len() > 100 || rows.iter().any(|row| row.len() > 40);
            let body = rows
                .into_iter()
                .take(100)
                .map(|row| {
                    let cells = row
                        .into_iter()
                        .take(40)
                        .map(|cell| format!("<td>{}</td>", escape_postgres_preview_html(&cell)))
                        .collect::<String>();
                    format!("<tr>{cells}</tr>")
                })
                .collect::<String>();
            Some(serde_json::json!({"name": sheet, "html": format!("<table><tbody>{body}</tbody></table>"), "truncated": truncated}))
        })
        .collect()
}

fn render_postgres_preview_blocks(
    document: crate::rag::parse::ParsedDocument,
) -> Vec<serde_json::Value> {
    document.blocks.into_iter().map(|block| match block {
        crate::rag::parse::DocumentBlock::Heading { level, text, location } => serde_json::json!({ "kind": "heading", "level": level, "text": text, "source_location": location }),
        crate::rag::parse::DocumentBlock::Paragraph { text, location } => serde_json::json!({ "kind": "paragraph", "text": text, "source_location": location }),
        crate::rag::parse::DocumentBlock::List { ordered, items, location } => serde_json::json!({ "kind": "list", "ordered": ordered, "items": items, "source_location": location }),
        crate::rag::parse::DocumentBlock::Table { rows, location } => serde_json::json!({ "kind": "table", "rows": rows, "source_location": location }),
        crate::rag::parse::DocumentBlock::Formula { text, location } => serde_json::json!({ "kind": "formula", "text": text, "source_location": location }),
        crate::rag::parse::DocumentBlock::Image { alt, location, .. } => serde_json::json!({ "kind": "image", "text": alt, "source_location": location }),
    }).collect()
}

fn render_postgres_preview_pages(
    document: &crate::rag::parse::ParsedDocument,
) -> Vec<serde_json::Value> {
    let mut pages = std::collections::BTreeMap::<u32, Vec<String>>::new();
    for block in &document.blocks {
        if let crate::rag::parse::DocumentBlock::Paragraph {
            text,
            location: crate::rag::model::SourceLocation::PdfPage { page, .. },
        } = block
        {
            pages.entry(*page).or_default().push(text.clone());
        }
    }
    pages
        .into_iter()
        .map(|(page, blocks)| serde_json::json!({ "page": page, "content": blocks.join("\n\n") }))
        .collect()
}

fn escape_postgres_preview_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[tauri::command]
pub async fn list_postgres_document_versions(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
) -> Result<Vec<PostgresDocumentVersionRecord>, String> {
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT v.id, v.document_id, v.content_sha256, v.mime_type, v.parser,
                v.embedding_profile_id, v.embedding_model_id, v.embedding_dimension,
                v.expected_asset_count, v.expected_chunk_count, v.manifest_sealed,
                v.created_at::text, v.activated_at::text,
                (SELECT COUNT(*) FROM document_assets a WHERE a.version_id = v.id) AS asset_count,
                (SELECT COUNT(*) FROM document_chunks c WHERE c.version_id = v.id) AS chunk_count
         FROM document_versions v
         JOIN source_documents d ON d.id = v.document_id
         WHERE v.document_id = $1 AND d.deleted_at IS NULL
         ORDER BY v.created_at DESC, v.id",
    )
    .bind(document_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL 文档版本失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresDocumentVersionRecord {
                id: row.try_get("id").map_err(|error| error.to_string())?,
                document_id: row
                    .try_get("document_id")
                    .map_err(|error| error.to_string())?,
                content_sha256: row
                    .try_get("content_sha256")
                    .map_err(|error| error.to_string())?,
                mime_type: row
                    .try_get("mime_type")
                    .map_err(|error| error.to_string())?,
                parser: row.try_get("parser").map_err(|error| error.to_string())?,
                parser_version: "postgresql".to_string(),
                chunk_policy_version: "postgresql-v1".to_string(),
                embedding_profile_id: row
                    .try_get("embedding_profile_id")
                    .map_err(|error| error.to_string())?,
                embedding_model_id: row
                    .try_get("embedding_model_id")
                    .map_err(|error| error.to_string())?,
                embedding_dimension: row
                    .try_get("embedding_dimension")
                    .map_err(|error| error.to_string())?,
                expected_asset_count: row
                    .try_get("expected_asset_count")
                    .map_err(|error| error.to_string())?,
                expected_chunk_count: row
                    .try_get("expected_chunk_count")
                    .map_err(|error| error.to_string())?,
                manifest_sealed: row
                    .try_get("manifest_sealed")
                    .map_err(|error| error.to_string())?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|error| error.to_string())?,
                activated_at: row
                    .try_get("activated_at")
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn get_postgres_document_raw(
    app: tauri::AppHandle,
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
) -> Result<serde_json::Value, String> {
    let pool = active_pool(&state)?;
    let row = sqlx::query(
        "SELECT source_path,
                COALESCE(NULLIF(storage_path, ''), source_path) AS storage_path
         FROM source_documents
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL 原始文件失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .ok_or_else(|| "PostgreSQL 文档不存在".to_string())?;
    let source_path: String = row
        .try_get("source_path")
        .map_err(|error| error.to_string())?;
    let storage_path: String = row
        .try_get("storage_path")
        .map_err(|error| error.to_string())?;
    let authorized_storage_path = authorize_postgres_content_file(&app, &storage_path)?;
    let bytes = std::fs::read(&authorized_storage_path)
        .map_err(|error| format!("读取 PostgreSQL 原始文件失败: {error}"))?;
    let mime_type = match std::path::Path::new(&source_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("xls") => "application/vnd.ms-excel",
        Some("csv") => "text/csv",
        Some("html") | Some("htm") => "text/html",
        Some("md") | Some("markdown") => "text/markdown",
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    };
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    Ok(serde_json::json!({
        "data_url": format!("data:{mime_type};base64,{}", STANDARD.encode(bytes)),
        "sheets": []
    }))
}

#[tauri::command]
pub async fn export_postgres_document(
    app: tauri::AppHandle,
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
    destination: std::path::PathBuf,
) -> Result<(), String> {
    if destination.as_os_str().is_empty() {
        return Err("导出路径不能为空".to_string());
    }
    let pool = active_pool(&state)?;
    let source: String = sqlx::query_scalar(
        "SELECT COALESCE(NULLIF(storage_path, ''), source_path)
         FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("读取文档原始路径失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "PostgreSQL 文档不存在".to_string())?;
    let source = authorize_postgres_content_file(&app, &source)?;
    if source == destination {
        return Err("导出目标不能覆盖文档原始文件".to_string());
    }
    std::fs::copy(&source, &destination).map_err(|error| format!("导出原始文档失败: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn delete_postgres_document(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
) -> Result<(), String> {
    let pool = active_pool(&state)?;
    let knowledge_base_id: Uuid = sqlx::query_scalar(
        "SELECT knowledge_base_id FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("读取文档所属知识库失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "PostgreSQL 文档不存在".to_string())?;
    ensure_private_knowledge_base(&pool, knowledge_base_id).await?;
    let result = sqlx::query(
        "UPDATE source_documents SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .execute(&pool)
    .await
    .map_err(|error| {
        format!(
            "删除 PostgreSQL 文档失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    if result.rows_affected() == 0 {
        return Err("PostgreSQL 文档不存在".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn reparse_postgres_document(
    app: tauri::AppHandle,
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
) -> Result<PostgresDocumentQueuedResponse, String> {
    queue_postgres_document_reparse(&app, state.inner(), document_id).await
}

pub(crate) async fn reparse_postgres_document_for_api(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    document_id: Uuid,
) -> Result<PostgresDocumentQueuedResponse, String> {
    queue_postgres_document_reparse(app, state, document_id).await
}

#[tauri::command]
pub async fn move_postgres_document(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
    target_knowledge_base_id: Uuid,
) -> Result<PostgresDocumentRecord, String> {
    let pool = active_pool(&state)?;
    ensure_private_knowledge_base(&pool, target_knowledge_base_id).await?;
    let source_knowledge_base_id: Uuid = sqlx::query_scalar(
        "SELECT knowledge_base_id FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("读取文档所属知识库失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "PostgreSQL 文档不存在".to_string())?;
    ensure_private_knowledge_base(&pool, source_knowledge_base_id).await?;
    let row = sqlx::query(
        "UPDATE source_documents d SET knowledge_base_id = $1, updated_at = now()
         WHERE d.id = $2 AND d.deleted_at IS NULL
           AND EXISTS (SELECT 1 FROM knowledge_bases kb WHERE kb.id = $1 AND kb.deleted_at IS NULL)
         RETURNING d.id, d.knowledge_base_id, d.display_name, d.source_kind,
                   d.source_path, d.content_sha256, d.file_size, d.metadata,
                   (SELECT v.id FROM document_versions v WHERE v.document_id = d.id AND v.activated_at IS NOT NULL ORDER BY v.activated_at DESC LIMIT 1) AS active_version_id,
                   d.created_at::text, d.updated_at::text",
    )
    .bind(target_knowledge_base_id)
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("移动文档失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "文档或目标知识库不存在".to_string())?;
    Ok(PostgresDocumentRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        knowledge_base_id: row
            .try_get("knowledge_base_id")
            .map_err(|error| error.to_string())?,
        display_name: row
            .try_get("display_name")
            .map_err(|error| error.to_string())?,
        source_kind: row
            .try_get("source_kind")
            .map_err(|error| error.to_string())?,
        source_path: row
            .try_get("source_path")
            .map_err(|error| error.to_string())?,
        content_sha256: row
            .try_get("content_sha256")
            .map_err(|error| error.to_string())?,
        file_size: row.try_get("file_size").unwrap_or_default(),
        metadata: row
            .try_get("metadata")
            .unwrap_or_else(|_| serde_json::json!({})),
        active_version_id: row
            .try_get("active_version_id")
            .map_err(|error| error.to_string())?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn rename_postgres_document(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
    display_name: String,
) -> Result<PostgresDocumentRecord, String> {
    let display_name = display_name.trim();
    if display_name.is_empty() || display_name.len() > 500 {
        return Err("PostgreSQL 文档名称不能为空或过长".to_string());
    }
    let pool = active_pool(&state)?;
    let knowledge_base_id: Uuid = sqlx::query_scalar(
        "SELECT knowledge_base_id FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(document_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("读取文档所属知识库失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "PostgreSQL 文档不存在".to_string())?;
    ensure_private_knowledge_base(&pool, knowledge_base_id).await?;
    let row = sqlx::query(
        "UPDATE source_documents
         SET display_name = $2, updated_at = now()
         WHERE id = $1 AND deleted_at IS NULL
         RETURNING id, knowledge_base_id, display_name, source_kind,
                   source_path, content_sha256, file_size, metadata,
                   (SELECT id FROM document_versions
                    WHERE document_id = source_documents.id
                      AND activated_at IS NOT NULL
                    ORDER BY activated_at DESC, id DESC LIMIT 1) AS active_version_id,
                   created_at::text, updated_at::text",
    )
    .bind(document_id)
    .bind(display_name)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "重命名 PostgreSQL 文档失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .ok_or_else(|| "PostgreSQL 文档不存在".to_string())?;
    Ok(PostgresDocumentRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        knowledge_base_id: row
            .try_get("knowledge_base_id")
            .map_err(|error| error.to_string())?,
        display_name: row
            .try_get("display_name")
            .map_err(|error| error.to_string())?,
        source_kind: row
            .try_get("source_kind")
            .map_err(|error| error.to_string())?,
        source_path: row
            .try_get("source_path")
            .map_err(|error| error.to_string())?,
        content_sha256: row
            .try_get("content_sha256")
            .map_err(|error| error.to_string())?,
        file_size: row.try_get("file_size").unwrap_or_default(),
        metadata: row
            .try_get("metadata")
            .unwrap_or_else(|_| serde_json::json!({})),
        active_version_id: row
            .try_get("active_version_id")
            .map_err(|error| error.to_string())?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

fn validate_page_input(input: &PostgresWikiPageInput) -> Result<(), String> {
    if input.slug.trim().is_empty() || input.title.trim().is_empty() {
        return Err("Wiki 页面 slug 和标题不能为空".to_string());
    }
    if input.slug.len() > 240 || input.title.len() > 240 {
        return Err("Wiki 页面 slug 或标题过长".to_string());
    }
    Ok(())
}

fn markdown_wiki_slugs(body: &str) -> Vec<String> {
    let mut slugs = Vec::new();
    let mut rest = body;
    while let Some((_, after_label)) = rest.split_once("](") {
        let Some((target, after_target)) = after_label.split_once(')') else {
            break;
        };
        let target = target
            .trim()
            .trim_start_matches('#')
            .trim_start_matches('/');
        if !target.is_empty()
            && !target.contains("://")
            && !target.starts_with("mailto:")
            && !target.contains(char::is_whitespace)
        {
            let target = target.split('#').next().unwrap_or_default();
            if !target.is_empty() && !slugs.iter().any(|value| value == target) {
                slugs.push(target.to_string());
            }
        }
        rest = after_target;
    }
    slugs
}

async fn rebuild_wiki_links(
    transaction: &mut Transaction<'_, Postgres>,
    page_id: Uuid,
    knowledge_base_id: Uuid,
    body: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE knowledge_edges SET deleted_at = now()
         WHERE source_page_id = $1 AND relation = 'wiki_link' AND deleted_at IS NULL",
    )
    .bind(page_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("清理 Wiki 链接失败: {}", safe_error(&error.to_string())))?;
    for slug in markdown_wiki_slugs(body) {
        let target_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM wiki_pages
             WHERE knowledge_base_id = $1 AND slug = $2 AND deleted_at IS NULL",
        )
        .bind(knowledge_base_id)
        .bind(&slug)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| format!("解析 Wiki 链接失败: {}", safe_error(&error.to_string())))?;
        let Some(target_id) = target_id else {
            continue;
        };
        sqlx::query(
            "INSERT INTO knowledge_edges
                (id, knowledge_base_id, source_page_id, target_page_id, relation, metadata)
             VALUES ($1, $2, $3, $4, 'wiki_link', $5)",
        )
        .bind(Uuid::new_v4())
        .bind(knowledge_base_id)
        .bind(page_id)
        .bind(target_id)
        .bind(serde_json::json!({ "slug": slug }))
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("保存 Wiki 链接失败: {}", safe_error(&error.to_string())))?;
    }
    Ok(())
}

async fn rebuild_wiki_source_edge(
    transaction: &mut Transaction<'_, Postgres>,
    page_id: Uuid,
    knowledge_base_id: Uuid,
    source_document_id: Option<Uuid>,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE knowledge_edges SET deleted_at = now()
         WHERE source_page_id = $1 AND relation = 'source_document' AND deleted_at IS NULL",
    )
    .bind(page_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("清理 Wiki 来源关系失败: {}", safe_error(&error.to_string())))?;

    let Some(source_document_id) = source_document_id else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO knowledge_edges
            (id, knowledge_base_id, source_page_id, source_document_id, relation, metadata)
         VALUES ($1, $2, $3, $4, 'source_document', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(knowledge_base_id)
    .bind(page_id)
    .bind(source_document_id)
    .bind(serde_json::json!({ "source": "wiki_page" }))
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("保存 Wiki 来源关系失败: {}", safe_error(&error.to_string())))?;
    Ok(())
}

async fn replace_wiki_page_tags(
    transaction: &mut Transaction<'_, Postgres>,
    page_id: Uuid,
    knowledge_base_id: Uuid,
    names: &[String],
) -> Result<(), String> {
    let mut normalized = names
        .iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    if normalized.len() > 100 || normalized.iter().any(|name| name.len() > 100) {
        return Err("标签数量或长度超出限制".to_string());
    }
    sqlx::query("DELETE FROM wiki_page_tags WHERE page_id = $1")
        .bind(page_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("清理页面标签失败: {}", safe_error(&error.to_string())))?;
    for name in normalized {
        let tag_id: Uuid = sqlx::query_scalar(
            "INSERT INTO tags (id, knowledge_base_id, name) VALUES ($1, $2, $3)
             ON CONFLICT (knowledge_base_id, name) DO UPDATE SET name = EXCLUDED.name
             RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(knowledge_base_id)
        .bind(&name)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|error| format!("保存标签失败: {}", safe_error(&error.to_string())))?;
        sqlx::query("INSERT INTO wiki_page_tags (page_id, tag_id) VALUES ($1, $2)")
            .bind(page_id)
            .bind(tag_id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| format!("关联页面标签失败: {}", safe_error(&error.to_string())))?;
    }
    Ok(())
}

fn wiki_page_from_row(row: &sqlx::postgres::PgRow) -> Result<PostgresWikiPageRecord, String> {
    Ok(PostgresWikiPageRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        knowledge_base_id: row
            .try_get("knowledge_base_id")
            .map_err(|error| error.to_string())?,
        slug: row.try_get("slug").map_err(|error| error.to_string())?,
        title: row.try_get("title").map_err(|error| error.to_string())?,
        body_markdown: row
            .try_get("body_markdown")
            .map_err(|error| error.to_string())?,
        source_document_id: row
            .try_get("source_document_id")
            .map_err(|error| error.to_string())?,
        revision: row.try_get("revision").map_err(|error| error.to_string())?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn list_postgres_wiki_pages(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    knowledge_base_id: Uuid,
) -> Result<Vec<PostgresWikiPageRecord>, String> {
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT p.id, p.knowledge_base_id, p.slug, p.title, p.body_markdown,
                p.source_document_id, COALESCE(MAX(r.revision), 0)::int AS revision,
                p.created_at::text, p.updated_at::text
         FROM wiki_pages p
         JOIN knowledge_bases kb ON kb.id = p.knowledge_base_id AND kb.deleted_at IS NULL
         LEFT JOIN wiki_page_revisions r ON r.page_id = p.id
         WHERE p.knowledge_base_id = $1 AND p.deleted_at IS NULL
         GROUP BY p.id
         ORDER BY p.updated_at DESC, p.id",
    )
    .bind(knowledge_base_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL Wiki 页面失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    rows.iter().map(wiki_page_from_row).collect()
}

#[tauri::command]
pub async fn create_postgres_wiki_page(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    input: PostgresWikiPageInput,
) -> Result<PostgresWikiPageRecord, String> {
    validate_page_input(&input)?;
    let pool = active_pool(&state)?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| format!("开启 Wiki 事务失败: {}", safe_error(&error.to_string())))?;
    let knowledge_base_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM knowledge_bases WHERE id = $1 AND visibility = 'private' AND deleted_at IS NULL)",
    )
    .bind(input.knowledge_base_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| format!("检查知识库失败: {}", safe_error(&error.to_string())))?;
    if !knowledge_base_exists {
        return Err("PostgreSQL 知识库不存在".to_string());
    }
    if let Some(source_document_id) = input.source_document_id {
        let source_matches: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM source_documents
                WHERE id = $1 AND knowledge_base_id = $2 AND deleted_at IS NULL
            )",
        )
        .bind(source_document_id)
        .bind(input.knowledge_base_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| format!("检查 Wiki 来源文档失败: {}", safe_error(&error.to_string())))?;
        if !source_matches {
            return Err("Wiki 来源文档不属于目标知识库".to_string());
        }
    }
    let id = Uuid::new_v4();
    let revision_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO wiki_pages
            (id, knowledge_base_id, slug, title, body_markdown, source_document_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(input.knowledge_base_id)
    .bind(input.slug.trim())
    .bind(input.title.trim())
    .bind(&input.body_markdown)
    .bind(input.source_document_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "创建 PostgreSQL Wiki 页面失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    sqlx::query(
        "INSERT INTO wiki_page_revisions (id, page_id, revision, title, body_markdown)
         VALUES ($1, $2, 1, $3, $4)",
    )
    .bind(revision_id)
    .bind(id)
    .bind(input.title.trim())
    .bind(&input.body_markdown)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "创建 Wiki revision 失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    rebuild_wiki_links(
        &mut transaction,
        id,
        input.knowledge_base_id,
        &input.body_markdown,
    )
    .await?;
    rebuild_wiki_source_edge(
        &mut transaction,
        id,
        input.knowledge_base_id,
        input.source_document_id,
    )
    .await?;
    replace_wiki_page_tags(&mut transaction, id, input.knowledge_base_id, &input.tags).await?;
    let row = sqlx::query(
        "SELECT p.id, p.knowledge_base_id, p.slug, p.title, p.body_markdown,
                p.source_document_id, 1::int AS revision,
                p.created_at::text, p.updated_at::text
         FROM wiki_pages p WHERE p.id = $1",
    )
    .bind(id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| format!("读取新建 Wiki 页面失败: {}", safe_error(&error.to_string())))?;
    transaction
        .commit()
        .await
        .map_err(|error| format!("提交 Wiki 事务失败: {}", safe_error(&error.to_string())))?;
    wiki_page_from_row(&row)
}

#[tauri::command]
pub async fn update_postgres_wiki_page(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    page_id: Uuid,
    title: String,
    body_markdown: String,
    tags: Option<Vec<String>>,
    source_document_id: Option<Uuid>,
) -> Result<PostgresWikiPageRecord, String> {
    if title.trim().is_empty() {
        return Err("Wiki 页面标题不能为空".to_string());
    }
    let pool = active_pool(&state)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let (knowledge_base_id, revision): (Uuid, i32) = sqlx::query_as(
        "SELECT knowledge_base_id,
                (SELECT COALESCE(MAX(revision), 0)::int + 1
                 FROM wiki_page_revisions WHERE page_id = p.id)
         FROM wiki_pages p JOIN knowledge_bases kb ON kb.id = p.knowledge_base_id
         WHERE p.id = $1 AND p.deleted_at IS NULL AND kb.visibility = 'private'",
    )
    .bind(page_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("读取 Wiki 页面状态失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "PostgreSQL Wiki 页面不存在".to_string())?;
    if let Some(source_document_id) = source_document_id {
        let source_matches: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM source_documents
                WHERE id = $1 AND knowledge_base_id = $2 AND deleted_at IS NULL
            )",
        )
        .bind(source_document_id)
        .bind(knowledge_base_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| format!("检查 Wiki 来源文档失败: {}", safe_error(&error.to_string())))?;
        if !source_matches {
            return Err("Wiki 来源文档不属于目标知识库".to_string());
        }
    }
    sqlx::query(
        "UPDATE wiki_pages SET title = $1, body_markdown = $2, source_document_id = $3, updated_at = now()
         WHERE id = $4 AND deleted_at IS NULL",
    )
    .bind(title.trim())
    .bind(&body_markdown)
    .bind(source_document_id)
    .bind(page_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("更新 PostgreSQL Wiki 页面失败: {}", safe_error(&error.to_string())))?;
    sqlx::query(
        "INSERT INTO wiki_page_revisions (id, page_id, revision, title, body_markdown)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(page_id)
    .bind(revision)
    .bind(title.trim())
    .bind(&body_markdown)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "保存 Wiki revision 失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    rebuild_wiki_links(&mut transaction, page_id, knowledge_base_id, &body_markdown).await?;
    rebuild_wiki_source_edge(
        &mut transaction,
        page_id,
        knowledge_base_id,
        source_document_id,
    )
    .await?;
    if let Some(tags) = tags.as_deref() {
        replace_wiki_page_tags(&mut transaction, page_id, knowledge_base_id, tags).await?;
    }
    let row = sqlx::query(
        "SELECT p.id, p.knowledge_base_id, p.slug, p.title, p.body_markdown,
                p.source_document_id, $2::int AS revision,
                p.created_at::text, p.updated_at::text
         FROM wiki_pages p WHERE p.id = $1",
    )
    .bind(page_id)
    .bind(revision)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "读取更新后的 Wiki 页面失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .ok_or_else(|| "PostgreSQL Wiki 页面不存在".to_string())?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    wiki_page_from_row(&row)
}

#[tauri::command]
pub async fn list_postgres_wiki_revisions(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    page_id: Uuid,
) -> Result<Vec<PostgresWikiRevisionRecord>, String> {
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT id, page_id, revision, title, body_markdown, created_at::text
         FROM wiki_page_revisions r
         JOIN wiki_pages p ON p.id = r.page_id AND p.deleted_at IS NULL
         JOIN knowledge_bases kb ON kb.id = p.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE r.page_id = $1 ORDER BY r.revision DESC",
    )
    .bind(page_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("读取 Wiki 历史失败: {}", safe_error(&error.to_string())))?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresWikiRevisionRecord {
                id: row.try_get("id").map_err(|error| error.to_string())?,
                page_id: row.try_get("page_id").map_err(|error| error.to_string())?,
                revision: row.try_get("revision").map_err(|error| error.to_string())?,
                title: row.try_get("title").map_err(|error| error.to_string())?,
                body_markdown: row
                    .try_get("body_markdown")
                    .map_err(|error| error.to_string())?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn restore_postgres_wiki_revision(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    page_id: Uuid,
    revision: i32,
) -> Result<PostgresWikiPageRecord, String> {
    if revision <= 0 {
        return Err("Wiki revision 无效".to_string());
    }
    let pool = active_pool(&state)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let source = sqlx::query(
        "SELECT r.title, r.body_markdown, p.knowledge_base_id
         FROM wiki_page_revisions r
         JOIN wiki_pages p ON p.id = r.page_id AND p.deleted_at IS NULL
         JOIN knowledge_bases kb ON kb.id = p.knowledge_base_id
         WHERE r.page_id = $1 AND r.revision = $2 AND kb.deleted_at IS NULL AND kb.visibility = 'private'",
    )
    .bind(page_id)
    .bind(revision)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "读取 Wiki revision 失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .ok_or_else(|| "Wiki revision 不存在".to_string())?;
    let title: String = source.try_get("title").map_err(|error| error.to_string())?;
    let body: String = source
        .try_get("body_markdown")
        .map_err(|error| error.to_string())?;
    let knowledge_base_id: Uuid = source
        .try_get("knowledge_base_id")
        .map_err(|error| error.to_string())?;
    let next_revision: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision), 0)::int + 1 FROM wiki_page_revisions WHERE page_id = $1",
    )
    .bind(page_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| format!("读取目标 revision 失败: {}", safe_error(&error.to_string())))?;
    sqlx::query(
        "UPDATE wiki_pages SET title = $1, body_markdown = $2, updated_at = now()
         WHERE id = $3 AND deleted_at IS NULL",
    )
    .bind(&title)
    .bind(&body)
    .bind(page_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("回滚 Wiki 页面失败: {}", safe_error(&error.to_string())))?;
    rebuild_wiki_links(&mut transaction, page_id, knowledge_base_id, &body).await?;
    let source_document_id: Option<Uuid> =
        sqlx::query_scalar("SELECT source_document_id FROM wiki_pages WHERE id = $1")
            .bind(page_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| {
                format!("读取 Wiki 来源文档失败: {}", safe_error(&error.to_string()))
            })?;
    rebuild_wiki_source_edge(
        &mut transaction,
        page_id,
        knowledge_base_id,
        source_document_id,
    )
    .await?;
    sqlx::query(
        "INSERT INTO wiki_page_revisions (id, page_id, revision, title, body_markdown)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(page_id)
    .bind(next_revision)
    .bind(&title)
    .bind(&body)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("保存回滚 revision 失败: {}", safe_error(&error.to_string())))?;
    let row = sqlx::query(
        "SELECT p.id, p.knowledge_base_id, p.slug, p.title, p.body_markdown,
                p.source_document_id, $2::int AS revision,
                p.created_at::text, p.updated_at::text
         FROM wiki_pages p WHERE p.id = $1",
    )
    .bind(page_id)
    .bind(next_revision)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "读取回滚后的 Wiki 页面失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    wiki_page_from_row(&row)
}

#[tauri::command]
pub async fn list_postgres_knowledge_edges(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    knowledge_base_id: Uuid,
) -> Result<Vec<PostgresKnowledgeEdgeRecord>, String> {
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT e.id, e.knowledge_base_id, e.source_page_id, e.target_page_id,
                e.source_document_id, e.relation, e.metadata, e.created_at::text
         FROM knowledge_edges e
         JOIN knowledge_bases kb ON kb.id = e.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE e.knowledge_base_id = $1 AND e.deleted_at IS NULL
         ORDER BY e.created_at, e.id",
    )
    .bind(knowledge_base_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("读取知识边失败: {}", safe_error(&error.to_string())))?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresKnowledgeEdgeRecord {
                id: row.try_get("id").map_err(|error| error.to_string())?,
                knowledge_base_id: row
                    .try_get("knowledge_base_id")
                    .map_err(|error| error.to_string())?,
                source_page_id: row
                    .try_get("source_page_id")
                    .map_err(|error| error.to_string())?,
                target_page_id: row
                    .try_get("target_page_id")
                    .map_err(|error| error.to_string())?,
                source_document_id: row
                    .try_get("source_document_id")
                    .map_err(|error| error.to_string())?,
                relation: row.try_get("relation").map_err(|error| error.to_string())?,
                metadata: row.try_get("metadata").map_err(|error| error.to_string())?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn list_postgres_tags(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    knowledge_base_id: Uuid,
) -> Result<Vec<PostgresTagRecord>, String> {
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT t.id, t.knowledge_base_id, t.name FROM tags t
         JOIN knowledge_bases kb ON kb.id = t.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE t.knowledge_base_id = $1 ORDER BY t.name, t.id",
    )
    .bind(knowledge_base_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("读取标签失败: {}", safe_error(&error.to_string())))?;
    rows.into_iter()
        .map(|row| {
            Ok(PostgresTagRecord {
                id: row.try_get("id").map_err(|error| error.to_string())?,
                knowledge_base_id: row
                    .try_get("knowledge_base_id")
                    .map_err(|error| error.to_string())?,
                name: row.try_get("name").map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn list_postgres_wiki_page_tags(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    page_id: Uuid,
) -> Result<Vec<PostgresTagRecord>, String> {
    let pool = active_pool(&state)?;
    sqlx::query(
        "SELECT t.id, t.knowledge_base_id, t.name
         FROM tags t
         JOIN wiki_page_tags pt ON pt.tag_id = t.id
         JOIN wiki_pages p ON p.id = pt.page_id AND p.deleted_at IS NULL
         JOIN knowledge_bases kb ON kb.id = p.knowledge_base_id AND kb.deleted_at IS NULL
         WHERE pt.page_id = $1 ORDER BY t.name, t.id",
    )
    .bind(page_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("读取页面标签失败: {}", safe_error(&error.to_string())))?
    .into_iter()
    .map(|row| {
        Ok(PostgresTagRecord {
            id: row.try_get("id").map_err(|error| error.to_string())?,
            knowledge_base_id: row
                .try_get("knowledge_base_id")
                .map_err(|error| error.to_string())?,
            name: row.try_get("name").map_err(|error| error.to_string())?,
        })
    })
    .collect()
}

#[tauri::command]
pub async fn set_postgres_wiki_page_tags(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    page_id: Uuid,
    names: Vec<String>,
) -> Result<Vec<PostgresTagRecord>, String> {
    let mut normalized = names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    if normalized.len() > 100 || normalized.iter().any(|name| name.len() > 100) {
        return Err("标签数量或长度超出限制".to_string());
    }
    let pool = active_pool(&state)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let knowledge_base_id: Uuid = sqlx::query_scalar(
        "SELECT p.knowledge_base_id FROM wiki_pages p
         JOIN knowledge_bases kb ON kb.id = p.knowledge_base_id
         WHERE p.id = $1 AND p.deleted_at IS NULL AND kb.deleted_at IS NULL AND kb.visibility = 'private'",
    )
    .bind(page_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("检查页面失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "Wiki 页面不存在".to_string())?;
    sqlx::query("DELETE FROM wiki_page_tags WHERE page_id = $1")
        .bind(page_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("清理页面标签失败: {}", safe_error(&error.to_string())))?;
    let mut output = Vec::with_capacity(normalized.len());
    for name in normalized {
        let tag = sqlx::query(
            "INSERT INTO tags (id, knowledge_base_id, name) VALUES ($1, $2, $3)
             ON CONFLICT (knowledge_base_id, name) DO UPDATE SET name = EXCLUDED.name
             RETURNING id, knowledge_base_id, name",
        )
        .bind(Uuid::new_v4())
        .bind(knowledge_base_id)
        .bind(&name)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| format!("保存标签失败: {}", safe_error(&error.to_string())))?;
        let tag_id: Uuid = tag.try_get("id").map_err(|error| error.to_string())?;
        sqlx::query("INSERT INTO wiki_page_tags (page_id, tag_id) VALUES ($1, $2)")
            .bind(page_id)
            .bind(tag_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("关联页面标签失败: {}", safe_error(&error.to_string())))?;
        output.push(PostgresTagRecord {
            id: tag_id,
            knowledge_base_id: tag
                .try_get("knowledge_base_id")
                .map_err(|error| error.to_string())?,
            name: tag.try_get("name").map_err(|error| error.to_string())?,
        });
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(output)
}

#[tauri::command]
pub async fn create_postgres_knowledge_edge(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    input: PostgresKnowledgeEdgeInput,
) -> Result<PostgresKnowledgeEdgeRecord, String> {
    let relation = input.relation.trim();
    if relation.is_empty()
        || relation.len() > 100
        || input.source_page_id.is_none() && input.target_page_id.is_none()
    {
        return Err("知识边关系和端点不能为空".to_string());
    }
    let metadata = if input.metadata.is_null() {
        serde_json::json!({})
    } else {
        input.metadata
    };
    let pool = active_pool(&state)?;
    ensure_private_knowledge_base(&pool, input.knowledge_base_id).await?;
    let endpoint_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wiki_pages
         WHERE knowledge_base_id = $1 AND deleted_at IS NULL
           AND id = ANY($2::uuid[])",
    )
    .bind(input.knowledge_base_id)
    .bind(
        [input.source_page_id, input.target_page_id]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("检查知识边端点失败: {}", safe_error(&error.to_string())))?;
    let expected = [input.source_page_id, input.target_page_id]
        .into_iter()
        .flatten()
        .collect::<std::collections::HashSet<_>>()
        .len() as i64;
    if endpoint_count != expected {
        return Err("知识边端点必须属于同一知识库".to_string());
    }
    let row = sqlx::query(
        "INSERT INTO knowledge_edges
           (id, knowledge_base_id, source_page_id, target_page_id, relation, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, knowledge_base_id, source_page_id, target_page_id, source_document_id,
                   relation, metadata, created_at::text",
    )
    .bind(Uuid::new_v4())
    .bind(input.knowledge_base_id)
    .bind(input.source_page_id)
    .bind(input.target_page_id)
    .bind(relation)
    .bind(metadata)
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("创建知识边失败: {}", safe_error(&error.to_string())))?;
    Ok(PostgresKnowledgeEdgeRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        knowledge_base_id: row
            .try_get("knowledge_base_id")
            .map_err(|error| error.to_string())?,
        source_page_id: row
            .try_get("source_page_id")
            .map_err(|error| error.to_string())?,
        target_page_id: row
            .try_get("target_page_id")
            .map_err(|error| error.to_string())?,
        source_document_id: row
            .try_get("source_document_id")
            .map_err(|error| error.to_string())?,
        relation: row.try_get("relation").map_err(|error| error.to_string())?,
        metadata: row.try_get("metadata").map_err(|error| error.to_string())?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn delete_postgres_knowledge_edge(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    edge_id: Uuid,
) -> Result<(), String> {
    let pool = active_pool(&state)?;
    let knowledge_base_id: Uuid = sqlx::query_scalar(
        "SELECT knowledge_base_id FROM knowledge_edges WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(edge_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取知识边所属知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .ok_or_else(|| "知识边不存在".to_string())?;
    ensure_private_knowledge_base(&pool, knowledge_base_id).await?;
    let result = sqlx::query(
        "UPDATE knowledge_edges SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(edge_id)
    .execute(&pool)
    .await
    .map_err(|error| format!("删除知识边失败: {}", safe_error(&error.to_string())))?;
    if result.rows_affected() == 0 {
        return Err("知识边不存在".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn merge_postgres_knowledge_bases(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    request: PostgresKnowledgeBaseMergeRequest,
) -> Result<PostgresKnowledgeBaseRecord, String> {
    if request.source_id == request.target_id {
        return Err("源知识库和目标知识库必须不同".to_string());
    }
    let mode = request.mode.trim();
    if mode != "existing" && mode != "new" {
        return Err("知识库合并模式无效".to_string());
    }
    let pool = active_pool(&state)?;
    ensure_private_knowledge_base(&pool, request.source_id).await?;
    ensure_private_knowledge_base(&pool, request.target_id).await?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    for id in [request.source_id, request.target_id] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "检查 PostgreSQL 知识库失败: {}",
                safe_error(&error.to_string())
            )
        })?;
        if !exists {
            return Err("PostgreSQL 知识库不存在".to_string());
        }
    }
    let destination_id = if mode == "existing" {
        request.target_id
    } else {
        let name = request
            .destination_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "新知识库名称不能为空".to_string())?;
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO knowledge_bases (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind(name)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                format!(
                    "创建 PostgreSQL 合并知识库失败: {}",
                    safe_error(&error.to_string())
                )
            })?;
        id
    };
    let source_ids = if mode == "existing" {
        vec![request.source_id]
    } else {
        vec![request.source_id, request.target_id]
    };
    sqlx::query(
        "UPDATE source_documents
         SET knowledge_base_id = $1, updated_at = now()
         WHERE knowledge_base_id = ANY($2::uuid[]) AND deleted_at IS NULL",
    )
    .bind(destination_id)
    .bind(source_ids)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "合并 PostgreSQL 文档失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let row = sqlx::query(
        "SELECT id, name, description, library_type, tags, visibility, embedding_model, chunk_strategy, created_at::text, updated_at::text
         FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(destination_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL 合并知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(PostgresKnowledgeBaseRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        name: row.try_get("name").map_err(|error| error.to_string())?,
        description: row
            .try_get("description")
            .map_err(|error| error.to_string())?,
        library_type: row
            .try_get("library_type")
            .map_err(|error| error.to_string())?,
        tags: row.try_get("tags").map_err(|error| error.to_string())?,
        visibility: row
            .try_get("visibility")
            .map_err(|error| error.to_string())?,
        embedding_model: row
            .try_get("embedding_model")
            .map_err(|error| error.to_string())?,
        chunk_strategy: row
            .try_get("chunk_strategy")
            .map_err(|error| error.to_string())?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn get_postgres_knowledge_health(
    state: tauri::State<'_, KnowledgeDatabaseState>,
) -> Result<PostgresKnowledgeHealth, String> {
    let pool = active_pool(&state)?;
    let row = sqlx::query(
        "SELECT
           (SELECT COUNT(*) FROM knowledge_bases WHERE deleted_at IS NULL) AS knowledge_base_count,
           (SELECT COUNT(*) FROM source_documents WHERE deleted_at IS NULL) AS document_count,
           (SELECT COUNT(*) FROM source_documents d
              WHERE d.deleted_at IS NULL AND EXISTS (
                SELECT 1 FROM document_versions v
                WHERE v.document_id = d.id AND v.activated_at IS NOT NULL)) AS active_document_count,
           (SELECT COUNT(*) FROM document_versions) AS version_count,
           (SELECT COUNT(*) FROM document_chunks) AS chunk_count,
           (SELECT COUNT(*) FROM chunk_embeddings) AS indexed_chunk_count,
           (SELECT COUNT(*) FROM ingestion_jobs
              WHERE state IN ('queued', 'uploading', 'parsing', 'chunking', 'embedding', 'indexing', 'pending', 'running', 'retrying')) AS active_task_count,
           (SELECT COUNT(*) FROM ingestion_jobs WHERE state = 'completed') AS processing_success_count,
           (SELECT COUNT(*) FROM ingestion_jobs WHERE state IN ('failed', 'quarantined')) AS processing_failure_count,
           (SELECT COUNT(*) FROM retrieval_audits) AS retrieval_count,
           (SELECT AVG(duration_ms)::double precision FROM retrieval_audits WHERE duration_ms > 0) AS average_retrieval_duration_ms,
           (SELECT AVG(EXTRACT(EPOCH FROM (finished_at - started_at)) * 1000)::double precision
              FROM ingestion_attempts WHERE finished_at IS NOT NULL) AS average_processing_duration_ms",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("读取 PostgreSQL 知识库健康状态失败: {}", safe_error(&error.to_string())))?;
    Ok(PostgresKnowledgeHealth {
        knowledge_base_count: row
            .try_get("knowledge_base_count")
            .map_err(|error| error.to_string())?,
        document_count: row
            .try_get("document_count")
            .map_err(|error| error.to_string())?,
        active_document_count: row
            .try_get("active_document_count")
            .map_err(|error| error.to_string())?,
        version_count: row
            .try_get("version_count")
            .map_err(|error| error.to_string())?,
        chunk_count: row
            .try_get("chunk_count")
            .map_err(|error| error.to_string())?,
        indexed_chunk_count: row
            .try_get("indexed_chunk_count")
            .map_err(|error| error.to_string())?,
        active_task_count: row
            .try_get("active_task_count")
            .map_err(|error| error.to_string())?,
        processing_success_count: row
            .try_get("processing_success_count")
            .map_err(|error| error.to_string())?,
        processing_failure_count: row
            .try_get("processing_failure_count")
            .map_err(|error| error.to_string())?,
        retrieval_count: row
            .try_get("retrieval_count")
            .map_err(|error| error.to_string())?,
        average_retrieval_duration_ms: row
            .try_get("average_retrieval_duration_ms")
            .map_err(|error| error.to_string())?,
        average_processing_duration_ms: row
            .try_get("average_processing_duration_ms")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn list_postgres_knowledge_bases(
    state: tauri::State<'_, KnowledgeDatabaseState>,
) -> Result<Vec<PostgresKnowledgeBaseRecord>, String> {
    let pool = active_pool(&state)?;
    sqlx::query(
        "SELECT id, name, description, library_type, tags, visibility, embedding_model, chunk_strategy, created_at::text, updated_at::text
         FROM knowledge_bases WHERE deleted_at IS NULL
         ORDER BY updated_at DESC, id",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        format!(
            "读取 PostgreSQL 知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .into_iter()
    .map(|row| {
        Ok(PostgresKnowledgeBaseRecord {
            id: row.try_get("id").map_err(|error| error.to_string())?,
            name: row.try_get("name").map_err(|error| error.to_string())?,
            description: row.try_get("description").map_err(|error| error.to_string())?,
            library_type: row.try_get("library_type").map_err(|error| error.to_string())?,
            tags: row.try_get("tags").map_err(|error| error.to_string())?,
            visibility: row.try_get("visibility").map_err(|error| error.to_string())?,
            embedding_model: row.try_get("embedding_model").map_err(|error| error.to_string())?,
            chunk_strategy: row.try_get("chunk_strategy").map_err(|error| error.to_string())?,
            created_at: row
                .try_get("created_at")
                .map_err(|error| error.to_string())?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|error| error.to_string())?,
        })
    })
    .collect()
}

#[tauri::command]
pub async fn create_postgres_knowledge_base(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    input: PostgresKnowledgeBaseInput,
) -> Result<PostgresKnowledgeBaseRecord, String> {
    let input = normalize_knowledge_base_input(input)?;
    let name = input.name.as_str();
    validate_local_knowledge_access(&input.visibility)?;
    if input.visibility.trim() != "private" {
        return Err("单人客户端只能创建私有知识库".to_string());
    }
    let pool = active_pool(&state)?;
    let id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO knowledge_bases (id, name, description, library_type, tags, visibility, embedding_model, chunk_strategy)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING created_at::text, updated_at::text",
    )
    .bind(id)
    .bind(name)
    .bind(&input.description)
    .bind(&input.library_type)
    .bind(&input.tags)
    .bind(&input.visibility)
    .bind(&input.embedding_model)
    .bind(&input.chunk_strategy)
    .fetch_one(&pool)
    .await
    .map_err(|error| {
        format!(
            "创建 PostgreSQL 知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    Ok(PostgresKnowledgeBaseRecord {
        id,
        name: name.to_string(),
        description: input.description,
        library_type: input.library_type,
        tags: input.tags,
        visibility: input.visibility,
        embedding_model: input.embedding_model,
        chunk_strategy: input.chunk_strategy,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn rename_postgres_knowledge_base(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    id: Uuid,
    name: String,
) -> Result<PostgresKnowledgeBaseRecord, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("知识库名称不能为空".to_string());
    }
    if name.chars().count() > MAX_KNOWLEDGE_BASE_NAME {
        return Err("知识库名称不能超过 200 个字符".to_string());
    }
    let pool = active_pool(&state)?;
    ensure_private_knowledge_base(&pool, id).await?;
    let result = sqlx::query(
        "UPDATE knowledge_bases SET name = $1, updated_at = now()
         WHERE id = $2 AND deleted_at IS NULL
         RETURNING id, name, description, library_type, tags, visibility, embedding_model, chunk_strategy, created_at::text, updated_at::text",
    )
    .bind(name)
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "重命名 PostgreSQL 知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .ok_or_else(|| "PostgreSQL 知识库不存在".to_string())?;
    Ok(PostgresKnowledgeBaseRecord {
        id: result.try_get("id").map_err(|error| error.to_string())?,
        name: result.try_get("name").map_err(|error| error.to_string())?,
        description: result
            .try_get("description")
            .map_err(|error| error.to_string())?,
        library_type: result
            .try_get("library_type")
            .map_err(|error| error.to_string())?,
        tags: result.try_get("tags").map_err(|error| error.to_string())?,
        visibility: result
            .try_get("visibility")
            .map_err(|error| error.to_string())?,
        embedding_model: result
            .try_get("embedding_model")
            .map_err(|error| error.to_string())?,
        chunk_strategy: result
            .try_get("chunk_strategy")
            .map_err(|error| error.to_string())?,
        created_at: result
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: result
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn update_postgres_knowledge_base_settings(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    id: Uuid,
    input: PostgresKnowledgeBaseInput,
) -> Result<PostgresKnowledgeBaseRecord, String> {
    let input = normalize_knowledge_base_input(input)?;
    validate_local_knowledge_access(&input.visibility)?;
    if input.visibility.trim() != "private" {
        return Err("公共知识库为只读，不能将知识库设置为公共范围".to_string());
    }
    let pool = active_pool(&state)?;
    ensure_private_knowledge_base(&pool, id).await?;
    let row = sqlx::query(
        "UPDATE knowledge_bases
         SET name = $1, description = $2, library_type = $3, tags = $4, visibility = $5,
             embedding_model = $6, chunk_strategy = $7, updated_at = now()
         WHERE id = $8 AND deleted_at IS NULL
         RETURNING id, name, description, library_type, tags, visibility,
                   embedding_model, chunk_strategy, created_at::text, updated_at::text",
    )
    .bind(&input.name)
    .bind(&input.description)
    .bind(&input.library_type)
    .bind(&input.tags)
    .bind(&input.visibility)
    .bind(&input.embedding_model)
    .bind(&input.chunk_strategy)
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        format!(
            "更新 PostgreSQL 知识库设置失败: {}",
            safe_error(&error.to_string())
        )
    })?
    .ok_or_else(|| "PostgreSQL 知识库不存在".to_string())?;
    Ok(PostgresKnowledgeBaseRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        name: row.try_get("name").map_err(|error| error.to_string())?,
        description: row
            .try_get("description")
            .map_err(|error| error.to_string())?,
        library_type: row
            .try_get("library_type")
            .map_err(|error| error.to_string())?,
        tags: row.try_get("tags").map_err(|error| error.to_string())?,
        visibility: row
            .try_get("visibility")
            .map_err(|error| error.to_string())?,
        embedding_model: row
            .try_get("embedding_model")
            .map_err(|error| error.to_string())?,
        chunk_strategy: row
            .try_get("chunk_strategy")
            .map_err(|error| error.to_string())?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn preview_delete_postgres_knowledge_base(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    id: Uuid,
) -> Result<PostgresKnowledgeBaseDeleteImpact, String> {
    let pool = active_pool(&state)?;
    ensure_private_knowledge_base(&pool, id).await?;
    let row = sqlx::query(
        "SELECT kb.name,
                (SELECT COUNT(*) FROM source_documents d WHERE d.knowledge_base_id = kb.id AND d.deleted_at IS NULL) AS document_count,
                (SELECT COUNT(*) FROM document_versions v JOIN source_documents d ON d.id = v.document_id WHERE d.knowledge_base_id = kb.id) AS version_count,
                (SELECT COUNT(*) FROM document_chunks c JOIN document_versions v ON v.id = c.version_id JOIN source_documents d ON d.id = v.document_id WHERE d.knowledge_base_id = kb.id) AS chunk_count,
                (SELECT COUNT(*) FROM document_assets a JOIN document_versions v ON v.id = a.version_id JOIN source_documents d ON d.id = v.document_id WHERE d.knowledge_base_id = kb.id) AS asset_count,
                (SELECT COUNT(*) FROM ingestion_jobs j WHERE j.knowledge_base_id = kb.id AND j.state IN ('queued', 'uploading', 'parsing', 'chunking', 'embedding', 'indexing', 'pending', 'running', 'retrying')) AS active_task_count
         FROM knowledge_bases kb WHERE kb.id = $1 AND kb.deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("读取 PostgreSQL 知识库删除影响失败: {}", safe_error(&error.to_string())))?
    .ok_or_else(|| "PostgreSQL 知识库不存在".to_string())?;
    Ok(PostgresKnowledgeBaseDeleteImpact {
        knowledge_base_id: id.to_string(),
        name: row.try_get("name").map_err(|error| error.to_string())?,
        document_count: row.try_get::<i64, _>("document_count").unwrap_or_default() as u32,
        version_count: row.try_get::<i64, _>("version_count").unwrap_or_default() as u32,
        chunk_count: row.try_get::<i64, _>("chunk_count").unwrap_or_default() as u32,
        asset_count: row.try_get::<i64, _>("asset_count").unwrap_or_default() as u32,
        active_task_count: row
            .try_get::<i64, _>("active_task_count")
            .unwrap_or_default() as u32,
    })
}

#[tauri::command]
pub async fn delete_postgres_knowledge_base(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    id: Uuid,
) -> Result<(), String> {
    let pool = active_pool(&state)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let visibility = sqlx::query_scalar::<_, String>(
        "SELECT visibility FROM knowledge_bases
         WHERE id = $1 AND deleted_at IS NULL
         FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "锁定 PostgreSQL 知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    let Some(visibility) = visibility else {
        return Err("PostgreSQL 知识库不存在".to_string());
    };
    if visibility != "private" {
        return Err("公共知识库为只读，单人客户端不能修改".to_string());
    }
    let active_task_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ingestion_jobs
         WHERE knowledge_base_id = $1 AND state IN ('queued', 'uploading', 'parsing', 'chunking', 'embedding', 'indexing', 'pending', 'running', 'retrying')",
    )
    .bind(id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "检查 PostgreSQL 知识库活动任务失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    if active_task_count > 0 {
        return Err("请先完成、取消或隔离该知识库的活动任务".to_string());
    }
    let changed = sqlx::query(
        "UPDATE knowledge_bases SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "删除 PostgreSQL 知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    if changed.rows_affected() == 0 {
        return Err("PostgreSQL 知识库不存在".to_string());
    }
    // 保留版本、分块和原始文件，但把知识库下的可见实体统一标记为软删除，
    // 避免删除知识库后文档或 Wiki 通过独立查询再次出现。
    for statement in [
        "UPDATE source_documents SET deleted_at = now(), updated_at = now() WHERE knowledge_base_id = $1 AND deleted_at IS NULL",
        "UPDATE wiki_pages SET deleted_at = now(), updated_at = now() WHERE knowledge_base_id = $1 AND deleted_at IS NULL",
        "UPDATE knowledge_edges SET deleted_at = now() WHERE knowledge_base_id = $1 AND deleted_at IS NULL",
    ] {
        sqlx::query(statement)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("清理 PostgreSQL 知识库关联数据失败: {}", safe_error(&error.to_string())))?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| safe_error(&error.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn test_knowledge_database(
    config: KnowledgeDatabaseConfig,
    password: String,
) -> Result<KnowledgeDatabaseHealth, String> {
    let pool = connect(&config, &password).await?;
    let vector_extension = check_vector(&pool).await?;
    let message = if vector_extension {
        "PostgreSQL 连接成功，pgvector 可用".to_string()
    } else {
        "PostgreSQL 连接成功，但未启用 pgvector".to_string()
    };
    pool.close().await;
    Ok(KnowledgeDatabaseHealth {
        configured: true,
        config: Some(config),
        connected: true,
        vector_extension,
        migration_version: None,
        message,
    })
}

#[tauri::command]
pub async fn configure_knowledge_database(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    _state: tauri::State<'_, KnowledgeDatabaseState>,
    config: KnowledgeDatabaseConfig,
    password: String,
) -> Result<KnowledgeDatabaseHealth, String> {
    validate_config(&config)?;
    let pool = connect(&config, &password).await?;
    if !check_vector(&pool).await? {
        pool.close().await;
        return Err("PostgreSQL 未安装或未启用 pgvector 扩展".to_string());
    }
    let previous_config = load_config(&db)?;
    if let Some(previous) = previous_config.as_ref() {
        let has_backup = with_conn(&db, |connection| {
            Ok(settings::get(connection, current_workspace_id(), CONFIG_BACKUP_KEY)?.is_some())
        })?;
        if !has_backup {
            backup_config(&db, previous)?;
        }
    }
    let store = secrets.store();
    let active_ref = secret_ref()?;
    let backup_ref = password_backup_ref()?;
    let has_password_backup = match store.get(&backup_ref) {
        Ok(_) => true,
        Err(error) if error.is_not_found() => false,
        Err(error) => return Err(error.to_string()),
    };
    if !has_password_backup {
        match store.get(&active_ref) {
            Ok(previous_password) => store
                .set(&backup_ref, &previous_password)
                .map_err(|error| error.to_string())?,
            Err(error) if error.is_not_found() => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let credential = SecretValue::new(password).map_err(|error| error.to_string())?;
    store
        .set(&active_ref, &credential)
        .map_err(|error| error.to_string())?;
    let value = serde_json::to_string(&config).map_err(|error| error.to_string())?;
    if let Err(error) = with_conn_mut(&db, |connection| {
        settings::set(connection, current_workspace_id(), CONFIG_KEY, &value)
    }) {
        let rollback = restore_config_backup(&db, &secrets);
        pool.close().await;
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback_error) => format!("{error}; 恢复原 PostgreSQL 配置失败: {rollback_error}"),
        });
    }
    if previous_config.is_none() {
        clear_config_backup(&db, &secrets)?;
    }
    pool.close().await;
    Ok(KnowledgeDatabaseHealth {
        configured: true,
        config: Some(config),
        connected: true,
        vector_extension: true,
        migration_version: None,
        message: "PostgreSQL 已连接，等待初始化知识库".to_string(),
    })
}

#[tauri::command]
pub async fn initialize_knowledge_database(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    state: tauri::State<'_, KnowledgeDatabaseState>,
) -> Result<KnowledgeDatabaseHealth, String> {
    let config = load_config(&db)?.ok_or("尚未配置 PostgreSQL")?;
    let stored_password = read_password(&secrets)?;
    let pool = match connect(&config, stored_password.expose()).await {
        Ok(pool) => pool,
        Err(error) => {
            return Err(match restore_config_backup(&db, &secrets) {
                Ok(()) => error,
                Err(rollback_error) => {
                    format!("{error}; 恢复原 PostgreSQL 配置失败: {rollback_error}")
                }
            });
        }
    };
    let version = match prepare_pool(&pool).await {
        Ok(version) => version,
        Err(error) => {
            pool.close().await;
            return Err(match restore_config_backup(&db, &secrets) {
                Ok(()) => error,
                Err(rollback_error) => {
                    format!("{error}; 恢复原 PostgreSQL 配置失败: {rollback_error}")
                }
            });
        }
    };
    clear_config_backup(&db, &secrets)?;
    *state.pool.lock().map_err(|_| "知识库状态已损坏")? = Some(pool);
    Ok(KnowledgeDatabaseHealth {
        configured: true,
        config: Some(config),
        connected: true,
        vector_extension: true,
        migration_version: Some(version),
        message: "PostgreSQL 知识库初始化完成".to_string(),
    })
}

/// Reconnect a saved knowledge database after the local SQLite database is ready.
/// The operation runs in the background so startup is never blocked by PostgreSQL.
pub(crate) fn auto_initialize_knowledge_database(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let db = app.state::<DbState>();
        let secrets = app.state::<SecretState>();
        let state = app.state::<KnowledgeDatabaseState>();
        let config = match load_config(&db) {
            Ok(Some(config)) => config,
            Ok(None) => return,
            Err(error) => {
                eprintln!("读取已保存的 PostgreSQL 配置失败: {error}");
                return;
            }
        };
        let stored_password = match read_password(&secrets) {
            Ok(password) => password,
            Err(error) => {
                eprintln!("自动连接 PostgreSQL 失败: {error}");
                return;
            }
        };
        let pool = match connect(&config, stored_password.expose()).await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("自动连接 PostgreSQL 失败: {error}");
                return;
            }
        };
        if let Err(error) = prepare_pool(&pool).await {
            pool.close().await;
            eprintln!("自动初始化 PostgreSQL 知识库失败: {error}");
            return;
        }
        match state.pool.lock() {
            Ok(mut active) => *active = Some(pool),
            Err(_) => eprintln!("保存 PostgreSQL 知识库连接失败: 知识库状态已损坏"),
        };
    });
}

#[tauri::command]
pub async fn get_knowledge_database_health(
    db: tauri::State<'_, DbState>,
    state: tauri::State<'_, KnowledgeDatabaseState>,
) -> Result<KnowledgeDatabaseHealth, String> {
    let config = load_config(&db)?;
    let configured = config.is_some();
    let pool = state.pool.lock().map_err(|_| "知识库状态已损坏")?.clone();
    let Some(pool) = pool else {
        return Ok(KnowledgeDatabaseHealth {
            configured,
            config,
            connected: false,
            vector_extension: false,
            migration_version: None,
            message: if configured {
                "PostgreSQL 已配置，尚未连接".to_string()
            } else {
                "尚未配置 PostgreSQL 知识库".to_string()
            },
        });
    };
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        return Ok(KnowledgeDatabaseHealth {
            configured,
            config,
            connected: false,
            vector_extension: false,
            migration_version: None,
            message: "PostgreSQL 连接已失效，请重新初始化知识库".to_string(),
        });
    }
    let vector_extension = check_vector(&pool).await.unwrap_or(false);
    let migration_version =
        sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    Ok(KnowledgeDatabaseHealth {
        configured,
        config,
        connected: true,
        vector_extension,
        migration_version,
        message: if !vector_extension {
            "PostgreSQL 已连接，但未启用 pgvector".to_string()
        } else if migration_version.is_some() {
            "PostgreSQL 知识库已连接".to_string()
        } else if configured {
            "PostgreSQL 已连接，尚未初始化迁移".to_string()
        } else {
            "尚未配置 PostgreSQL 知识库".to_string()
        },
    })
}

#[tauri::command]
pub fn disconnect_knowledge_database(
    state: tauri::State<'_, KnowledgeDatabaseState>,
) -> Result<(), String> {
    *state.pool.lock().map_err(|_| "知识库状态已损坏")? = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_incomplete_config() {
        let config = KnowledgeDatabaseConfig {
            host: String::new(),
            port: 5432,
            database: "suna".to_string(),
            username: "postgres".to_string(),
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn redacts_password_labels_from_database_errors() {
        assert_eq!(
            safe_error("password=secret PASSWORD=secret"),
            "credential=secret credential=secret"
        );
    }

    #[test]
    fn extracts_unique_internal_wiki_links() {
        assert_eq!(
            markdown_wiki_slugs("[A](alpha) [B](/beta#part) [C](https://example.com) [D](alpha)"),
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn encodes_pgvector_and_rejects_non_finite_values() {
        assert_eq!(encode_pgvector(&[0.5, -1.0]).unwrap(), "[0.5,-1]");
        assert!(encode_pgvector(&[f32::NAN]).is_err());
    }

    #[test]
    fn embedding_batches_require_one_finite_dimension() {
        let item = |values: Vec<f32>| PostgresChunkEmbeddingInput {
            chunk_id: Uuid::new_v4(),
            model_id: "bge-m3".to_string(),
            embedding: values,
        };
        assert_eq!(
            validate_embedding_batch(&[item(vec![0.1, 0.2])]).unwrap(),
            2
        );
        assert!(validate_embedding_batch(&[item(vec![0.1]), item(vec![0.2, 0.3])]).is_err());
        assert!(validate_embedding_batch(&[item(vec![f32::INFINITY])]).is_err());
    }

    #[test]
    fn local_permission_rejects_multi_user_sharing_scopes() {
        assert!(validate_local_knowledge_access("private").is_ok());
        assert!(validate_local_knowledge_access("public").is_ok());
        for visibility in ["project", "workspace"] {
            assert!(validate_local_knowledge_access(visibility).is_err());
        }
        assert!(validate_knowledge_visibility("unknown").is_err());
    }

    #[test]
    fn knowledge_base_input_is_normalized_and_deduplicates_tags() {
        let input = normalize_knowledge_base_input(PostgresKnowledgeBaseInput {
            name: "  高强钢研究  ".to_string(),
            description: "  本机资料  ".to_string(),
            library_type: "RESEARCH".to_string(),
            tags: vec![" 钢铁 ".to_string(), "钢铁".to_string(), "".to_string()],
            visibility: " PRIVATE ".to_string(),
            embedding_model: "  text-embedding  ".to_string(),
            chunk_strategy: "SEMANTIC".to_string(),
        })
        .unwrap();

        assert_eq!(input.name, "高强钢研究");
        assert_eq!(input.description, "本机资料");
        assert_eq!(input.library_type, "research");
        assert_eq!(input.visibility, "private");
        assert_eq!(input.tags, vec!["钢铁"]);
        assert_eq!(input.chunk_strategy, "semantic");
    }

    #[test]
    fn knowledge_base_input_rejects_invalid_lifecycle_fields() {
        let base = PostgresKnowledgeBaseInput {
            name: "研究库".to_string(),
            description: String::new(),
            library_type: "research".to_string(),
            tags: Vec::new(),
            visibility: "private".to_string(),
            embedding_model: String::new(),
            chunk_strategy: "semantic".to_string(),
        };
        for invalid in [
            PostgresKnowledgeBaseInput {
                name: "".to_string(),
                ..base.clone()
            },
            PostgresKnowledgeBaseInput {
                library_type: "team".to_string(),
                ..base.clone()
            },
            PostgresKnowledgeBaseInput {
                chunk_strategy: "unknown".to_string(),
                ..base.clone()
            },
            PostgresKnowledgeBaseInput {
                visibility: "workspace".to_string(),
                ..base.clone()
            },
        ] {
            assert!(normalize_knowledge_base_input(invalid).is_err());
        }
    }

    #[test]
    fn spreadsheet_preview_groups_sheets_and_escapes_cell_html() {
        let document = crate::rag::parse::ParsedDocument {
            blocks: vec![crate::rag::parse::DocumentBlock::Table {
                rows: vec![vec!["Grade".to_string(), "<script>".to_string()]],
                location: crate::rag::model::SourceLocation::SheetRange {
                    sheet: "Steel Data".to_string(),
                    range: "A1:B1".to_string(),
                },
            }],
            assets: Vec::new(),
            warnings: Vec::new(),
        };

        let sheets = render_postgres_preview_sheets(document);

        assert_eq!(sheets.len(), 1);
        assert_eq!(sheets[0]["name"], "Steel Data");
        assert!(sheets[0]["html"]
            .as_str()
            .unwrap()
            .contains("&lt;script&gt;"));
        assert!(!sheets[0]["html"].as_str().unwrap().contains("<script>"));
    }

    #[test]
    fn spreadsheet_preview_caps_rows_and_columns() {
        let document = crate::rag::parse::ParsedDocument {
            blocks: vec![crate::rag::parse::DocumentBlock::Table {
                rows: (0..101)
                    .map(|row| (0..41).map(|column| format!("{row}:{column}")).collect())
                    .collect(),
                location: crate::rag::model::SourceLocation::SheetRange {
                    sheet: "Large".to_string(),
                    range: "A1:AO101".to_string(),
                },
            }],
            assets: Vec::new(),
            warnings: Vec::new(),
        };

        let sheets = render_postgres_preview_sheets(document);
        let html = sheets[0]["html"].as_str().unwrap();

        assert_eq!(html.matches("<tr>").count(), 100);
        assert_eq!(html.matches("<td>").count(), 100 * 40);
        assert_eq!(sheets[0]["truncated"], true);
    }

    #[test]
    fn structured_preview_keeps_docx_headings_and_lists() {
        let document = crate::rag::parse::ParsedDocument {
            blocks: vec![
                crate::rag::parse::DocumentBlock::Heading {
                    level: 2,
                    text: "热处理工艺".to_string(),
                    location: crate::rag::model::SourceLocation::Heading {
                        path: vec!["热处理工艺".to_string()],
                    },
                },
                crate::rag::parse::DocumentBlock::List {
                    ordered: false,
                    items: vec!["升温".to_string(), "空冷".to_string()],
                    location: crate::rag::model::SourceLocation::TextOffsets { start: 0, end: 8 },
                },
            ],
            assets: Vec::new(),
            warnings: Vec::new(),
        };

        let blocks = render_postgres_preview_blocks(document);

        assert_eq!(blocks[0]["kind"], "heading");
        assert_eq!(blocks[0]["level"], 2);
        assert_eq!(blocks[1]["kind"], "list");
        assert_eq!(blocks[1]["items"][0], "升温");
    }

    #[test]
    fn postgres_content_path_stays_inside_application_data_root() {
        let root = std::env::temp_dir().join(format!("suna-content-root-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("objects").join("document.pdf");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"pdf").unwrap();

        let authorized = authorize_postgres_content_path(&root, &file).unwrap();
        assert_eq!(authorized, std::fs::canonicalize(&file).unwrap());
        assert!(authorize_postgres_content_path(&root, &root.join(r"..\outside.pdf")).is_err());
        assert!(authorize_postgres_content_path(
            &root,
            std::path::Path::new(r"\\server\share\x.pdf")
        )
        .is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
