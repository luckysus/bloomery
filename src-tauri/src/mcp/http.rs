use std::{collections::BTreeMap, sync::Arc};

use http::header::{HeaderName, HeaderValue};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};

use super::{McpClient, McpClientConfig, McpError, McpSseConfig};

const MAX_SSE_EVENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct McpHttpConfig {
    pub url: String,
    pub bearer_token: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub sse: McpSseConfig,
    pub max_sse_event_bytes: usize,
    pub allow_stateless: bool,
    pub reinit_on_expired_session: bool,
}

impl McpHttpConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            bearer_token: None,
            headers: BTreeMap::new(),
            sse: McpSseConfig::default(),
            max_sse_event_bytes: MAX_SSE_EVENT_BYTES,
            allow_stateless: true,
            reinit_on_expired_session: true,
        }
    }

    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn with_sse(mut self, sse: McpSseConfig) -> Self {
        self.sse = sse;
        self
    }

    pub fn with_max_sse_event_bytes(mut self, max_bytes: usize) -> Self {
        self.max_sse_event_bytes = max_bytes;
        self
    }

    pub fn validate(&self) -> Result<(), McpError> {
        let url = reqwest::Url::parse(&self.url)
            .map_err(|error| McpError::InvalidTransport(format!("invalid MCP URL: {error}")))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(McpError::InvalidTransport(
                "MCP URL must use http or https and include a host".to_string(),
            ));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(McpError::InvalidTransport(
                "MCP URL must not contain credentials".to_string(),
            ));
        }
        if self.max_sse_event_bytes == 0 || self.max_sse_event_bytes > MAX_SSE_EVENT_BYTES {
            return Err(McpError::InvalidTransport(format!(
                "MCP SSE event limit must be between 1 and {MAX_SSE_EVENT_BYTES} bytes"
            )));
        }
        if self.bearer_token.as_deref().is_some_and(|token| {
            token.trim().is_empty() || token.to_ascii_lowercase().starts_with("bearer ")
        }) {
            return Err(McpError::InvalidTransport(
                "MCP bearer token must be a non-empty raw token".to_string(),
            ));
        }
        self.sse.validate()?;
        for (name, value) in &self.headers {
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                McpError::InvalidTransport(format!("invalid MCP header name: {name}"))
            })?;
            HeaderValue::from_str(value).map_err(|_| {
                McpError::InvalidTransport(format!("invalid MCP header value for {name}"))
            })?;
        }
        Ok(())
    }

    pub(crate) async fn connect(
        self,
        client_config: McpClientConfig,
    ) -> Result<McpClient, McpError> {
        self.validate()?;
        let mut headers = std::collections::HashMap::new();
        for (name, value) in self.headers {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).expect("validated MCP header name"),
                HeaderValue::from_str(&value).expect("validated MCP header value"),
            );
        }
        let mut config = StreamableHttpClientTransportConfig::with_uri(Arc::<str>::from(self.url))
            .custom_headers(headers)
            .max_sse_event_size(self.max_sse_event_bytes)
            .reinit_on_expired_session(self.reinit_on_expired_session);
        config.allow_stateless = self.allow_stateless;
        config.retry_config = self.sse.retry_policy();
        if let Some(token) = self.bearer_token {
            config = config.auth_header(token);
        }
        let client =
            crate::providers::http::build_mcp_client().map_err(McpError::InvalidTransport)?;
        let transport = StreamableHttpClientTransport::with_client(client, config);
        McpClient::connect(transport, client_config).await
    }
}
