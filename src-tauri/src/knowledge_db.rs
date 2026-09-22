use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::providers::capabilities::{EmbeddingProvider, RerankProvider};
use crate::providers::profiles::{ProviderKind, ProviderProfileRecord};
use crate::providers::{
    configured_embedding_provider, configured_rerank_provider, SiliconFlowPlan,
};
use crate::rag::chunk::{chunk_document, ChunkPolicy};
use crate::rag::ingest::{ingest_file, IngestLimits};
use crate::rag::parse::{parse_document, ParseLimits};
use crate::storage::repositories::provider_profiles;
use crate::storage::repositories::settings;
use crate::storage::secrets::{SecretRef, SecretState, SecretValue};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use std::sync::Mutex;
use uuid::Uuid;

const CONFIG_KEY: &str = "knowledge.postgres";
const CONFIG_BACKUP_KEY: &str = "knowledge.postgres.backup";
const PASSWORD_NAME: &str = "knowledge_postgres_password";
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
    pub connected: bool,
    pub vector_extension: bool,
    pub migration_version: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresKnowledgeBaseRecord {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresDocumentImportResponse {
    pub knowledge_base_id: Uuid,
    pub document_id: Uuid,
    pub version_id: Uuid,
    pub chunk_count: u32,
    pub asset_count: u32,
    pub duplicate_content: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresKnowledgeSearchRequest {
    pub knowledge_base_id: Uuid,
    pub query: String,
    pub limit: u32,
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
    pub active_version_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
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
    Ok(version)
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
) -> Result<PostgresDocumentImportResponse, String> {
    let pool = active_pool(&state)?;
    let kb_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(request.knowledge_base_id)
    .fetch_one(&pool)
    .await
    .map_err(|error| {
        format!(
            "检查 PostgreSQL 知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    if !kb_exists {
        return Err("PostgreSQL 知识库不存在".to_string());
    }
    let job_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ingestion_jobs
            (id, knowledge_base_id, state, attempts)
         VALUES ($1, $2, 'running', 1)",
    )
    .bind(job_id)
    .bind(request.knowledge_base_id)
    .execute(&pool)
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
    .execute(&pool)
    .await
    .map_err(|error| {
        format!(
            "创建 PostgreSQL 导入尝试失败: {}",
            safe_error(&error.to_string())
        )
    })?;
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
             SET source_document_id = $1, state = 'completed', updated_at = now()
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
        return Ok(PostgresDocumentImportResponse {
            knowledge_base_id: request.knowledge_base_id,
            document_id,
            version_id,
            chunk_count: 0,
            asset_count: 0,
            duplicate_content: true,
        });
    }
    let source_path = request.source_path.to_string_lossy().to_string();
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
        sqlx::query(
            "UPDATE source_documents SET display_name = $1, source_kind = $2,
                    content_sha256 = $3, updated_at = now()
             WHERE id = $4 AND deleted_at IS NULL",
        )
        .bind(&display_name)
        .bind(source.format.as_str())
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
                (id, knowledge_base_id, display_name, source_kind, source_path, content_sha256)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(document_id)
        .bind(request.knowledge_base_id)
        .bind(&display_name)
        .bind(source.format.as_str())
        .bind(&source_path)
        .bind(&source.content_sha256)
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
             chunk_policy_version, activated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())",
    )
    .bind(version_id)
    .bind(document_id)
    .bind(&source.content_sha256)
    .bind(&source.mime_type)
    .bind("bloomery")
    .bind("v1")
    .bind(ChunkPolicy::default().version)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "写入 PostgreSQL 文档版本失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    for asset in &parsed.assets {
        sqlx::query(
            "INSERT INTO document_assets
                (id, version_id, asset_kind, local_path, mime_type, metadata)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(version_id)
        .bind(&asset.kind)
        .bind(source.storage_key.as_str())
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
         SET source_document_id = $1, state = 'completed', updated_at = now()
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
    Ok(PostgresDocumentImportResponse {
        knowledge_base_id: request.knowledge_base_id,
        document_id,
        version_id,
        chunk_count: chunks.len() as u32,
        asset_count: parsed.assets.len() as u32,
        duplicate_content: false,
    })
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
    sqlx::query(
        "UPDATE ingestion_jobs
         SET state = 'failed', error_message = $1, updated_at = now()
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
    .bind(serde_json::json!({ "stage": error_code }))
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
    let query = request.query.trim();
    if query.is_empty() {
        return Err("检索内容不能为空".to_string());
    }
    let limit = request.limit.clamp(1, 100);
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT c.id, c.version_id, d.id AS document_id, d.knowledge_base_id, d.display_name,
                c.text, c.source_location,
                ts_rank_cd(c.search_vector, plainto_tsquery('simple', $2))::real AS rank
         FROM document_chunks c
         JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
         JOIN source_documents d ON d.id = v.document_id
         WHERE d.knowledge_base_id = $1
           AND d.deleted_at IS NULL
           AND c.search_vector @@ plainto_tsquery('simple', $2)
         ORDER BY rank DESC, c.ordinal
         LIMIT $3",
    )
    .bind(request.knowledge_base_id)
    .bind(query)
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
        "INSERT INTO retrieval_audits (id, knowledge_base_id, query, configuration, evidence)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(request.knowledge_base_id)
    .bind(query)
    .bind(serde_json::json!({ "mode": "full_text", "limit": limit }))
    .bind(serde_json::to_value(&hits).map_err(|error| error.to_string())?)
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
    let pool = active_pool(&state)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    for item in &embeddings {
        let vector = encode_pgvector(&item.embedding)?;
        sqlx::query(
            "INSERT INTO chunk_embeddings (chunk_id, model_id, dimension, embedding)
             VALUES ($1, $2, $3, $4::vector)
             ON CONFLICT (chunk_id) DO UPDATE SET
                model_id = EXCLUDED.model_id,
                dimension = EXCLUDED.dimension,
                embedding = EXCLUDED.embedding,
                created_at = now()",
        )
        .bind(item.chunk_id)
        .bind(model_id.trim())
        .bind(i32::try_from(item.embedding.len()).map_err(|_| "Embedding 维度过大")?)
        .bind(vector)
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
    let record = with_conn(&db, |connection| {
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
    let pool = active_pool(&state)?;
    let rows = sqlx::query(
        "SELECT id, text FROM document_chunks
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
    let inputs = chunk_ids
        .into_iter()
        .zip(response.vectors)
        .map(|(chunk_id, embedding)| PostgresChunkEmbeddingInput {
            chunk_id,
            model_id: response.model_id.clone(),
            embedding,
        })
        .collect::<Vec<_>>();
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    for item in &inputs {
        let vector = encode_pgvector(&item.embedding)?;
        sqlx::query(
            "INSERT INTO chunk_embeddings (chunk_id, model_id, dimension, embedding)
             VALUES ($1, $2, $3, $4::vector)
             ON CONFLICT (chunk_id) DO UPDATE SET
                model_id = EXCLUDED.model_id,
                dimension = EXCLUDED.dimension,
                embedding = EXCLUDED.embedding,
                created_at = now()",
        )
        .bind(item.chunk_id)
        .bind(&item.model_id)
        .bind(i32::try_from(item.embedding.len()).map_err(|_| "Embedding 维度过大")?)
        .bind(vector)
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
         WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
         ORDER BY e.embedding <=> $2::vector, c.ordinal
         LIMIT $3",
    )
    .bind(request.knowledge_base_id)
    .bind(vector)
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
            WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
              AND c.search_vector @@ plainto_tsquery('simple', $2)
            LIMIT $4
         ), dense AS (
            SELECT c.id,
                   ROW_NUMBER() OVER (ORDER BY e.embedding <=> $3::vector, c.ordinal) AS position
            FROM chunk_embeddings e
            JOIN document_chunks c ON c.id = e.chunk_id
            JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
            JOIN source_documents d ON d.id = v.document_id
            WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
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
        "INSERT INTO retrieval_audits (id, knowledge_base_id, query, configuration, evidence)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(request.knowledge_base_id)
    .bind(query)
    .bind(serde_json::json!({ "mode": "hybrid", "limit": limit, "rrf_k": rrf_k }))
    .bind(serde_json::to_value(&hits).map_err(|error| error.to_string())?)
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
) -> Result<Vec<PostgresKnowledgeSearchHit>, String> {
    let vector = encode_pgvector(embedding)?;
    let limit = limit.clamp(1, 100);
    let rrf_k = rrf_k.clamp(1, 10_000);
    let rows = sqlx::query(
        "WITH lexical AS (
            SELECT c.id, ROW_NUMBER() OVER (
              ORDER BY ts_rank_cd(c.search_vector, plainto_tsquery('simple', $2)) DESC, c.ordinal
            ) AS position
            FROM document_chunks c
            JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
            JOIN source_documents d ON d.id = v.document_id
            WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
              AND c.search_vector @@ plainto_tsquery('simple', $2)
            LIMIT $4
         ), dense AS (
            SELECT c.id, ROW_NUMBER() OVER (ORDER BY e.embedding <=> $3::vector, c.ordinal) AS position
            FROM chunk_embeddings e
            JOIN document_chunks c ON c.id = e.chunk_id
            JOIN document_versions v ON v.id = c.version_id AND v.activated_at IS NOT NULL
            JOIN source_documents d ON d.id = v.document_id
            WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
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
    let hit: PostgresKnowledgeSearchHit =
        serde_json::from_value(evidence).map_err(|error| format!("解析 citation 失败: {error}"))?;
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
        "SELECT id, knowledge_base_id, source_document_id, state, attempts,
                error_message, next_attempt_at::text, created_at::text, updated_at::text
         FROM ingestion_jobs WHERE knowledge_base_id = $1
         ORDER BY created_at DESC, id LIMIT 200",
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
        "SELECT id, job_id, error_code, error_message, details, created_at::text
         FROM processing_failures WHERE job_id = $1 ORDER BY created_at DESC, id",
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
    state: tauri::State<'_, KnowledgeDatabaseState>,
    job_id: Uuid,
) -> Result<PostgresIngestionJobRecord, String> {
    let pool = active_pool(&state)?;
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let current = sqlx::query(
        "SELECT id, knowledge_base_id, source_document_id, state, attempts,
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
    let next_attempts = attempts.saturating_add(1);
    let state_name = if next_attempts >= 3 {
        "quarantined"
    } else {
        "retrying"
    };
    sqlx::query(
        "UPDATE ingestion_jobs
         SET state = $1, attempts = $2,
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
    sqlx::query(
        "INSERT INTO ingestion_attempts (id, job_id, state, error_message)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
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
        "SELECT id, knowledge_base_id, source_document_id, state, attempts,
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
    Ok(PostgresIngestionJobRecord {
        id: row.try_get("id").map_err(|error| error.to_string())?,
        knowledge_base_id: row
            .try_get("knowledge_base_id")
            .map_err(|error| error.to_string())?,
        source_document_id: row
            .try_get("source_document_id")
            .map_err(|error| error.to_string())?,
        state: row.try_get("state").map_err(|error| error.to_string())?,
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
                d.source_path, d.content_sha256, v.id AS active_version_id,
                d.created_at::text, d.updated_at::text
         FROM source_documents d
         LEFT JOIN document_versions v
           ON v.document_id = d.id AND v.activated_at IS NOT NULL
         WHERE d.knowledge_base_id = $1 AND d.deleted_at IS NULL
         ORDER BY d.updated_at DESC, d.id",
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
pub async fn delete_postgres_document(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    document_id: Uuid,
) -> Result<(), String> {
    let pool = active_pool(&state)?;
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
        "SELECT EXISTS (SELECT 1 FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL)",
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
         FROM wiki_pages p WHERE p.id = $1 AND p.deleted_at IS NULL",
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
         FROM wiki_page_revisions WHERE page_id = $1 ORDER BY revision DESC",
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
        "SELECT title, body_markdown FROM wiki_page_revisions
         WHERE page_id = $1 AND revision = $2",
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
    let knowledge_base_id: Uuid = sqlx::query_scalar(
        "SELECT knowledge_base_id FROM wiki_pages
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(page_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| {
        format!(
            "读取 Wiki 所属知识库失败: {}",
            safe_error(&error.to_string())
        )
    })?;
    rebuild_wiki_links(&mut transaction, page_id, knowledge_base_id, &body).await?;
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
        "SELECT id, knowledge_base_id, source_page_id, target_page_id,
                relation, metadata, created_at::text
         FROM knowledge_edges
         WHERE knowledge_base_id = $1 AND deleted_at IS NULL
         ORDER BY created_at, id",
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
        "SELECT id, knowledge_base_id, name FROM tags
         WHERE knowledge_base_id = $1 ORDER BY name, id",
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
         FROM tags t JOIN wiki_page_tags pt ON pt.tag_id = t.id
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
        "SELECT knowledge_base_id FROM wiki_pages
         WHERE id = $1 AND deleted_at IS NULL",
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
         RETURNING id, knowledge_base_id, source_page_id, target_page_id, relation, metadata, created_at::text",
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
pub async fn list_postgres_knowledge_bases(
    state: tauri::State<'_, KnowledgeDatabaseState>,
) -> Result<Vec<PostgresKnowledgeBaseRecord>, String> {
    let pool = active_pool(&state)?;
    sqlx::query(
        "SELECT id, name, created_at::text, updated_at::text
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
    name: String,
) -> Result<PostgresKnowledgeBaseRecord, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("知识库名称不能为空".to_string());
    }
    if name.len() > 200 {
        return Err("知识库名称不能超过 200 个字符".to_string());
    }
    let pool = active_pool(&state)?;
    let id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO knowledge_bases (id, name) VALUES ($1, $2)
         RETURNING created_at::text, updated_at::text",
    )
    .bind(id)
    .bind(name)
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
    let pool = active_pool(&state)?;
    let result = sqlx::query(
        "UPDATE knowledge_bases SET name = $1, updated_at = now()
         WHERE id = $2 AND deleted_at IS NULL
         RETURNING id, name, created_at::text, updated_at::text",
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
        created_at: result
            .try_get("created_at")
            .map_err(|error| error.to_string())?,
        updated_at: result
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
    let row = sqlx::query(
        "SELECT kb.name,
                (SELECT COUNT(*) FROM source_documents d WHERE d.knowledge_base_id = kb.id AND d.deleted_at IS NULL) AS document_count,
                (SELECT COUNT(*) FROM document_versions v JOIN source_documents d ON d.id = v.document_id WHERE d.knowledge_base_id = kb.id) AS version_count,
                (SELECT COUNT(*) FROM document_chunks c JOIN document_versions v ON v.id = c.version_id JOIN source_documents d ON d.id = v.document_id WHERE d.knowledge_base_id = kb.id) AS chunk_count
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
        asset_count: 0,
        active_task_count: 0,
    })
}

#[tauri::command]
pub async fn delete_postgres_knowledge_base(
    state: tauri::State<'_, KnowledgeDatabaseState>,
    id: Uuid,
) -> Result<(), String> {
    let pool = active_pool(&state)?;
    let changed = sqlx::query(
        "UPDATE knowledge_bases SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(&pool)
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
    if let Some(previous) = load_config(&db)? {
        backup_config(&db, &previous)?;
    }
    let value = serde_json::to_string(&config).map_err(|error| error.to_string())?;
    with_conn_mut(&db, |connection| {
        settings::set(connection, current_workspace_id(), CONFIG_KEY, &value)
    })?;
    let credential = SecretValue::new(password).map_err(|error| error.to_string())?;
    secrets
        .store()
        .set(&secret_ref()?, &credential)
        .map_err(|error| error.to_string())?;
    pool.close().await;
    Ok(KnowledgeDatabaseHealth {
        configured: true,
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
    let pool = connect(&config, stored_password.expose()).await?;
    let version = match prepare_pool(&pool).await {
        Ok(version) => version,
        Err(error) => {
            pool.close().await;
            return Err(error);
        }
    };
    *state.pool.lock().map_err(|_| "知识库状态已损坏")? = Some(pool);
    Ok(KnowledgeDatabaseHealth {
        configured: true,
        connected: true,
        vector_extension: true,
        migration_version: Some(version),
        message: "PostgreSQL 知识库初始化完成".to_string(),
    })
}

#[tauri::command]
pub async fn get_knowledge_database_health(
    db: tauri::State<'_, DbState>,
    state: tauri::State<'_, KnowledgeDatabaseState>,
) -> Result<KnowledgeDatabaseHealth, String> {
    let configured = load_config(&db)?.is_some();
    let pool = state.pool.lock().map_err(|_| "知识库状态已损坏")?.clone();
    let Some(pool) = pool else {
        return Ok(KnowledgeDatabaseHealth {
            configured,
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
            database: "bloomery".to_string(),
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
}
