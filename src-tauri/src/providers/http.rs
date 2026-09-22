use crate::diagnostics::redaction::Redactor;
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::{redirect, Client, StatusCode};
use serde::Serialize;
use serde_json::{Map, Value};
use std::time::Duration;
use std::{fmt, sync::Once};

const USER_AGENT: &str = concat!("Bloomery/", env!("CARGO_PKG_VERSION"), " (desktop)");
static RUSTLS_PROVIDER_INIT: Once = Once::new();

#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub proxy_url: Option<String>,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(120),
            proxy_url: None,
        }
    }
}

pub fn build_client(config: &HttpClientConfig) -> Result<Client, ProviderError> {
    build_client_with_redirects(config, true)
}

pub fn build_no_redirect_client(config: &HttpClientConfig) -> Result<Client, ProviderError> {
    build_client_with_redirects(config, false)
}

pub fn build_mcp_client() -> Result<reqwest_013::Client, String> {
    RUSTLS_PROVIDER_INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest_013::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(0)
        .redirect(reqwest_013::redirect::Policy::none())
        .build()
        .map_err(|error| format!("build MCP HTTP client failed: {error}"))
}

fn build_client_with_redirects(
    config: &HttpClientConfig,
    follow_redirects: bool,
) -> Result<Client, ProviderError> {
    let builder = Client::builder()
        .connect_timeout(config.connect_timeout)
        .timeout(config.request_timeout)
        .user_agent(USER_AGENT);
    let builder = if follow_redirects {
        builder.redirect(redirect::Policy::custom(|attempt| {
            let previous = attempt.previous();
            if previous.len() >= 5
                || is_https_downgrade(previous.last(), attempt.url())
                || is_cross_origin_redirect(previous.last(), attempt.url())
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
    } else {
        builder.redirect(redirect::Policy::none())
    };

    let builder = if let Some(proxy_url) = config
        .proxy_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let proxy = reqwest::Proxy::all(proxy_url).map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::Network,
                None,
                "invalid HTTP proxy configuration",
            )
        })?;
        builder.proxy(proxy)
    } else {
        builder.no_proxy()
    };

    builder.build().map_err(|_| {
        ProviderError::new(
            ProviderErrorCode::Network,
            None,
            "HTTP client configuration failed",
        )
    })
}

fn is_https_downgrade(previous: Option<&reqwest::Url>, next: &reqwest::Url) -> bool {
    previous.is_some_and(|url| url.scheme() == "https") && next.scheme() == "http"
}

fn is_cross_origin_redirect(previous: Option<&reqwest::Url>, next: &reqwest::Url) -> bool {
    previous.is_some_and(|url| {
        url.scheme() != next.scheme()
            || url.host_str() != next.host_str()
            || url.port_or_known_default() != next.port_or_known_default()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorCode {
    Network,
    Authentication,
    Quota,
    Timeout,
    ProviderResponse,
    ContextLimit,
    Cancelled,
    UnsupportedCapability,
}

impl ProviderErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Authentication => "authentication",
            Self::Quota => "quota",
            Self::Timeout => "timeout",
            Self::ProviderResponse => "provider_response",
            Self::ContextLimit => "context_limit",
            Self::Cancelled => "cancelled",
            Self::UnsupportedCapability => "unsupported_capability",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderError {
    code: ProviderErrorCode,
    status: Option<u16>,
    message: String,
    retry_after_seconds: Option<u64>,
    request_id: Option<String>,
}

impl ProviderError {
    pub fn new(code: ProviderErrorCode, status: Option<u16>, message: impl Into<String>) -> Self {
        Self {
            code,
            status,
            message: message.into(),
            retry_after_seconds: None,
            request_id: None,
        }
    }

    pub fn from_status(status: StatusCode, body: &str, redactor: &Redactor) -> Self {
        Self::from_status_with_headers(status, &HeaderMap::new(), body, redactor)
    }

    pub fn from_status_with_headers(
        status: StatusCode,
        headers: &HeaderMap,
        body: &str,
        redactor: &Redactor,
    ) -> Self {
        let code = if is_context_limit(status, body) {
            ProviderErrorCode::ContextLimit
        } else {
            match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    ProviderErrorCode::Authentication
                }
                StatusCode::TOO_MANY_REQUESTS => ProviderErrorCode::Quota,
                _ => ProviderErrorCode::ProviderResponse,
            }
        };
        let message = redactor.redact_body(body);
        let message = if message.trim().is_empty() {
            status
                .canonical_reason()
                .unwrap_or("provider request failed")
                .to_string()
        } else {
            message.chars().take(4096).collect()
        };
        Self {
            code,
            status: Some(status.as_u16()),
            message,
            retry_after_seconds: headers
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_retry_after_seconds),
            request_id: request_id(headers),
        }
    }

    pub fn from_reqwest(error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::new(
                ProviderErrorCode::Timeout,
                None,
                "provider request timed out",
            )
        } else {
            Self::new(
                ProviderErrorCode::Network,
                None,
                "provider network request failed",
            )
        }
    }

    pub fn cancelled() -> Self {
        Self::new(
            ProviderErrorCode::Cancelled,
            None,
            "provider request cancelled",
        )
    }

    pub fn code(&self) -> ProviderErrorCode {
        self.code
    }

    pub fn status(&self) -> Option<u16> {
        self.status
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub fn retry_after_seconds(&self) -> Option<u64> {
        self.retry_after_seconds
    }

    pub fn details(&self) -> Option<Value> {
        let mut details = Map::new();
        if let Some(status) = self.status {
            details.insert("status".to_string(), Value::from(status));
        }
        if let Some(request_id) = &self.request_id {
            details.insert("request_id".to_string(), Value::from(request_id.clone()));
        }
        if let Some(seconds) = self.retry_after_seconds {
            details.insert("retry_after_seconds".to_string(), Value::from(seconds));
        }
        (!details.is_empty()).then_some(Value::Object(details))
    }
}

fn is_context_limit(status: StatusCode, body: &str) -> bool {
    if status != StatusCode::BAD_REQUEST {
        return false;
    }
    let body = body.to_ascii_lowercase();
    [
        "context_length_exceeded",
        "context length",
        "maximum context",
        "prompt is too long",
        "input is too long",
        "too many tokens",
        "token limit",
    ]
    .iter()
    .any(|needle| body.contains(needle))
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    ["x-deepseek-request-id", "x-request-id"]
        .iter()
        .find_map(|name| {
            headers
                .get(*name)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.chars().take(256).collect())
        })
}

fn parse_retry_after_seconds(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds);
    }
    let deadline = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    deadline
        .signed_duration_since(Utc::now())
        .num_seconds()
        .try_into()
        .ok()
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for ProviderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_https_to_http_downgrade() {
        let https = reqwest::Url::parse("https://provider.example/start").unwrap();
        let http = reqwest::Url::parse("http://provider.example/next").unwrap();
        let https_next = reqwest::Url::parse("https://provider.example/next").unwrap();

        assert!(is_https_downgrade(Some(&https), &http));
        assert!(!is_https_downgrade(Some(&https), &https_next));
    }

    #[test]
    fn identifies_cross_origin_redirects() {
        let origin = reqwest::Url::parse("https://provider.example/start").unwrap();
        let same_origin = reqwest::Url::parse("https://provider.example/next").unwrap();
        let different_host = reqwest::Url::parse("https://other.example/next").unwrap();
        let different_port = reqwest::Url::parse("https://provider.example:8443/next").unwrap();

        assert!(!is_cross_origin_redirect(Some(&origin), &same_origin));
        assert!(is_cross_origin_redirect(Some(&origin), &different_host));
        assert!(is_cross_origin_redirect(Some(&origin), &different_port));
    }

    #[test]
    fn classifies_context_limit_and_preserves_deepseek_request_diagnostics() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-deepseek-request-id", "ds-123".parse().unwrap());
        headers.insert(reqwest::header::RETRY_AFTER, "7".parse().unwrap());
        let error = ProviderError::from_status_with_headers(
            StatusCode::BAD_REQUEST,
            &headers,
            r#"{"error":{"code":"context_length_exceeded","message":"prompt is too long"}}"#,
            &Redactor::new(),
        );

        assert_eq!(error.code(), ProviderErrorCode::ContextLimit);
        assert_eq!(error.request_id(), Some("ds-123"));
        assert_eq!(error.retry_after_seconds(), Some(7));
        assert_eq!(
            error.details().unwrap(),
            serde_json::json!({
                "status": 400,
                "request_id": "ds-123",
                "retry_after_seconds": 7,
            })
        );
    }

    #[test]
    fn builds_mcp_client_without_panicking_when_rustls_has_no_default_provider() {
        let result = std::panic::catch_unwind(build_mcp_client);
        assert!(result.is_ok(), "MCP client construction must not panic");
        assert!(result.unwrap().is_ok());
    }
}
