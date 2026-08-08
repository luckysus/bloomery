use crate::diagnostics::redaction::Redactor;
use crate::providers::capabilities::{
    DocumentParseRequest, DocumentParserProvider, DocumentTaskState, DocumentTaskStatus,
    ParsedDocumentArtifact, ProviderCapabilities, RemoteTaskId,
};
use crate::providers::http::{
    build_client, build_no_redirect_client, HttpClientConfig, ProviderError, ProviderErrorCode,
};
use crate::providers::profiles::{
    validate_bearer_transport, ProviderCapability, ProviderKind, ProviderProfile,
};
use crate::storage::secrets::SecretValue;
use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, RETRY_AFTER};
use reqwest::{Client, Response, StatusCode};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use std::path::{Component, Path};
use std::time::Duration;
use tokio::time::sleep;
use zip::ZipArchive;

const DEFAULT_MODEL: &str = "vlm";
const MAX_SOURCE_BYTES: usize = 200 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 1024 * 1024;
const MAX_ARCHIVE_BYTES: usize = 512 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_ATTEMPTS: usize = 3;
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

pub struct MinerUProvider {
    profile: ProviderProfile,
    credential: SecretValue,
    client: Client,
    signed_client: Client,
    batch_url: String,
    result_url: String,
    capabilities: ProviderCapabilities,
}

impl MinerUProvider {
    pub fn new(
        profile: ProviderProfile,
        credential: Option<SecretValue>,
    ) -> Result<Self, ProviderError> {
        let profile = profile.validate().map_err(provider_response)?;
        if profile.kind != ProviderKind::MinerU {
            return Err(provider_response(
                "MinerU provider requires a MinerU profile",
            ));
        }
        let credential = credential.ok_or_else(|| {
            ProviderError::new(
                ProviderErrorCode::Authentication,
                None,
                "MinerU credential is required",
            )
        })?;
        validate_bearer_transport(&profile.base_url, true).map_err(provider_response)?;
        let api_root = api_root(&profile.base_url);
        let model_id = profile
            .model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_MODEL)
            .to_string();
        let client_config = HttpClientConfig {
            request_timeout: Duration::from_secs(300),
            ..HttpClientConfig::default()
        };
        let client = build_client(&client_config)?;
        let signed_client = build_no_redirect_client(&client_config)?;
        Ok(Self {
            profile,
            credential,
            client,
            signed_client,
            batch_url: format!("{api_root}/file-urls/batch"),
            result_url: format!("{api_root}/extract-results/batch"),
            capabilities: ProviderCapabilities {
                provider_kind: ProviderKind::MinerU,
                model_id,
                capabilities: vec![ProviderCapability::DocumentParser],
                context_window: None,
                streaming: false,
                tool_calls: false,
                json_schema: false,
                max_batch_size: Some(1),
            },
        })
    }

    pub fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    async fn fetch_task(&self, id: &RemoteTaskId) -> Result<TaskDetails, ProviderError> {
        validate_task_id(id)?;
        let url = format!("{}/{}", self.result_url, id.0);
        let value = self.get_json(&url).await?;
        let data = value["data"]
            .as_object()
            .ok_or_else(|| provider_response("MinerU response is missing task data"))?;
        if let Some(batch_id) = data.get("batch_id").and_then(Value::as_str) {
            if batch_id != id.0 {
                return Err(provider_response("MinerU returned a mismatched task ID"));
            }
        }
        let results = data
            .get("extract_result")
            .or_else(|| data.get("results"))
            .and_then(Value::as_array)
            .filter(|results| !results.is_empty())
            .ok_or_else(|| provider_response("MinerU response is missing task results"))?;

        let mut extracted_pages = 0u64;
        let mut total_pages = 0u64;
        let mut all_done = true;
        let mut failed = false;
        let mut cancelled = false;
        let mut artifact_url = None;
        let mut checksum = None;
        for result in results {
            let state = result["state"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            all_done &= state == "done";
            failed |= state == "failed";
            cancelled |= state == "cancelled";
            if artifact_url.is_none() {
                artifact_url = result["full_zip_url"]
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
            }
            for name in ["x-checksum-sha256", "sha256", "checksum"] {
                if let Some(value) = result.get(name) {
                    let value = value
                        .as_str()
                        .ok_or_else(|| provider_response("invalid MinerU artifact checksum"))?;
                    merge_checksum(&mut checksum, value, "MinerU task checksums disagree")?;
                }
            }
            if let Some(progress) = result["extract_progress"].as_object() {
                extracted_pages = extracted_pages.saturating_add(
                    progress
                        .get("extracted_pages")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                );
                total_pages = total_pages.saturating_add(
                    progress
                        .get("total_pages")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                );
            }
        }
        let state = if failed {
            DocumentTaskState::Failed
        } else if cancelled {
            DocumentTaskState::Cancelled
        } else if all_done {
            DocumentTaskState::Completed
        } else {
            DocumentTaskState::Running
        };
        let progress_percent = match state {
            DocumentTaskState::Completed => Some(100),
            _ if total_pages > 0 => Some(
                extracted_pages
                    .saturating_mul(100)
                    .checked_div(total_pages)
                    .unwrap_or(0)
                    .min(100) as u8,
            ),
            _ => None,
        };
        Ok(TaskDetails {
            status: DocumentTaskStatus {
                id: id.clone(),
                state,
                progress_percent,
            },
            artifact_url,
            checksum,
        })
    }

    async fn get_json(&self, url: &str) -> Result<Value, ProviderError> {
        let redactor = Redactor::new().with_secret(&self.credential);
        crate::diagnostics::observability::register_secret(&self.credential);
        for attempt in 1..=MAX_ATTEMPTS {
            let response = self
                .client
                .get(url)
                .bearer_auth(self.credential.expose())
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(_) if attempt < MAX_ATTEMPTS => {
                    sleep(retry_delay(attempt)).await;
                    continue;
                }
                Err(error) => return Err(ProviderError::from_reqwest(&error)),
            };
            if let Some(delay) = response_retry_delay(&response, attempt) {
                sleep(delay).await;
                continue;
            }
            let successful = response.status().is_success();
            let result = parse_json_response(response, &redactor).await;
            if successful
                && attempt < MAX_ATTEMPTS
                && result.as_ref().is_err_and(retryable_network_error)
            {
                sleep(retry_delay(attempt)).await;
                continue;
            }
            return result;
        }
        Err(provider_response("MinerU request failed"))
    }

    async fn post_json<T: Serialize + ?Sized>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<Value, ProviderError> {
        let redactor = Redactor::new().with_secret(&self.credential);
        crate::diagnostics::observability::register_secret(&self.credential);
        let response = self
            .client
            .post(url)
            .bearer_auth(self.credential.expose())
            .json(body)
            .send()
            .await
            .map_err(|error| ProviderError::from_reqwest(&error))?;
        parse_json_response(response, &redactor).await
    }

    pub(crate) async fn upload_batch(&self, url: &str, bytes: &[u8]) -> Result<(), ProviderError> {
        for attempt in 1..=MAX_ATTEMPTS {
            let response = self
                .signed_client
                .put(url)
                .body(bytes.to_vec())
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(_) if attempt < MAX_ATTEMPTS => {
                    sleep(retry_delay(attempt)).await;
                    continue;
                }
                Err(error) => return Err(ProviderError::from_reqwest(&error)),
            };
            if response.status().is_success() {
                return Ok(());
            }
            if let Some(delay) = response_retry_delay(&response, attempt) {
                sleep(delay).await;
                continue;
            }
            return Err(ProviderError::from_status(
                response.status(),
                "MinerU signed upload failed",
                &Redactor::new(),
            ));
        }
        Err(provider_response("MinerU upload failed"))
    }

    async fn download_archive(
        &self,
        url: &str,
        expected_checksum: Option<&str>,
    ) -> Result<Vec<u8>, ProviderError> {
        for attempt in 1..=MAX_ATTEMPTS {
            let response = self.signed_client.get(url).send().await;
            let response = match response {
                Ok(response) => response,
                Err(_) if attempt < MAX_ATTEMPTS => {
                    sleep(retry_delay(attempt)).await;
                    continue;
                }
                Err(error) => return Err(ProviderError::from_reqwest(&error)),
            };
            let status = response.status();
            if let Some(delay) = response_retry_delay(&response, attempt) {
                sleep(delay).await;
                continue;
            }
            if !status.is_success() {
                return Err(ProviderError::from_status(
                    status,
                    "MinerU artifact download failed",
                    &Redactor::new(),
                ));
            }
            validate_archive_content_type(&response)?;
            let mut response_checksum = None;
            for name in ["x-checksum-sha256", "sha256"] {
                if let Some(value) = response.headers().get(name) {
                    let value = value
                        .to_str()
                        .map_err(|_| provider_response("invalid MinerU artifact checksum"))?;
                    merge_checksum(
                        &mut response_checksum,
                        value,
                        "MinerU response checksums disagree",
                    )?;
                }
            }
            if let (Some(expected), Some(actual)) =
                (expected_checksum, response_checksum.as_deref())
            {
                if expected != actual {
                    return Err(provider_response("MinerU artifact checksums disagree"));
                }
            }
            let bytes = match read_bounded(response, MAX_ARCHIVE_BYTES).await {
                Ok(bytes) => bytes,
                Err(error) if attempt < MAX_ATTEMPTS && retryable_network_error(&error) => {
                    sleep(retry_delay(attempt)).await;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let checksum = format!("{:x}", Sha256::digest(&bytes));
            if expected_checksum
                .or(response_checksum.as_deref())
                .is_some_and(|expected| expected != checksum)
            {
                return Err(provider_response("MinerU artifact checksum mismatch"));
            }
            validate_zip(&bytes)?;
            return Ok(bytes);
        }
        Err(provider_response("MinerU artifact download failed"))
    }

    pub(crate) async fn create_batch(
        &self,
        request: &DocumentParseRequest,
    ) -> Result<(RemoteTaskId, String), ProviderError> {
        validate_parse_request(request)?;
        let data_id = data_id(&request.file_name, &request.bytes);
        let body = BatchRequest {
            files: vec![BatchFile {
                name: &request.file_name,
                data_id: &data_id,
                is_ocr: true,
            }],
            model_version: &self.capabilities.model_id,
            enable_formula: true,
            enable_table: true,
        };
        let value = self.post_json(&self.batch_url, &body).await?;
        let batch_id = value["data"]["batch_id"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| provider_response("MinerU response is missing batch ID"))?;
        let id = RemoteTaskId(batch_id.to_string());
        validate_task_id(&id)?;
        let urls = value["data"]["file_urls"]
            .as_array()
            .filter(|urls| urls.len() == 1)
            .ok_or_else(|| provider_response("MinerU returned an invalid upload URL count"))?;
        let upload_url = urls[0]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| provider_response("MinerU returned an invalid upload URL"))?;
        validate_download_url(upload_url)?;
        Ok((id, upload_url.to_string()))
    }
}

impl DocumentParserProvider for MinerUProvider {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    async fn submit(&self, request: DocumentParseRequest) -> Result<RemoteTaskId, ProviderError> {
        let (id, upload_url) = self.create_batch(&request).await?;
        self.upload_batch(&upload_url, &request.bytes).await?;
        Ok(id)
    }

    async fn poll(&self, id: &RemoteTaskId) -> Result<DocumentTaskStatus, ProviderError> {
        Ok(self.fetch_task(id).await?.status)
    }

    async fn download(&self, id: &RemoteTaskId) -> Result<ParsedDocumentArtifact, ProviderError> {
        let details = self.fetch_task(id).await?;
        if details.status.state != DocumentTaskState::Completed {
            return Err(provider_response("MinerU task is not complete"));
        }
        let url = details
            .artifact_url
            .ok_or_else(|| provider_response("MinerU task has no artifact URL"))?;
        validate_download_url(&url)?;
        let bytes = self
            .download_archive(&url, details.checksum.as_deref())
            .await?;
        Ok(ParsedDocumentArtifact {
            file_name: format!("{}.zip", id.0),
            mime_type: "application/zip".to_string(),
            bytes,
        })
    }

    async fn cancel(&self, id: &RemoteTaskId) -> Result<(), ProviderError> {
        validate_task_id(id)?;
        Err(ProviderError::new(
            ProviderErrorCode::UnsupportedCapability,
            None,
            "MinerU v4 does not expose a remote cancellation endpoint",
        ))
    }
}

#[derive(Serialize)]
struct BatchRequest<'a> {
    files: Vec<BatchFile<'a>>,
    model_version: &'a str,
    enable_formula: bool,
    enable_table: bool,
}

#[derive(Serialize)]
struct BatchFile<'a> {
    name: &'a str,
    data_id: &'a str,
    is_ocr: bool,
}

struct TaskDetails {
    status: DocumentTaskStatus,
    artifact_url: Option<String>,
    checksum: Option<String>,
}

fn api_root(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/api/v4") {
        base.to_string()
    } else {
        format!("{base}/api/v4")
    }
}

fn validate_parse_request(request: &DocumentParseRequest) -> Result<(), ProviderError> {
    let path = Path::new(request.file_name.trim());
    if request.file_name.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(provider_response("invalid MinerU source file name"));
    }
    if request.mime_type.trim().is_empty() {
        return Err(provider_response("MinerU source MIME type is required"));
    }
    if request.bytes.is_empty() || request.bytes.len() > MAX_SOURCE_BYTES {
        return Err(provider_response("MinerU source file size is invalid"));
    }
    Ok(())
}

fn data_id(file_name: &str, bytes: &[u8]) -> String {
    if file_name.len() <= 128
        && file_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        file_name.to_string()
    } else {
        let digest = format!("{:x}", Sha256::digest(bytes));
        format!("bloomery-{}", &digest[..32])
    }
}

fn validate_task_id(id: &RemoteTaskId) -> Result<(), ProviderError> {
    if id.0.is_empty()
        || id.0.len() > 256
        || !id
            .0
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(provider_response("invalid MinerU task ID"));
    }
    Ok(())
}

fn validate_download_url(value: &str) -> Result<(), ProviderError> {
    let url = reqwest::Url::parse(value).map_err(|_| provider_response("invalid MinerU URL"))?;
    if url.scheme() == "https"
        || (url.scheme() == "http"
            && url.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            }))
    {
        Ok(())
    } else {
        Err(provider_response("MinerU URL must use HTTPS"))
    }
}

fn validate_archive_content_type(response: &Response) -> Result<(), ProviderError> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        content_type.as_str(),
        "application/zip" | "application/x-zip-compressed" | "application/octet-stream"
    ) {
        return Err(provider_response("MinerU artifact is not a ZIP response"));
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > MAX_ARCHIVE_BYTES)
    {
        return Err(provider_response("MinerU artifact exceeded size limit"));
    }
    Ok(())
}

fn validate_zip(bytes: &[u8]) -> Result<(), ProviderError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| provider_response("MinerU artifact is not a valid ZIP archive"))?;
    if archive.is_empty() || archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(provider_response("MinerU artifact entry count is invalid"));
    }
    let mut expanded = 0u64;
    let mut buffer = [0u8; 16 * 1024];
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| provider_response("MinerU artifact contains an invalid ZIP entry"))?;
        if entry.enclosed_name().is_none()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(provider_response("MinerU artifact contains an unsafe path"));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_EXPANDED_BYTES {
            return Err(provider_response(
                "MinerU artifact expanded size exceeded limit",
            ));
        }
        loop {
            let read = entry
                .read(&mut buffer)
                .map_err(|_| provider_response("MinerU artifact ZIP integrity check failed"))?;
            if read == 0 {
                break;
            }
        }
    }
    Ok(())
}

fn normalize_checksum(value: &str) -> Result<String, ProviderError> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value)
    } else {
        Err(provider_response("invalid MinerU artifact checksum"))
    }
}

fn merge_checksum(
    checksum: &mut Option<String>,
    value: &str,
    mismatch_message: &str,
) -> Result<(), ProviderError> {
    let value = normalize_checksum(value)?;
    if checksum.as_ref().is_some_and(|existing| existing != &value) {
        return Err(provider_response(mismatch_message));
    }
    checksum.get_or_insert(value);
    Ok(())
}

async fn parse_json_response(
    response: Response,
    redactor: &Redactor,
) -> Result<Value, ProviderError> {
    let status = response.status();
    let bytes = match read_bounded(response, MAX_JSON_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) if !status.is_success() => {
            return Err(ProviderError::from_status(status, "", redactor));
        }
        Err(error) => return Err(error),
    };
    let text = String::from_utf8_lossy(&bytes);
    if !status.is_success() {
        return Err(ProviderError::from_status(status, &text, redactor));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| provider_response("MinerU returned invalid JSON"))?;
    if value["code"].as_i64().unwrap_or(-1) != 0 {
        let message = value["msg"]
            .as_str()
            .or_else(|| value["message"].as_str())
            .unwrap_or("MinerU request failed");
        let message = redactor.redact_body(message);
        let code = if message.to_ascii_lowercase().contains("quota") {
            ProviderErrorCode::Quota
        } else {
            ProviderErrorCode::ProviderResponse
        };
        return Err(ProviderError::new(code, None, message));
    }
    Ok(value)
}

async fn read_bounded(response: Response, limit: usize) -> Result<Vec<u8>, ProviderError> {
    if response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .is_some_and(|length| length > limit)
    {
        return Err(provider_response("MinerU response exceeded size limit"));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| ProviderError::from_reqwest(&error))?;
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(provider_response("MinerU response exceeded size limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn response_retry_delay(response: &Response, attempt: usize) -> Option<Duration> {
    if attempt >= MAX_ATTEMPTS || !retryable_status(response.status()) {
        return None;
    }
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_retry_after(value, chrono::Utc::now()));
    match retry_after {
        Some(delay) if delay <= MAX_RETRY_AFTER => Some(delay),
        Some(_) => None,
        None => Some(retry_delay(attempt)),
    }
}

fn parse_retry_after(value: &str, now: chrono::DateTime<chrono::Utc>) -> Option<Duration> {
    let value = value.trim();
    value
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
        .or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|deadline| {
                    deadline
                        .with_timezone(&chrono::Utc)
                        .signed_duration_since(now)
                        .to_std()
                        .unwrap_or(Duration::ZERO)
                })
        })
}

fn retryable_network_error(error: &ProviderError) -> bool {
    matches!(
        error.code(),
        ProviderErrorCode::Network | ProviderErrorCode::Timeout
    )
}

fn retry_delay(attempt: usize) -> Duration {
    Duration::from_millis(100 * (1u64 << attempt.saturating_sub(1)))
}

fn provider_response(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ProviderErrorCode::ProviderResponse, None, message)
}
