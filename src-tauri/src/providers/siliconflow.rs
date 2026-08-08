use crate::diagnostics::redaction::Redactor;
use crate::providers::capabilities::{
    EmbeddingProvider, EmbeddingResponse, ProviderCapabilities, RerankDocument, RerankProvider,
    RerankResult,
};
use crate::providers::http::{build_client, HttpClientConfig, ProviderError, ProviderErrorCode};
use crate::providers::profiles::{
    validate_bearer_transport, ProviderCapability, ProviderKind, ProviderProfile,
};
use crate::storage::secrets::SecretValue;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

pub const DEFAULT_EMBEDDING_MODEL: &str = "BAAI/bge-m3";
pub const DEFAULT_RERANK_MODEL: &str = "BAAI/bge-reranker-v2-m3";
const MAX_BATCH_SIZE: usize = 64;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 3;
const MAX_RETRY_AFTER: Duration = Duration::from_secs(5);
const MAX_RETRY_JITTER: Duration = Duration::from_millis(100);
static RETRY_JITTER_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiliconFlowPlan {
    Free,
    Pro,
}

pub struct SiliconFlowProvider {
    profile: ProviderProfile,
    credential: SecretValue,
    plan: SiliconFlowPlan,
    client: Client,
    embedding_url: String,
    rerank_url: String,
    embedding_capabilities: ProviderCapabilities,
    rerank_capabilities: ProviderCapabilities,
}

impl SiliconFlowProvider {
    pub fn with_models(
        profile: ProviderProfile,
        credential: Option<SecretValue>,
        plan: SiliconFlowPlan,
        embedding_model: Option<String>,
        rerank_model: Option<String>,
    ) -> Result<Self, ProviderError> {
        let profile = profile.validate().map_err(provider_response)?;
        if profile.kind != ProviderKind::SiliconFlow {
            return Err(provider_response(
                "SiliconFlow provider requires a SiliconFlow profile",
            ));
        }
        let credential = credential.ok_or_else(|| {
            ProviderError::new(
                ProviderErrorCode::Authentication,
                None,
                "SiliconFlow credential is required",
            )
        })?;
        validate_bearer_transport(&profile.base_url, true).map_err(provider_response)?;
        let embedding_model = normalized_model(embedding_model, DEFAULT_EMBEDDING_MODEL);
        let rerank_model = normalized_model(rerank_model, DEFAULT_RERANK_MODEL);
        let embedding_url = endpoint(&profile.base_url, "embeddings");
        let rerank_url = endpoint(&profile.base_url, "rerank");
        let client = build_client(&HttpClientConfig::default())?;
        Ok(Self {
            profile,
            credential,
            plan,
            client,
            embedding_url,
            rerank_url,
            embedding_capabilities: capabilities(ProviderCapability::Embedding, embedding_model),
            rerank_capabilities: capabilities(ProviderCapability::Rerank, rerank_model),
        })
    }

    pub fn plan(&self) -> SiliconFlowPlan {
        self.plan
    }

    pub fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    async fn post_json<T: Serialize + ?Sized>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<Value, ProviderError> {
        let redactor = Redactor::new().with_secret(&self.credential);
        crate::diagnostics::observability::register_secret(&self.credential);
        for attempt in 1..=MAX_ATTEMPTS {
            let response = match self
                .client
                .post(url)
                .bearer_auth(self.credential.expose())
                .json(body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(_) if attempt < MAX_ATTEMPTS => {
                    sleep(jittered_delay(exponential_backoff(attempt))).await;
                    continue;
                }
                Err(error) => return Err(ProviderError::from_reqwest(&error)),
            };
            let status = response.status();
            let retry_decision = retry_decision(status, response.headers(), attempt);
            let bytes = match read_bounded(response).await {
                Ok(bytes) => bytes,
                Err(_) if !status.is_success() => {
                    if let RetryDecision::Wait(delay) = retry_decision {
                        sleep(jittered_delay(delay)).await;
                        continue;
                    }
                    return Err(ProviderError::from_status(status, "", &redactor));
                }
                Err(_) if attempt < MAX_ATTEMPTS => {
                    sleep(jittered_delay(exponential_backoff(attempt))).await;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let body_text = String::from_utf8_lossy(&bytes);
            if !status.is_success() {
                if let RetryDecision::Wait(delay) = retry_decision {
                    sleep(jittered_delay(delay)).await;
                    continue;
                }
                return Err(ProviderError::from_status(status, &body_text, &redactor));
            }
            return serde_json::from_slice(&bytes)
                .map_err(|_| provider_response("SiliconFlow returned invalid JSON"));
        }
        Err(provider_response("SiliconFlow request failed"))
    }
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

impl EmbeddingProvider for SiliconFlowProvider {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.embedding_capabilities
    }

    async fn embed(&self, inputs: Vec<String>) -> Result<EmbeddingResponse, ProviderError> {
        if inputs.is_empty() {
            return Err(provider_response("embedding inputs are required"));
        }
        let mut vectors = Vec::with_capacity(inputs.len());
        let mut dimensions = None;
        for batch in inputs.chunks(MAX_BATCH_SIZE) {
            let value = self
                .post_json(
                    &self.embedding_url,
                    &EmbeddingRequest {
                        model: &self.embedding_capabilities.model_id,
                        input: batch,
                    },
                )
                .await?;
            let data = value["data"].as_array().ok_or_else(|| {
                provider_response("SiliconFlow embedding response is missing data")
            })?;
            if data.len() != batch.len() {
                return Err(provider_response(
                    "SiliconFlow embedding count does not match input count",
                ));
            }
            let mut batch_vectors = vec![None; batch.len()];
            for item in data {
                let index = parse_index(item, batch.len())?;
                let vector = parse_vector(&item["embedding"])?;
                match dimensions {
                    Some(expected) if vector.len() != expected => {
                        return Err(provider_response(
                            "SiliconFlow embedding dimension mismatch",
                        ));
                    }
                    None => dimensions = Some(vector.len()),
                    _ => {}
                }
                if batch_vectors[index].replace(vector).is_some() {
                    return Err(provider_response("duplicate SiliconFlow embedding index"));
                }
            }
            vectors.extend(
                batch_vectors
                    .into_iter()
                    .map(|vector| {
                        vector
                            .ok_or_else(|| provider_response("missing SiliconFlow embedding index"))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        Ok(EmbeddingResponse {
            model_id: self.embedding_capabilities.model_id.clone(),
            vectors,
        })
    }
}

#[derive(Serialize)]
struct RerankRequest<'a> {
    model: &'a str,
    query: &'a str,
    documents: Vec<&'a str>,
}

impl RerankProvider for SiliconFlowProvider {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.rerank_capabilities
    }

    async fn rerank(
        &self,
        query: String,
        documents: Vec<RerankDocument>,
    ) -> Result<Vec<RerankResult>, ProviderError> {
        if query.trim().is_empty() || documents.is_empty() {
            return Err(provider_response("rerank query and documents are required"));
        }
        if documents.len() > MAX_BATCH_SIZE {
            return Err(provider_response(
                "rerank document batch exceeds provider limit",
            ));
        }
        let value = self
            .post_json(
                &self.rerank_url,
                &RerankRequest {
                    model: &self.rerank_capabilities.model_id,
                    query: query.trim(),
                    documents: documents
                        .iter()
                        .map(|document| document.text.as_str())
                        .collect(),
                },
            )
            .await?;
        let results = value["results"]
            .as_array()
            .ok_or_else(|| provider_response("SiliconFlow rerank response is missing results"))?;
        if results.len() != documents.len() {
            return Err(provider_response(
                "SiliconFlow rerank result count does not match document count",
            ));
        }
        let mut seen = vec![false; documents.len()];
        let mut normalized = Vec::with_capacity(results.len());
        for result in results {
            let index = parse_index(result, documents.len())?;
            if std::mem::replace(&mut seen[index], true) {
                return Err(provider_response("duplicate SiliconFlow rerank index"));
            }
            let score = result["relevance_score"]
                .as_f64()
                .and_then(finite_f32)
                .ok_or_else(|| provider_response("invalid SiliconFlow rerank score"))?;
            normalized.push(RerankResult {
                id: documents[index].id.clone(),
                score,
            });
        }
        Ok(normalized)
    }
}

fn capabilities(capability: ProviderCapability, model_id: String) -> ProviderCapabilities {
    ProviderCapabilities {
        provider_kind: ProviderKind::SiliconFlow,
        model_id,
        capabilities: vec![capability],
        context_window: None,
        streaming: false,
        tool_calls: false,
        json_schema: false,
        max_batch_size: Some(MAX_BATCH_SIZE),
    }
}

fn normalized_model(value: Option<String>, fallback: &str) -> String {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn endpoint(base_url: &str, path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with(&format!("/{path}")) {
        base.to_string()
    } else {
        format!("{base}/{path}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryDecision {
    Wait(Duration),
    DoNotRetry,
}

fn retry_decision(
    status: StatusCode,
    headers: &reqwest::header::HeaderMap,
    attempt: usize,
) -> RetryDecision {
    if attempt >= MAX_ATTEMPTS
        || !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
    {
        return RetryDecision::DoNotRetry;
    }
    if let Some(value) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
    {
        return parse_retry_after(Some(value), chrono::Utc::now());
    }
    RetryDecision::Wait(exponential_backoff(attempt))
}

fn exponential_backoff(attempt: usize) -> Duration {
    Duration::from_millis(100 * (1u64 << attempt.saturating_sub(1)))
}

fn jittered_delay(base: Duration) -> Duration {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let sequence = RETRY_JITTER_COUNTER.fetch_add(1, Ordering::Relaxed);
    jittered_delay_with_seed(base, timestamp ^ sequence)
}

fn jittered_delay_with_seed(base: Duration, seed: u64) -> Duration {
    let jitter = Duration::from_millis(1 + seed % MAX_RETRY_JITTER.as_millis() as u64);
    base.saturating_add(jitter)
}

fn parse_retry_after(value: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> RetryDecision {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    let delay = value
        .and_then(|value| value.parse::<u64>().ok().map(Duration::from_secs))
        .or_else(|| {
            value
                .and_then(|value| chrono::DateTime::parse_from_rfc2822(value).ok())
                .map(|deadline| {
                    let deadline = deadline.with_timezone(&chrono::Utc);
                    deadline
                        .signed_duration_since(now)
                        .to_std()
                        .unwrap_or(Duration::ZERO)
                })
        })
        .unwrap_or(Duration::from_secs(1));
    if delay <= MAX_RETRY_AFTER {
        RetryDecision::Wait(delay)
    } else {
        RetryDecision::DoNotRetry
    }
}

async fn read_bounded(response: reqwest::Response) -> Result<Vec<u8>, ProviderError> {
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| ProviderError::from_reqwest(&error))?;
        append_bounded(&mut bytes, &chunk, MAX_RESPONSE_BYTES)?;
    }
    Ok(bytes)
}

fn append_bounded(buffer: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<(), ProviderError> {
    if bytes.len() > limit.saturating_sub(buffer.len()) {
        return Err(provider_response(
            "SiliconFlow response exceeded size limit",
        ));
    }
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn parse_index(value: &Value, length: usize) -> Result<usize, ProviderError> {
    let index = value["index"]
        .as_u64()
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < length)
        .ok_or_else(|| provider_response("invalid SiliconFlow result index"))?;
    Ok(index)
}

fn parse_vector(value: &Value) -> Result<Vec<f32>, ProviderError> {
    let values = value
        .as_array()
        .filter(|values| !values.is_empty())
        .ok_or_else(|| provider_response("invalid SiliconFlow embedding vector"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_f64()
                .and_then(finite_f32)
                .ok_or_else(|| provider_response("invalid SiliconFlow embedding value"))
        })
        .collect()
}

fn finite_f32(value: f64) -> Option<f32> {
    let converted = value as f32;
    (converted.is_finite() && (value == 0.0 || converted != 0.0)).then_some(converted)
}

fn provider_response(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ProviderErrorCode::ProviderResponse, None, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_response_buffer_rejects_overflow_before_append() {
        let mut buffer = vec![1, 2];

        append_bounded(&mut buffer, &[3, 4], 4).unwrap();
        let error = append_bounded(&mut buffer, &[5], 4).unwrap_err();

        assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
        assert_eq!(buffer, vec![1, 2, 3, 4]);
    }

    #[test]
    fn retry_after_supports_seconds_http_date_and_long_wait_rejection() {
        let now = chrono::DateTime::parse_from_rfc2822("Wed, 21 Oct 2015 07:28:00 GMT")
            .unwrap()
            .with_timezone(&chrono::Utc);

        assert_eq!(
            parse_retry_after(Some("3"), now),
            RetryDecision::Wait(Duration::from_secs(3))
        );
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2015 07:28:04 GMT"), now),
            RetryDecision::Wait(Duration::from_secs(4))
        );
        assert_eq!(
            parse_retry_after(Some("60"), now),
            RetryDecision::DoNotRetry
        );
    }

    #[test]
    fn retry_jitter_is_positive_for_zero_wait_and_stays_bounded() {
        assert_eq!(
            jittered_delay_with_seed(Duration::ZERO, 0),
            Duration::from_millis(1)
        );
        assert_eq!(
            jittered_delay_with_seed(Duration::from_secs(5), 100),
            Duration::from_millis(5_001)
        );
        assert!(jittered_delay_with_seed(Duration::from_millis(400), u64::MAX) <= MAX_RETRY_AFTER);
    }
}
