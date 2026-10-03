use crate::app::knowledge_commands::logic::{query_postgres_knowledge, LocalKnowledgeQueryRequest};
use crate::db::DbState;
use crate::knowledge_db::{
    normalize_knowledge_base_input, pool_for_query, validate_local_knowledge_access,
    KnowledgeDatabaseState, PostgresDocumentImportRequest, PostgresKnowledgeBaseInput,
    PostgresKnowledgeSearchFilters,
};
use crate::rag::model::KnowledgeBaseId;
use crate::storage::secrets::SecretState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::str::FromStr;
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

const API_ADDR: &str = "127.0.0.1:37641";
const MAX_BODY: usize = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<ApiError>,
    request_id: String,
}
#[derive(Debug, Serialize)]
struct ApiError {
    code: String,
    message: String,
    details: Value,
}

pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(API_ADDR).await {
            Ok(listener) => listener,
            Err(_) => return,
        };
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(value) => value,
                Err(_) => continue,
            };
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = handle_connection(stream, handle).await;
            });
        }
    });
}

async fn handle_connection(mut stream: TcpStream, app: tauri::AppHandle) -> Result<(), String> {
    let (method, path, body) = read_request(&mut stream).await?;
    let request_id = Uuid::new_v4().to_string();
    let response = match app.state::<KnowledgeDatabaseState>().inner().pool_for_api() {
        Ok(pool) => {
            dispatch(
                &method,
                &path,
                &body,
                &pool,
                &app,
                app.state::<KnowledgeDatabaseState>().inner(),
            )
            .await
        }
        Err(error) => Err(("KNOWLEDGE_UNAVAILABLE", error)),
    };
    let status = match response.as_ref() {
        Ok(_) => "200 OK",
        Err((code, _)) => match *code {
            "INVALID_BODY" | "INVALID_ID" => "400 Bad Request",
            "KNOWLEDGE_UNAVAILABLE" | "DATABASE_ERROR" => "503 Service Unavailable",
            "NOT_FOUND" => "404 Not Found",
            "FORBIDDEN" | "READ_ONLY" => "403 Forbidden",
            "CONFLICT" => "409 Conflict",
            _ => "500 Internal Server Error",
        },
    };
    eprintln!(
        "{}",
        crate::diagnostics::observability::redact_line(
            &json!({
                "request_id": request_id.clone(),
                "operation": format!("{method} {path}"),
                "status": status,
            })
            .to_string()
        )
    );
    let payload = match response {
        Ok(data) => serde_json::to_vec(&ApiResponse {
            success: true,
            data: Some(data),
            error: None,
            request_id,
        })
        .unwrap_or_default(),
        Err((code, message)) => serde_json::to_vec(&ApiResponse::<Value> {
            success: false,
            data: None,
            error: Some(ApiError {
                code: code.to_string(),
                message,
                details: json!({}),
            }),
            request_id,
        })
        .unwrap_or_default(),
    };
    let header = format!("HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
    stream
        .write_all(header.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&payload)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn read_request(stream: &mut TcpStream) -> Result<(String, String, Vec<u8>), String> {
    let mut bytes = Vec::new();
    let header_end;
    loop {
        let mut chunk = [0_u8; 8192];
        let count = stream
            .read(&mut chunk)
            .await
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("请求已关闭".to_string());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_BODY {
            return Err("请求体过大".to_string());
        }
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
    }
    let header_text = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or_else(|| "请求行无效".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let path = parts.next().unwrap_or("/").to_string();
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find_map(|(name, value)| {
            (name.eq_ignore_ascii_case("content-length"))
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    if content_length > MAX_BODY {
        return Err("请求体过大".to_string());
    }
    while bytes.len() < header_end + content_length {
        let mut chunk = [0_u8; 8192];
        let count = stream
            .read(&mut chunk)
            .await
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("请求体不完整".to_string());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok((
        method,
        path,
        bytes[header_end..header_end + content_length].to_vec(),
    ))
}

async fn dispatch(
    method: &str,
    path: &str,
    body: &[u8],
    pool: &PgPool,
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
) -> Result<Value, (&'static str, String)> {
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if segments.len() < 2 || segments[0] != "api" || segments[1] != "knowledge" {
        return Err(("NOT_FOUND", "知识 API 路径不存在".to_string()));
    }
    match (method, segments.as_slice()) {
        ("GET", ["api", "knowledge", "health"]) => health(pool).await,
        ("POST", ["api", "knowledge", "upload"]) => upload(app, state, body).await,
        ("POST", ["api", "knowledge", "bases", _, "documents"]) => upload(app, state, body).await,
        ("GET", ["api", "knowledge", "bases"]) => list_bases(pool).await,
        ("POST", ["api", "knowledge", "bases"]) => create_base(pool, body).await,
        ("GET", ["api", "knowledge", "bases", id]) => get_base(pool, id).await,
        ("PATCH", ["api", "knowledge", "bases", id]) => update_base(pool, id, body).await,
        ("DELETE", ["api", "knowledge", "bases", id]) => delete_base(pool, id).await,
        ("GET", ["api", "knowledge", "bases", id, "documents"]) => list_documents(pool, id).await,
        ("GET", ["api", "knowledge", "bases", id, "stats"]) => stats(pool, id).await,
        ("GET", ["api", "knowledge", "documents", id]) => get_document(pool, id).await,
        ("DELETE", ["api", "knowledge", "documents", id]) => delete_document(pool, id).await,
        ("POST", ["api", "knowledge", "documents", id, "reprocess"]) => {
            reprocess(app, state, id).await
        }
        ("POST", ["api", "knowledge", "documents", id, "reindex"]) => {
            reindex(app, state, id, body).await
        }
        ("GET", ["api", "knowledge", "tasks", id]) => get_task(pool, id).await,
        ("POST", ["api", "knowledge", "tasks", id, "cancel"]) => cancel_task(pool, id).await,
        ("POST", ["api", "knowledge", "tasks", id, "retry"]) => {
            retry_task(app, state, pool, id).await
        }
        ("GET", ["api", "knowledge", "tags"]) => list_tags(pool).await,
        ("POST", ["api", "knowledge", "tags"]) => create_tag(pool, body).await,
        ("POST", ["api", "knowledge", "search"]) => search(app, state, pool, body).await,
        _ => Err(("NOT_FOUND", "知识 API 端点不存在".to_string())),
    }
}

async fn health(pool: &PgPool) -> Result<Value, (&'static str, String)> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .map_err(db)?;
    Ok(json!({"status":"ok","database":"connected","api":"127.0.0.1:37641"}))
}

async fn upload(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    raw: &[u8],
) -> Result<Value, (&'static str, String)> {
    let value: Value = body(raw)?;
    let knowledge_base_id = uuid(
        value
            .get("knowledge_base_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    let source_path = value
        .get("source_path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if source_path.is_empty() {
        return Err(("INVALID_BODY", "source_path 不能为空".to_string()));
    }
    let path = std::path::Path::new(source_path);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
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
    ) {
        return Err(("INVALID_BODY", "不支持的文件类型".to_string()));
    }
    let metadata = std::fs::metadata(path)
        .map_err(|error| ("INVALID_BODY", format!("无法读取文件: {error}")))?;
    if !metadata.is_file() {
        return Err(("INVALID_BODY", "source_path 必须指向文件".to_string()));
    }
    if metadata.len() > 100 * 1024 * 1024 {
        return Err(("INVALID_BODY", "文件不能超过 100 MB".to_string()));
    }
    let response = crate::knowledge_db::import_postgres_document_for_api(
        app,
        state,
        PostgresDocumentImportRequest {
            knowledge_base_id,
            source_path: source_path.into(),
            embedding_profile_id: None,
        },
    )
    .await
    .map_err(|error| ("UPLOAD_FAILED", error))?;
    Ok(serde_json::to_value(response).unwrap_or_else(|_| json!({})))
}

async fn reprocess(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    id: &str,
) -> Result<Value, (&'static str, String)> {
    let document_id = uuid(id)?;
    let response = crate::knowledge_db::reparse_postgres_document_for_api(app, state, document_id)
        .await
        .map_err(|error| ("REPROCESS_FAILED", error))?;
    Ok(serde_json::to_value(response).unwrap_or_else(|_| json!({})))
}

async fn reindex(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    id: &str,
    raw: &[u8],
) -> Result<Value, (&'static str, String)> {
    let document_id = uuid(id)?;
    let value: Value = body(raw)?;
    let version_id = uuid(
        value
            .get("version_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    let embedding_profile_id = uuid(
        value
            .get("embedding_profile_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    let count = crate::knowledge_db::embed_postgres_document_for_api(
        app.state::<DbState>().inner(),
        app.state::<SecretState>().inner(),
        state,
        crate::knowledge_db::PostgresEmbeddingRequest {
            version_id,
            embedding_profile_id,
        },
    )
    .await
    .map_err(|error| ("REINDEX_FAILED", error))?;
    Ok(json!({"document_id": document_id, "version_id": version_id, "indexed_chunks": count}))
}

fn uuid(value: &str) -> Result<Uuid, (&'static str, String)> {
    Uuid::parse_str(value).map_err(|_| ("INVALID_ID", "资源 ID 无效".to_string()))
}
fn body<T: for<'de> Deserialize<'de>>(value: &[u8]) -> Result<T, (&'static str, String)> {
    serde_json::from_slice(value).map_err(|error| ("INVALID_BODY", error.to_string()))
}
fn db(error: sqlx::Error) -> (&'static str, String) {
    ("DATABASE_ERROR", error.to_string())
}

async fn require_private_base(pool: &PgPool, id: Uuid) -> Result<(), (&'static str, String)> {
    let visibility = sqlx::query_scalar::<_, String>(
        "SELECT visibility FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(db)?
    .ok_or(("NOT_FOUND", "知识库不存在".to_string()))?;
    if visibility != "private" {
        return Err((
            "READ_ONLY",
            "公共知识库为只读，单人客户端不能修改".to_string(),
        ));
    }
    Ok(())
}

async fn require_active_base(pool: &PgPool, id: Uuid) -> Result<(), (&'static str, String)> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(db)?;
    exists
        .then_some(())
        .ok_or(("NOT_FOUND", "知识库不存在".to_string()))
}

async fn list_bases(pool: &PgPool) -> Result<Value, (&'static str, String)> {
    let rows = sqlx::query("SELECT id,name,description,library_type,tags,visibility,embedding_model,chunk_strategy,created_at::text,updated_at::text FROM knowledge_bases WHERE deleted_at IS NULL ORDER BY updated_at DESC,id")
        .fetch_all(pool).await.map_err(db)?;
    let bases = rows.into_iter().map(|row| {
        Ok(json!({
            "id": row.try_get::<Uuid,_>("id").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "name": row.try_get::<String,_>("name").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "description": row.try_get::<String,_>("description").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "library_type": row.try_get::<String,_>("library_type").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "tags": row.try_get::<Vec<String>,_>("tags").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "visibility": row.try_get::<String,_>("visibility").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "embedding_model": row.try_get::<String,_>("embedding_model").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "chunk_strategy": row.try_get::<String,_>("chunk_strategy").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "created_at": row.try_get::<String,_>("created_at").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,
            "updated_at": row.try_get::<String,_>("updated_at").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?
        }))
    }).collect::<Result<Vec<_>, _>>()?;
    Ok(json!(bases))
}

async fn get_base(pool: &PgPool, id: &str) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    let row = sqlx::query("SELECT id,name,description,library_type,tags,visibility,embedding_model,chunk_strategy,created_at::text,updated_at::text FROM knowledge_bases WHERE id=$1 AND deleted_at IS NULL").bind(id).fetch_optional(pool).await.map_err(db)?.ok_or(("NOT_FOUND", "知识库不存在".to_string()))?;
    Ok(
        json!({"id":row.try_get::<Uuid,_>("id").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"name":row.try_get::<String,_>("name").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"description":row.try_get::<String,_>("description").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"library_type":row.try_get::<String,_>("library_type").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"tags":row.try_get::<Vec<String>,_>("tags").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"visibility":row.try_get::<String,_>("visibility").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"embedding_model":row.try_get::<String,_>("embedding_model").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"chunk_strategy":row.try_get::<String,_>("chunk_strategy").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?}),
    )
}

async fn create_base(pool: &PgPool, raw: &[u8]) -> Result<Value, (&'static str, String)> {
    let input = normalize_knowledge_base_input(body::<PostgresKnowledgeBaseInput>(raw)?)
        .map_err(|error| ("INVALID_BODY", error))?;
    validate_local_knowledge_access(&input.visibility).map_err(|error| ("INVALID_BODY", error))?;
    if input.visibility.trim() != "private" {
        return Err(("READ_ONLY", "单人客户端只能创建私有知识库".to_string()));
    }
    let id = Uuid::new_v4();
    let row=sqlx::query("INSERT INTO knowledge_bases(id,name,description,library_type,tags,visibility,embedding_model,chunk_strategy) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING created_at::text,updated_at::text").bind(id).bind(&input.name).bind(&input.description).bind(&input.library_type).bind(&input.tags).bind(&input.visibility).bind(&input.embedding_model).bind(&input.chunk_strategy).fetch_one(pool).await.map_err(db)?;
    Ok(
        json!({"id":id,"name":input.name,"description":input.description,"library_type":input.library_type,"tags":input.tags,"visibility":input.visibility,"embedding_model":input.embedding_model,"chunk_strategy":input.chunk_strategy,"created_at":row.try_get::<String,_>("created_at").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"updated_at":row.try_get::<String,_>("updated_at").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?}),
    )
}

async fn update_base(pool: &PgPool, id: &str, raw: &[u8]) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    let input = normalize_knowledge_base_input(body::<PostgresKnowledgeBaseInput>(raw)?)
        .map_err(|error| ("INVALID_BODY", error))?;
    validate_local_knowledge_access(&input.visibility).map_err(|error| ("INVALID_BODY", error))?;
    if input.visibility.trim() != "private" {
        return Err((
            "READ_ONLY",
            "公共知识库为只读，不能设置为公共范围".to_string(),
        ));
    }
    require_private_base(pool, id).await?;
    sqlx::query("UPDATE knowledge_bases SET name=$1,description=$2,library_type=$3,tags=$4,visibility=$5,embedding_model=$6,chunk_strategy=$7,updated_at=now() WHERE id=$8 AND deleted_at IS NULL").bind(&input.name).bind(&input.description).bind(&input.library_type).bind(&input.tags).bind(&input.visibility).bind(&input.embedding_model).bind(&input.chunk_strategy).bind(id).execute(pool).await.map_err(db)?;
    get_base(pool, &id.to_string()).await
}
async fn delete_base(pool: &PgPool, id: &str) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    let mut transaction = pool.begin().await.map_err(db)?;
    let visibility = sqlx::query_scalar::<_, String>(
        "SELECT visibility FROM knowledge_bases WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(db)?
    .ok_or(("NOT_FOUND", "知识库不存在".to_string()))?;
    if visibility != "private" {
        return Err((
            "READ_ONLY",
            "公共知识库为只读，单人客户端不能修改".to_string(),
        ));
    }
    let active_task_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ingestion_jobs WHERE knowledge_base_id = $1
         AND state IN ('queued','uploading','parsing','chunking','embedding','indexing','pending','running','retrying')",
    )
    .bind(id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(db)?;
    if active_task_count > 0 {
        return Err((
            "CONFLICT",
            "请先完成、取消或隔离该知识库的活动任务".to_string(),
        ));
    }
    sqlx::query("UPDATE knowledge_bases SET deleted_at=now(),updated_at=now() WHERE id=$1 AND deleted_at IS NULL")
        .bind(id).execute(&mut *transaction).await.map_err(db)?;
    for statement in [
        "UPDATE source_documents SET deleted_at=now(),updated_at=now() WHERE knowledge_base_id=$1 AND deleted_at IS NULL",
        "UPDATE wiki_pages SET deleted_at=now(),updated_at=now() WHERE knowledge_base_id=$1 AND deleted_at IS NULL",
        "UPDATE knowledge_edges SET deleted_at=now() WHERE knowledge_base_id=$1 AND deleted_at IS NULL",
    ] {
        sqlx::query(statement).bind(id).execute(&mut *transaction).await.map_err(db)?;
    }
    transaction.commit().await.map_err(db)?;
    Ok(json!({"deleted":true}))
}
async fn list_documents(pool: &PgPool, id: &str) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    require_active_base(pool, id).await?;
    let rows=sqlx::query("SELECT d.id,d.knowledge_base_id,d.display_name,d.source_kind,d.source_path,d.content_sha256,d.file_size,(SELECT av.id FROM document_versions av WHERE av.document_id=d.id AND av.activated_at IS NOT NULL ORDER BY av.activated_at DESC LIMIT 1) AS active_version_id,d.created_at::text,d.updated_at::text FROM source_documents d WHERE d.knowledge_base_id=$1 AND d.deleted_at IS NULL ORDER BY d.updated_at DESC,d.id LIMIT 200").bind(id).fetch_all(pool).await.map_err(db)?;
    Ok(json!(rows.into_iter().map(|row| json!({"id":row.try_get::<Uuid,_>("id").ok(),"knowledge_base_id":row.try_get::<Uuid,_>("knowledge_base_id").ok(),"display_name":row.try_get::<String,_>("display_name").ok(),"source_kind":row.try_get::<String,_>("source_kind").ok(),"source_path":row.try_get::<String,_>("source_path").ok(),"content_sha256":row.try_get::<String,_>("content_sha256").ok(),"file_size":row.try_get::<i64,_>("file_size").unwrap_or_default(),"active_version_id":row.try_get::<Option<Uuid>,_>("active_version_id").ok(),"created_at":row.try_get::<String,_>("created_at").ok(),"updated_at":row.try_get::<String,_>("updated_at").ok()})).collect::<Vec<_>>()))
}

async fn get_document(pool: &PgPool, id: &str) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    let row=sqlx::query("SELECT d.id,d.knowledge_base_id,d.display_name,d.source_kind,d.source_path,d.content_sha256,d.created_at::text,d.updated_at::text FROM source_documents d JOIN knowledge_bases kb ON kb.id=d.knowledge_base_id AND kb.deleted_at IS NULL WHERE d.id=$1 AND d.deleted_at IS NULL").bind(id).fetch_optional(pool).await.map_err(db)?.ok_or(("NOT_FOUND","文档不存在".to_string()))?;
    Ok(
        json!({"id":row.try_get::<Uuid,_>("id").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"knowledge_base_id":row.try_get::<Uuid,_>("knowledge_base_id").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"display_name":row.try_get::<String,_>("display_name").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"source_kind":row.try_get::<String,_>("source_kind").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"source_path":row.try_get::<String,_>("source_path").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"content_sha256":row.try_get::<String,_>("content_sha256").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"created_at":row.try_get::<String,_>("created_at").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"updated_at":row.try_get::<String,_>("updated_at").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?}),
    )
}
async fn delete_document(pool: &PgPool, id: &str) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    let base_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT knowledge_base_id FROM source_documents WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(db)?
    .ok_or(("NOT_FOUND", "文档不存在".to_string()))?;
    require_private_base(pool, base_id).await?;
    let changed=sqlx::query("UPDATE source_documents SET deleted_at=now(),updated_at=now() WHERE id=$1 AND deleted_at IS NULL").bind(id).execute(pool).await.map_err(db)?;
    if changed.rows_affected() == 0 {
        return Err(("NOT_FOUND", "文档不存在".to_string()));
    }
    Ok(json!({"deleted":true}))
}
async fn get_task(pool: &PgPool, id: &str) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    let row=sqlx::query("SELECT id,knowledge_base_id,source_document_id,state,progress,attempts,error_message,next_attempt_at::text,created_at::text,updated_at::text FROM ingestion_jobs WHERE id=$1").bind(id).fetch_optional(pool).await.map_err(db)?.ok_or(("NOT_FOUND","任务不存在".to_string()))?;
    Ok(
        json!({"id":row.try_get::<Uuid,_>("id").ok(),"knowledge_base_id":row.try_get::<Uuid,_>("knowledge_base_id").ok(),"source_document_id":row.try_get::<Option<Uuid>,_>("source_document_id").ok(),"state":row.try_get::<String,_>("state").ok(),"progress":row.try_get::<i32,_>("progress").ok(),"attempts":row.try_get::<i32,_>("attempts").ok(),"error_message":row.try_get::<Option<String>,_>("error_message").ok(),"next_attempt_at":row.try_get::<Option<String>,_>("next_attempt_at").ok(),"created_at":row.try_get::<String,_>("created_at").ok(),"updated_at":row.try_get::<String,_>("updated_at").ok()}),
    )
}
async fn cancel_task(pool: &PgPool, id: &str) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    let knowledge_base_id: Uuid =
        sqlx::query_scalar("SELECT knowledge_base_id FROM ingestion_jobs WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(db)?
            .ok_or(("NOT_FOUND", "任务不存在或已结束".to_string()))?;
    require_private_base(pool, knowledge_base_id).await?;
    let row = sqlx::query("UPDATE ingestion_jobs SET state='cancelled', progress=100, updated_at=now() WHERE id=$1 AND state IN ('queued','uploading','parsing','chunking','embedding','indexing','pending','running','retrying') RETURNING id,state,progress,updated_at::text")
        .bind(id).fetch_optional(pool).await.map_err(db)?.ok_or(("NOT_FOUND", "任务不存在或已结束".to_string()))?;
    Ok(
        json!({"id": row.try_get::<Uuid,_>("id").ok(), "state": row.try_get::<String,_>("state").ok(), "progress": row.try_get::<i32,_>("progress").ok(), "updated_at": row.try_get::<String,_>("updated_at").ok()}),
    )
}
async fn retry_task(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    _pool: &PgPool,
    id: &str,
) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    let response = crate::knowledge_db::retry_postgres_ingestion_job_for_api(app, state, id)
        .await
        .map_err(|error| ("RETRY_FAILED", error))?;
    serde_json::to_value(response).map_err(|error| ("RETRY_FAILED", error.to_string()))
}
async fn list_tags(pool: &PgPool) -> Result<Value, (&'static str, String)> {
    let rows = sqlx::query("SELECT t.id,t.knowledge_base_id,t.name FROM tags t JOIN knowledge_bases kb ON kb.id=t.knowledge_base_id AND kb.deleted_at IS NULL ORDER BY t.name")
        .fetch_all(pool)
        .await
        .map_err(db)?;
    Ok(json!(rows.into_iter().map(|row| json!({"id":row.try_get::<Uuid,_>("id").ok(),"knowledge_base_id":row.try_get::<Uuid,_>("knowledge_base_id").ok(),"name":row.try_get::<String,_>("name").ok()})).collect::<Vec<_>>()))
}
async fn create_tag(pool: &PgPool, raw: &[u8]) -> Result<Value, (&'static str, String)> {
    let value: Value = body(raw)?;
    let kb = uuid(
        value
            .get("knowledge_base_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if name.is_empty() {
        return Err(("INVALID_BODY", "标签名称不能为空".to_string()));
    }
    require_private_base(pool, kb).await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO tags(id,knowledge_base_id,name) VALUES($1,$2,$3) ON CONFLICT(knowledge_base_id,name) DO UPDATE SET name=EXCLUDED.name").bind(id).bind(kb).bind(name).execute(pool).await.map_err(db)?;
    Ok(json!({"id":id,"knowledge_base_id":kb,"name":name}))
}
async fn stats(pool: &PgPool, id: &str) -> Result<Value, (&'static str, String)> {
    let id = uuid(id)?;
    require_active_base(pool, id).await?;
    let row=sqlx::query("SELECT (SELECT COUNT(*) FROM source_documents WHERE knowledge_base_id=$1 AND deleted_at IS NULL) AS document_count,(SELECT COUNT(*) FROM document_chunks c JOIN document_versions v ON v.id=c.version_id JOIN source_documents d ON d.id=v.document_id WHERE d.knowledge_base_id=$1 AND d.deleted_at IS NULL) AS chunk_count,(SELECT COUNT(*) FROM chunk_embeddings e JOIN document_chunks c ON c.id=e.chunk_id JOIN document_versions v ON v.id=c.version_id JOIN source_documents d ON d.id=v.document_id WHERE d.knowledge_base_id=$1 AND d.deleted_at IS NULL) AS indexed_chunk_count,(SELECT COUNT(*) FROM ingestion_jobs WHERE knowledge_base_id=$1 AND state='completed') AS processing_success_count,(SELECT COUNT(*) FROM ingestion_jobs WHERE knowledge_base_id=$1 AND state IN ('failed','quarantined')) AS processing_failure_count,(SELECT COUNT(*) FROM retrieval_audits WHERE knowledge_base_id=$1) AS retrieval_count,(SELECT AVG(duration_ms)::double precision FROM retrieval_audits WHERE knowledge_base_id=$1 AND duration_ms > 0) AS average_retrieval_duration_ms,(SELECT AVG(EXTRACT(EPOCH FROM (finished_at-started_at))*1000)::double precision FROM ingestion_attempts a JOIN ingestion_jobs j ON j.id=a.job_id WHERE j.knowledge_base_id=$1 AND a.finished_at IS NOT NULL) AS average_processing_duration_ms").bind(id).fetch_one(pool).await.map_err(db)?;
    Ok(
        json!({"knowledge_base_id":id,"document_count":row.try_get::<i64,_>("document_count").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"chunk_count":row.try_get::<i64,_>("chunk_count").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"indexed_chunk_count":row.try_get::<i64,_>("indexed_chunk_count").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"processing_success_count":row.try_get::<i64,_>("processing_success_count").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"processing_failure_count":row.try_get::<i64,_>("processing_failure_count").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"retrieval_count":row.try_get::<i64,_>("retrieval_count").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"average_retrieval_duration_ms":row.try_get::<Option<f64>,_>("average_retrieval_duration_ms").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?,"average_processing_duration_ms":row.try_get::<Option<f64>,_>("average_processing_duration_ms").map_err(|e| db(sqlx::Error::Decode(Box::new(e))))?}),
    )
}
async fn search(
    app: &tauri::AppHandle,
    state: &KnowledgeDatabaseState,
    pool: &PgPool,
    raw: &[u8],
) -> Result<Value, (&'static str, String)> {
    let (kb, query, limit, filters) = parse_search_request(raw)?;
    require_active_base(pool, kb).await?;
    let knowledge_base_id = KnowledgeBaseId::from_str(&kb.to_string())
        .map_err(|error| ("INVALID_ID", error.to_string()))?;
    let request = LocalKnowledgeQueryRequest {
        query: query.clone(),
        knowledge_base_ids: vec![knowledge_base_id],
        lexical_limit: (limit as usize * 3).min(100),
        dense_limit: (limit as usize * 3).min(100),
        candidate_limit: limit as usize,
        rrf_k: 60,
        rerank_limit: limit as usize,
        similarity_threshold: None,
        degradation_policy: None,
        filters: Some(filters.clone()),
    };
    let secrets = app.state::<SecretState>();
    let pack = query_postgres_knowledge(app, &secrets, state, request)
        .await
        .map_err(|error| ("DATABASE_ERROR", error))?;
    let mode = if pack.configuration.dense_limit > 0
        && pack.configuration.embedding_model_id != "tsvector"
    {
        "postgresql_hybrid"
    } else {
        "postgresql_fts"
    };
    let results = pack
        .evidence
        .iter()
        .map(|item| {
            json!({
                "knowledge_base_id": item.chunk.knowledge_base_id,
                "chunk_id": item.chunk.chunk_id,
                "version_id": item.chunk.version_id,
                "document_id": item.chunk.document_id,
                "document_name": item.chunk.source_name,
                "text": item.chunk.text,
                "source_location": item.chunk.source_location,
                "rank": item
                    .chunk
                    .rerank_score
                    .map(f64::from)
                    .unwrap_or(item.chunk.rrf_score),
                "citation_number": item.citation_number,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "knowledge_base_id": kb,
        "results": results,
        "limit": limit,
        "filters": filters,
        "mode": mode,
        "embedding_degradation": pack.configuration.embedding_degradation,
        "rerank_degradation": pack.configuration.rerank_degradation,
        "evidence_pack_id": pack.id,
        "created_at": pack.created_at,
    }))
}

fn parse_search_request(
    raw: &[u8],
) -> Result<(Uuid, String, i64, PostgresKnowledgeSearchFilters), (&'static str, String)> {
    let value: Value = body(raw)?;
    let kb = uuid(
        value
            .get("knowledge_base_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    let query = value
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if query.is_empty() {
        return Err(("INVALID_BODY", "检索内容不能为空".to_string()));
    }
    let limit = value
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 100) as i64;
    let filters = value
        .get("filters")
        .cloned()
        .map(serde_json::from_value::<PostgresKnowledgeSearchFilters>)
        .transpose()
        .map_err(|error| ("INVALID_BODY", format!("检索过滤条件无效: {error}")))?
        .unwrap_or_default();
    Ok((kb, query, limit, filters))
}

trait ApiPool {
    fn pool_for_api(&self) -> Result<PgPool, String>;
}
impl ApiPool for KnowledgeDatabaseState {
    fn pool_for_api(&self) -> Result<PgPool, String> {
        pool_for_query(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_ID: &str = "11111111-1111-4111-8111-111111111111";

    #[test]
    fn search_request_requires_query_and_valid_filters() {
        let empty = serde_json::json!({"knowledge_base_id": BASE_ID, "query": "  "});
        assert_eq!(
            parse_search_request(&serde_json::to_vec(&empty).unwrap())
                .unwrap_err()
                .0,
            "INVALID_BODY"
        );

        let invalid = serde_json::json!({
            "knowledge_base_id": BASE_ID,
            "query": "Q355B",
            "filters": {"year": "twenty twenty-four"}
        });
        assert_eq!(
            parse_search_request(&serde_json::to_vec(&invalid).unwrap())
                .unwrap_err()
                .0,
            "INVALID_BODY"
        );
    }

    #[test]
    fn search_request_trims_query_clamps_limit_and_keeps_filters() {
        let request = serde_json::json!({
            "knowledge_base_id": BASE_ID,
            "query": "  Q355B  ",
            "limit": 1000,
            "filters": {"material": "Q355B", "year": 2024}
        });
        let (id, query, limit, filters) =
            parse_search_request(&serde_json::to_vec(&request).unwrap()).unwrap();
        assert_eq!(id.to_string(), BASE_ID);
        assert_eq!(query, "Q355B");
        assert_eq!(limit, 100);
        assert_eq!(filters.material.as_deref(), Some("Q355B"));
        assert_eq!(filters.year, Some(2024));
    }
}
