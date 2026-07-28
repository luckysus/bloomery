use crate::auth::AuthState;
use crate::db::{upsert_cloud_job_for_user, with_conn, DbState};
use crate::models::{CloudJob, CloudJobInput};
use reqwest::Method;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

const CLOUD_API_BASE_KEY: &str = "cloud_api_base";
const CLOUD_API_BASE_NOT_CONFIGURED: &str =
    "云端服务未配置：请在设置页填写云端 API 地址后再使用云任务功能";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCloudTaskRequest {
    path: String,
    method: Option<String>,
    body: Option<Value>,
    mirror: Option<DesktopCloudTaskMirror>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCloudTaskMirror {
    job_type: String,
    cloud_job_id: Option<String>,
    status: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DesktopCloudTaskResponse {
    status: u16,
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCloudBinaryRequest {
    path: String,
    method: Option<String>,
    content_type: Option<String>,
    bytes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCloudDownloadRequest {
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCloudDownloadResponse {
    status: u16,
    bytes: Vec<u8>,
    content_type: Option<String>,
    content_disposition: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCloudJobsResponse {
    synced: usize,
    failed: usize,
    jobs: Vec<CloudJob>,
}

#[tauri::command]
pub async fn desktop_cloud_task_request(
    auth: tauri::State<'_, AuthState>,
    db: tauri::State<'_, DbState>,
    request: DesktopCloudTaskRequest,
) -> Result<DesktopCloudTaskResponse, String> {
    let session = auth.current_session()?;
    let path = normalize_allowed_path(&request.path)?;
    let method_text = request.method.as_deref().unwrap_or("GET").to_uppercase();
    let method = Method::from_bytes(method_text.as_bytes()).map_err(|err| err.to_string())?;
    ensure_allowed_method_for_path(&path, &method)?;
    let cloud_base = require_cloud_api_base(&db, &session.user_id)?;
    let url = format!("{}/{}", cloud_base.trim_end_matches('/'), path);

    let mut mirrored_local_id = None;
    let mut mirrored_cloud_job_id = None;
    if let Some(mirror) = request.mirror.as_ref() {
        let cloud_job_id = mirror
            .cloud_job_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("pending-{}", Uuid::new_v4()));
        let saved = mirror_cloud_job(
            &db,
            &session.user_id,
            mirror,
            None,
            &cloud_job_id,
            mirror.status.as_deref().unwrap_or("running"),
            request.body.as_ref().map(|body| {
                serde_json::json!({
                    "source": mirror.source.as_deref().unwrap_or("desktop_cloud_task"),
                    "path": path,
                    "method": method_text,
                    "payload": body,
                })
            }),
            None,
        )?;
        mirrored_local_id = Some(saved.id);
        mirrored_cloud_job_id = Some(cloud_job_id);
    }

    let client = reqwest::Client::new();
    let mut builder = client.request(method.clone(), url);
    if !session.token.trim().is_empty() {
        builder = builder.bearer_auth(session.token.trim());
    }
    if let Some(body) = request.body.as_ref() {
        builder = builder.json(body);
    }
    let response = match builder.send().await {
        Ok(response) => response,
        Err(err) => {
            let error_text = format!("cloud task request failed: {err}");
            if let (Some(mirror), Some(local_id), Some(cloud_job_id)) = (
                request.mirror.as_ref(),
                mirrored_local_id.as_deref(),
                mirrored_cloud_job_id.as_deref(),
            ) {
                mirror_cloud_job(
                    &db,
                    &session.user_id,
                    mirror,
                    Some(local_id),
                    cloud_job_id,
                    "failed",
                    None,
                    Some(serde_json::json!({ "error": error_text })),
                )
                .map_err(|mirror_err| {
                    format!("{error_text}; failed to update local mirror: {mirror_err}")
                })?;
            }
            return Err(error_text);
        }
    };
    let status = response.status().as_u16();
    let ok = response.status().is_success();
    let body_text = response.text().await.map_err(|err| err.to_string())?;
    let body_value = serde_json::from_str::<Value>(&body_text).ok();

    if let Some(mirror) = request.mirror.as_ref() {
        let cloud_job_id = body_value
            .as_ref()
            .and_then(extract_job_id)
            .or_else(|| mirror.cloud_job_id.clone())
            .unwrap_or_else(|| {
                mirrored_local_id
                    .as_deref()
                    .and_then(|id| id.split_once(':').map(|(_, value)| value.to_string()))
                    .unwrap_or_else(|| format!("pending-{}", Uuid::new_v4()))
            });
        let next_status = body_value
            .as_ref()
            .and_then(extract_status)
            .unwrap_or_else(|| default_status(&path, &method, ok, mirror));
        mirror_cloud_job(
            &db,
            &session.user_id,
            mirror,
            mirrored_local_id.as_deref(),
            &cloud_job_id,
            &next_status,
            None,
            Some(
                body_value
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({ "body": body_text })),
            ),
        )?;
    }

    if let Some(value) = body_value.as_ref() {
        mirror_known_job_lists(&db, &session.user_id, &path, value)?;
    }

    Ok(DesktopCloudTaskResponse {
        status,
        body: body_text,
    })
}

#[tauri::command]
pub async fn desktop_cloud_binary_request(
    auth: tauri::State<'_, AuthState>,
    db: tauri::State<'_, DbState>,
    request: DesktopCloudBinaryRequest,
) -> Result<DesktopCloudTaskResponse, String> {
    let session = auth.current_session()?;
    let path = normalize_allowed_binary_path(&request.path)?;
    let method_text = request.method.as_deref().unwrap_or("POST").to_uppercase();
    let method = Method::from_bytes(method_text.as_bytes()).map_err(|err| err.to_string())?;
    if method != Method::POST {
        return Err("binary cloud task only supports POST".to_string());
    }
    let cloud_base = require_cloud_api_base(&db, &session.user_id)?;
    let url = format!("{}/{}", cloud_base.trim_end_matches('/'), path);

    let mut builder = reqwest::Client::new().request(method, url);
    if !session.token.trim().is_empty() {
        builder = builder.bearer_auth(session.token.trim());
    }
    if let Some(content_type) = request
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        builder = builder.header(reqwest::header::CONTENT_TYPE, content_type);
    }
    let response = builder
        .body(request.bytes)
        .send()
        .await
        .map_err(|err| format!("cloud binary request failed: {err}"))?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|err| err.to_string())?;
    Ok(DesktopCloudTaskResponse { status, body })
}

#[tauri::command]
pub async fn desktop_cloud_download_request(
    auth: tauri::State<'_, AuthState>,
    db: tauri::State<'_, DbState>,
    request: DesktopCloudDownloadRequest,
) -> Result<DesktopCloudDownloadResponse, String> {
    let session = auth.current_session()?;
    let path = normalize_allowed_download_path(&request.path)?;
    let cloud_base = require_cloud_api_base(&db, &session.user_id)?;
    let url = format!("{}/{}", cloud_base.trim_end_matches('/'), path);
    let mut builder = reqwest::Client::new().get(url);
    if !session.token.trim().is_empty() {
        builder = builder.bearer_auth(session.token.trim());
    }
    let response = builder
        .send()
        .await
        .map_err(|err| format!("cloud download request failed: {err}"))?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let content_disposition = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = response
        .bytes()
        .await
        .map_err(|err| err.to_string())?
        .to_vec();
    Ok(DesktopCloudDownloadResponse {
        status,
        bytes,
        content_type,
        content_disposition,
    })
}

#[tauri::command]
pub async fn sync_cloud_jobs(
    auth: tauri::State<'_, AuthState>,
    db: tauri::State<'_, DbState>,
) -> Result<SyncCloudJobsResponse, String> {
    let session = auth.current_session()?;
    let Some(cloud_base) = load_cloud_api_base(&db, &session.user_id)? else {
        // 未配置云端服务时不产生同步噪音，直接返回本地镜像。
        return Ok(SyncCloudJobsResponse {
            synced: 0,
            failed: 0,
            jobs: load_user_cloud_jobs(&db, &session.user_id)?,
        });
    };
    let current_jobs = load_user_cloud_jobs(&db, &session.user_id)?;
    let client = reqwest::Client::new();
    let mut synced = 0usize;
    let mut failed = 0usize;

    for job in current_jobs
        .iter()
        .filter(|job| job.r#type == "training" && should_sync_job(job))
    {
        let path = format!("api/training/status/{}", job.cloud_job_id);
        match fetch_cloud_json(&client, &cloud_base, session.token.as_str(), &path).await {
            Ok(value) => {
                mirror_job_status_value(
                    &db,
                    &session.user_id,
                    "training",
                    &job.cloud_job_id,
                    &value,
                    "training_status_sync",
                )?;
                synced += 1;
            }
            Err(_) => failed += 1,
        }
    }

    if current_jobs
        .iter()
        .any(|job| job.r#type == "optimization" && should_sync_job(job))
    {
        match fetch_cloud_json(
            &client,
            &cloud_base,
            session.token.as_str(),
            "api/optimize/recent?limit=50",
        )
        .await
        {
            Ok(value) => {
                mirror_known_job_lists(
                    &db,
                    &session.user_id,
                    "api/optimize/recent?limit=50",
                    &value,
                )?;
                synced += 1;
            }
            Err(_) => failed += 1,
        }
    }

    if current_jobs
        .iter()
        .any(|job| job.r#type == "literature" && should_sync_job(job))
    {
        match fetch_cloud_json(
            &client,
            &cloud_base,
            session.token.as_str(),
            "api/literature/jobs",
        )
        .await
        {
            Ok(value) => {
                mirror_known_job_lists(&db, &session.user_id, "api/literature/jobs", &value)?;
                synced += 1;
            }
            Err(_) => failed += 1,
        }
    }

    Ok(SyncCloudJobsResponse {
        synced,
        failed,
        jobs: load_user_cloud_jobs(&db, &session.user_id)?,
    })
}

fn normalize_allowed_path(path: &str) -> Result<String, String> {
    let path = path.trim().trim_start_matches('/').to_string();
    reject_suspicious_path(&path)?;
    let allowed = path == "api/overview"
        || path == "api/search"
        || path == "api/coil_match"
        || path == "api/lab-service/status"
        || path == "api/lab-service/status?refresh=1"
        || path == "api/lab-service/reconnect"
        || path.starts_with("api/training/")
        || path == "api/optimize"
        || path.starts_with("api/optimize/")
        || path.starts_with("api/literature/");
    if allowed {
        Ok(path)
    } else {
        Err("cloud task path is not allowed".to_string())
    }
}

fn normalize_allowed_binary_path(path: &str) -> Result<String, String> {
    let path = path.trim().trim_start_matches('/').to_string();
    reject_suspicious_path(&path)?;
    if path.starts_with("api/literature/upload?") {
        Ok(path)
    } else {
        Err("binary cloud task path is not allowed".to_string())
    }
}

fn normalize_allowed_download_path(path: &str) -> Result<String, String> {
    let path = path.trim().trim_start_matches('/').to_string();
    reject_suspicious_path(&path)?;
    if path == "api/export"
        || path.starts_with("api/export?")
        || path.starts_with("api/literature/files/raw?")
        || path.starts_with("api/literature/files/image?")
    {
        Ok(path)
    } else {
        Err("cloud download path is not allowed".to_string())
    }
}

fn ensure_allowed_method_for_path(path: &str, method: &Method) -> Result<(), String> {
    let ok = match path {
        "api/overview" => method == Method::GET,
        "api/search" | "api/coil_match" => method == Method::POST,
        "api/lab-service/status" | "api/lab-service/status?refresh=1" => method == Method::GET,
        "api/lab-service/reconnect" => method == Method::POST,
        "api/training/start" => method == Method::POST,
        "api/training/models" | "api/training/latest" => method == Method::GET,
        "api/optimize" => method == Method::POST,
        "api/optimize/logs" => method == Method::GET,
        "api/optimize/cancel" => method == Method::POST,
        "api/literature/folders" | "api/literature/jobs" => method == Method::GET,
        "api/literature/process"
        | "api/literature/files/rename"
        | "api/literature/folders/merge" => method == Method::POST,
        _ if path.starts_with("api/training/status/") => method == Method::GET,
        _ if path.starts_with("api/training/cancel/") => method == Method::POST,
        _ if path.starts_with("api/training/models/") && path.ends_with("/logs") => {
            method == Method::GET
        }
        _ if path.starts_with("api/training/models/") && path.ends_with("/activate") => {
            method == Method::POST
        }
        _ if path.starts_with("api/training/models/") => method == Method::DELETE,
        _ if path.starts_with("api/optimize/recent?") => method == Method::GET,
        _ if path.starts_with("api/literature/folders?") => method == Method::DELETE,
        _ if path.starts_with("api/literature/files?") => {
            method == Method::GET || method == Method::DELETE
        }
        _ if path.starts_with("api/literature/files/preview?") => method == Method::GET,
        _ if path.starts_with("api/literature/jobs/") && path.ends_with("/logs") => {
            method == Method::GET
        }
        _ if path.starts_with("api/literature/jobs/") => method == Method::DELETE,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err("cloud task method is not allowed".to_string())
    }
}

fn reject_suspicious_path(path: &str) -> Result<(), String> {
    let lower = path.to_lowercase();
    if path.chars().any(char::is_whitespace)
        || path.contains('\\')
        || path.contains("..")
        || lower.contains("%2e")
        || lower.contains("%5c")
    {
        return Err("cloud task path is not allowed".to_string());
    }
    Ok(())
}

fn load_cloud_api_base(db: &tauri::State<DbState>, user_id: &str) -> Result<Option<String>, String> {
    let raw = with_conn(db, |conn| {
        conn.query_row(
            "SELECT value_json FROM settings WHERE user_id = ?1 AND key = ?2",
            params![user_id, CLOUD_API_BASE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| err.to_string())
    })?;
    Ok(raw
        .map(|value| serde_json::from_str::<String>(&value).unwrap_or(value))
        .filter(|value| !value.trim().is_empty()))
}

fn require_cloud_api_base(db: &tauri::State<DbState>, user_id: &str) -> Result<String, String> {
    load_cloud_api_base(db, user_id)?.ok_or_else(|| CLOUD_API_BASE_NOT_CONFIGURED.to_string())
}

fn mirror_cloud_job(
    db: &tauri::State<DbState>,
    user_id: &str,
    mirror: &DesktopCloudTaskMirror,
    local_id: Option<&str>,
    cloud_job_id: &str,
    status: &str,
    payload: Option<Value>,
    result: Option<Value>,
) -> Result<crate::models::CloudJob, String> {
    upsert_cloud_job_for_user(
        db,
        user_id,
        CloudJobInput {
            id: local_id.map(str::to_string),
            conversation_id: None,
            cloud_job_id: cloud_job_id.to_string(),
            r#type: mirror.job_type.trim().to_string(),
            status: status.to_string(),
            payload_json: payload.map(|value| value.to_string()),
            result_json: result.map(|value| value.to_string()),
        },
    )
}

fn extract_job_id(value: &Value) -> Option<String> {
    value
        .get("job_id")
        .or_else(|| value.get("remote_job_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn extract_status(value: &Value) -> Option<String> {
    value
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn default_status(
    path: &str,
    method: &Method,
    ok: bool,
    mirror: &DesktopCloudTaskMirror,
) -> String {
    if !ok {
        return "failed".to_string();
    }
    if method == Method::DELETE {
        return "deleted".to_string();
    }
    if mirror.job_type == "optimization" && method == Method::POST && path == "api/optimize" {
        return "completed".to_string();
    }
    "running".to_string()
}

fn mirror_known_job_lists(
    db: &tauri::State<DbState>,
    user_id: &str,
    path: &str,
    value: &Value,
) -> Result<(), String> {
    if path.starts_with("api/optimize/recent") {
        if let Some(jobs) = value.as_array() {
            for job in jobs {
                mirror_job_from_value(db, user_id, "optimization", job, "optimizer_recent")?;
            }
        }
    } else if path == "api/literature/jobs" {
        if let Some(jobs) = value.get("jobs").and_then(Value::as_array) {
            for job in jobs {
                mirror_job_from_value(db, user_id, "literature", job, "literature_jobs")?;
            }
        }
    } else if path == "api/training/latest" {
        mirror_job_from_value(db, user_id, "training", value, "training_latest")?;
    }
    Ok(())
}

fn mirror_job_from_value(
    db: &tauri::State<DbState>,
    user_id: &str,
    job_type: &str,
    value: &Value,
    source: &str,
) -> Result<(), String> {
    let Some(cloud_job_id) = extract_job_id(value) else {
        return Ok(());
    };
    let status = extract_status(value).unwrap_or_else(|| "running".to_string());
    let mirror = DesktopCloudTaskMirror {
        job_type: job_type.to_string(),
        cloud_job_id: Some(cloud_job_id.clone()),
        status: Some(status.clone()),
        source: Some(source.to_string()),
    };
    mirror_cloud_job(
        db,
        user_id,
        &mirror,
        None,
        &cloud_job_id,
        &status,
        Some(serde_json::json!({ "source": source })),
        Some(value.clone()),
    )?;
    Ok(())
}

fn mirror_job_status_value(
    db: &tauri::State<DbState>,
    user_id: &str,
    job_type: &str,
    cloud_job_id: &str,
    value: &Value,
    source: &str,
) -> Result<(), String> {
    let status = extract_status(value).unwrap_or_else(|| "running".to_string());
    let mirror = DesktopCloudTaskMirror {
        job_type: job_type.to_string(),
        cloud_job_id: Some(cloud_job_id.to_string()),
        status: Some(status.clone()),
        source: Some(source.to_string()),
    };
    mirror_cloud_job(
        db,
        user_id,
        &mirror,
        None,
        cloud_job_id,
        &status,
        Some(serde_json::json!({ "source": source })),
        Some(value.clone()),
    )?;
    Ok(())
}

fn should_sync_job(job: &CloudJob) -> bool {
    let status = job.status.trim().to_lowercase();
    let cloud_job_id = job.cloud_job_id.trim();
    !cloud_job_id.is_empty()
        && !cloud_job_id.starts_with("pending-")
        && !matches!(
            status.as_str(),
            "completed" | "failed" | "cancelled" | "deleted" | "needs_input"
        )
}

fn load_user_cloud_jobs(
    db: &tauri::State<DbState>,
    user_id: &str,
) -> Result<Vec<CloudJob>, String> {
    with_conn(db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, conversation_id, cloud_job_id, type, status, payload_json, result_json, created_at, updated_at
                 FROM cloud_jobs
                 WHERE user_id = ?1
                 ORDER BY updated_at DESC",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params![user_id], |row| {
                Ok(CloudJob {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    cloud_job_id: row.get(2)?,
                    r#type: row.get(3)?,
                    status: row.get(4)?,
                    payload_json: row.get(5)?,
                    result_json: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|err| err.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())
    })
}

async fn fetch_cloud_json(
    client: &reqwest::Client,
    cloud_base: &str,
    token: &str,
    path: &str,
) -> Result<Value, String> {
    let url = format!("{}/{}", cloud_base.trim_end_matches('/'), path);
    let mut builder = client.get(url);
    if !token.trim().is_empty() {
        builder = builder.bearer_auth(token.trim());
    }
    let response = builder
        .send()
        .await
        .map_err(|err| format!("cloud job sync failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("cloud job sync failed: HTTP {}", response.status()));
    }
    response
        .json::<Value>()
        .await
        .map_err(|err| format!("parse cloud job sync response failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_allows_known_cloud_task_paths() {
        assert!(normalize_allowed_path("/api/overview").is_ok());
        assert!(normalize_allowed_path("/api/search").is_ok());
        assert!(normalize_allowed_path("/api/coil_match").is_ok());
        assert!(normalize_allowed_path("/api/lab-service/status?refresh=1").is_ok());
        assert!(normalize_allowed_path("/api/lab-service/reconnect").is_ok());
        assert!(normalize_allowed_path("/api/training/start").is_ok());
        assert!(normalize_allowed_path("api/optimize").is_ok());
        assert!(normalize_allowed_path("api/literature/jobs").is_ok());
        assert!(normalize_allowed_path("api/ask").is_err());
        assert!(normalize_allowed_path("api/agent/chat").is_err());
        assert!(normalize_allowed_path("api/agent/stream").is_err());
        assert!(normalize_allowed_path("api/agent/conversations").is_err());
        assert!(normalize_allowed_path("api/training/../agent/chat").is_err());
        assert!(normalize_allowed_path("api/training/%2e%2e/agent/chat").is_err());
        assert!(normalize_allowed_path("https://example.test/api/training/start").is_err());
    }

    #[test]
    fn only_allows_expected_methods_for_cloud_task_paths() {
        assert!(ensure_allowed_method_for_path("api/overview", &Method::GET).is_ok());
        assert!(ensure_allowed_method_for_path("api/search", &Method::POST).is_ok());
        assert!(ensure_allowed_method_for_path("api/search", &Method::GET).is_err());
        assert!(ensure_allowed_method_for_path("api/optimize", &Method::POST).is_ok());
        assert!(ensure_allowed_method_for_path("api/optimize", &Method::DELETE).is_err());
        assert!(ensure_allowed_method_for_path("api/literature/folders", &Method::GET).is_ok());
        assert!(ensure_allowed_method_for_path("api/literature/folders", &Method::POST).is_err());
        assert!(
            ensure_allowed_method_for_path("api/literature/folders?folder=a", &Method::DELETE)
                .is_ok()
        );
    }

    #[test]
    fn only_allows_export_download_path() {
        assert!(normalize_allowed_download_path("/api/export?query=Q355B").is_ok());
        assert!(normalize_allowed_download_path("api/export").is_ok());
        assert!(normalize_allowed_download_path(
            "/api/literature/files/raw?folder=a&filename=b.pdf"
        )
        .is_ok());
        assert!(normalize_allowed_download_path(
            "/api/literature/files/image?folder=a&filename=b.pdf&image=c.jpg"
        )
        .is_ok());
        assert!(normalize_allowed_download_path("api/search").is_err());
        assert!(normalize_allowed_download_path("api/export/../ask").is_err());
        assert!(normalize_allowed_download_path("https://example.test/api/export").is_err());
    }

    #[test]
    fn only_allows_literature_upload_binary_path() {
        assert!(
            normalize_allowed_binary_path("/api/literature/upload?folder=a&filename=b.pdf").is_ok()
        );
        assert!(
            normalize_allowed_binary_path("/api/literature/files/raw?folder=a&filename=b.pdf")
                .is_err()
        );
        assert!(
            normalize_allowed_binary_path("https://example.test/api/literature/upload").is_err()
        );
        assert!(normalize_allowed_binary_path("/api/literature/upload?folder=../secret").is_err());
    }

    #[test]
    fn extracts_job_id_and_status() {
        let value = serde_json::json!({ "job_id": "job-1", "status": "running" });
        assert_eq!(extract_job_id(&value).as_deref(), Some("job-1"));
        assert_eq!(extract_status(&value).as_deref(), Some("running"));
    }

    #[test]
    fn sync_only_targets_remote_non_terminal_jobs() {
        let mut job = CloudJob {
            id: "local-1".to_string(),
            conversation_id: None,
            cloud_job_id: "job-1".to_string(),
            r#type: "training".to_string(),
            status: "running".to_string(),
            payload_json: "{}".to_string(),
            result_json: None,
            created_at: "t1".to_string(),
            updated_at: "t1".to_string(),
        };

        assert!(should_sync_job(&job));
        job.status = "completed".to_string();
        assert!(!should_sync_job(&job));
        job.status = "needs_input".to_string();
        assert!(!should_sync_job(&job));
        job.status = "running".to_string();
        job.cloud_job_id = "pending-local".to_string();
        assert!(!should_sync_job(&job));
    }
}
