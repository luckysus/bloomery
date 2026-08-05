use crate::diagnostics::redaction::Redactor;
use reqwest::{redirect, Client, StatusCode};
use serde::Serialize;
use std::fmt;
use std::time::Duration;

const USER_AGENT: &str = concat!("Bloomery/", env!("CARGO_PKG_VERSION"), " (desktop)");

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

fn build_client_with_redirects(
    config: &HttpClientConfig,
    follow_redirects: bool,
) -> Result<Client, ProviderError> {
    let builder = Client::builder()
        .connect_timeout(config.connect_timeout)
        .timeout(config.request_timeout)
        .user_agent(USER_AGENT);
    let mut builder = if follow_redirects {
        builder.redirect(redirect::Policy::custom(|attempt| {
            let previous = attempt.previous();
            if previous.len() >= 5 || is_https_downgrade(previous.last(), attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
    } else {
        builder.redirect(redirect::Policy::none())
    };

    if let Some(proxy_url) = config
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
        builder = builder.proxy(proxy);
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorCode {
    Network,
    Authentication,
    Quota,
    Timeout,
    ProviderResponse,
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
}

impl ProviderError {
    pub fn new(code: ProviderErrorCode, status: Option<u16>, message: impl Into<String>) -> Self {
        Self {
            code,
            status,
            message: message.into(),
        }
    }

    pub fn from_status(status: StatusCode, body: &str, redactor: &Redactor) -> Self {
        let code = match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderErrorCode::Authentication,
            StatusCode::TOO_MANY_REQUESTS => ProviderErrorCode::Quota,
            _ => ProviderErrorCode::ProviderResponse,
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
        Self::new(code, Some(status.as_u16()), message)
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
}
