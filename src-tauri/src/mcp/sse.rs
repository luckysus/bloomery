use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use futures_util::StreamExt;
use reqwest_013::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE,
};
use rmcp::transport::common::client_side_sse::SseRetryPolicy;
use rmcp::{model::ServerJsonRpcMessage, RoleClient};
use tokio::{
    sync::{mpsc, watch, Mutex, Notify},
    task::JoinHandle,
};

use super::{McpClient, McpClientConfig, McpError};

const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);
const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENDPOINT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSseConfig {
    pub max_retries: Option<usize>,
    pub base_delay: Duration,
}

impl McpSseConfig {
    pub fn new(max_retries: Option<usize>, base_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
        }
    }

    pub fn delay_for_attempt(&self, attempt: usize) -> Option<Duration> {
        if self.max_retries.is_some_and(|max| attempt >= max) {
            return None;
        }
        if attempt >= u32::BITS as usize {
            return None;
        }
        self.base_delay
            .checked_mul(1u32 << attempt)
            .filter(|delay| *delay <= MAX_RETRY_DELAY)
    }

    pub(crate) fn validate(&self) -> Result<(), McpError> {
        if self.base_delay.is_zero() || self.base_delay > MAX_RETRY_DELAY {
            return Err(McpError::InvalidConfiguration(
                "MCP SSE base delay must be between 1ms and 60s".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn retry_policy(&self) -> Arc<dyn SseRetryPolicy> {
        Arc::new(BoundedBackoff {
            config: self.clone(),
        })
    }
}

impl Default for McpSseConfig {
    fn default() -> Self {
        Self {
            max_retries: Some(3),
            base_delay: Duration::from_millis(250),
        }
    }
}

#[derive(Debug)]
struct BoundedBackoff {
    config: McpSseConfig,
}

impl SseRetryPolicy for BoundedBackoff {
    fn retry(&self, current_times: usize) -> Option<Duration> {
        self.config.delay_for_attempt(current_times)
    }
}

#[derive(Debug, Clone)]
pub struct McpLegacySseConfig {
    pub url: String,
    pub bearer_token: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub retry: McpSseConfig,
    pub max_event_bytes: usize,
}

impl McpLegacySseConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            bearer_token: None,
            headers: BTreeMap::new(),
            retry: McpSseConfig::default(),
            max_event_bytes: MAX_EVENT_BYTES,
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

    pub fn with_sse(mut self, retry: McpSseConfig) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_max_event_bytes(mut self, max_bytes: usize) -> Self {
        self.max_event_bytes = max_bytes;
        self
    }

    pub fn validate(&self) -> Result<(), McpError> {
        let url = reqwest_013::Url::parse(&self.url)
            .map_err(|error| McpError::InvalidTransport(format!("invalid MCP SSE URL: {error}")))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(McpError::InvalidTransport(
                "MCP SSE URL must use http or https and include a host".to_string(),
            ));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(McpError::InvalidTransport(
                "MCP SSE URL must not contain credentials".to_string(),
            ));
        }
        if self.max_event_bytes == 0 || self.max_event_bytes > MAX_EVENT_BYTES {
            return Err(McpError::InvalidTransport(format!(
                "MCP SSE event limit must be between 1 and {MAX_EVENT_BYTES} bytes"
            )));
        }
        if self.bearer_token.as_deref().is_some_and(|token| {
            token.trim().is_empty() || token.to_ascii_lowercase().starts_with("bearer ")
        }) {
            return Err(McpError::InvalidTransport(
                "MCP bearer token must be a non-empty raw token".to_string(),
            ));
        }
        self.retry.validate()?;
        for (name, value) in &self.headers {
            let parsed_name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                McpError::InvalidTransport(format!("invalid MCP SSE header name: {name}"))
            })?;
            HeaderValue::from_str(value).map_err(|_| {
                McpError::InvalidTransport(format!("invalid MCP SSE header value for {name}"))
            })?;
            if matches!(parsed_name, AUTHORIZATION | ACCEPT | CONTENT_TYPE)
                || parsed_name == HeaderName::from_static("last-event-id")
            {
                return Err(McpError::InvalidTransport(
                    "MCP SSE custom headers must not override protocol headers".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) async fn connect(
        self,
        client_config: McpClientConfig,
    ) -> Result<McpClient, McpError> {
        self.validate()?;
        let transport = LegacySseTransport::start(self, client_config.request_timeout)
            .await
            .map_err(|error| McpError::Initialization(error.to_string()))?;
        McpClient::connect(transport, client_config).await
    }
}

#[derive(Debug, Clone)]
struct SseEvent {
    event: Option<String>,
    data: String,
    id: Option<String>,
    retry: Option<Duration>,
}

#[derive(Debug, Default)]
struct SseParser {
    line_buffer: Vec<u8>,
    event_bytes: usize,
    event: Option<String>,
    data: Vec<String>,
    id: Option<String>,
    retry: Option<Duration>,
}

impl SseParser {
    fn push(
        &mut self,
        chunk: &[u8],
        max_event_bytes: usize,
    ) -> Result<Vec<SseEvent>, McpLegacySseError> {
        self.line_buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(index) = self.line_buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.line_buffer.drain(..=index).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.event_bytes = self.event_bytes.saturating_add(line.len() + 1);
            if self.event_bytes > max_event_bytes {
                return Err(McpLegacySseError::new("SSE event exceeded configured size"));
            }
            if line.is_empty() {
                if self.event.is_some()
                    || !self.data.is_empty()
                    || self.id.is_some()
                    || self.retry.is_some()
                {
                    events.push(SseEvent {
                        event: self.event.take(),
                        data: self.data.drain(..).collect::<Vec<_>>().join("\n"),
                        id: self.id.take(),
                        retry: self.retry.take(),
                    });
                }
                self.event_bytes = 0;
                continue;
            }
            if line.first() == Some(&b':') {
                continue;
            }
            let line = String::from_utf8(line)
                .map_err(|_| McpLegacySseError::new("SSE stream is not valid UTF-8"))?;
            let (field, value) = line
                .split_once(':')
                .map(|(field, value)| (field, value.strip_prefix(' ').unwrap_or(value)))
                .unwrap_or((line.as_str(), ""));
            match field {
                "event" => self.event = Some(value.to_string()),
                "data" => self.data.push(value.to_string()),
                "id" if !value.contains('\0') => self.id = Some(value.to_string()),
                "retry" => {
                    if let Ok(milliseconds) = value.parse::<u64>() {
                        self.retry = Some(Duration::from_millis(milliseconds).min(MAX_RETRY_DELAY));
                    }
                }
                _ => {}
            }
        }
        if self.line_buffer.len() > max_event_bytes {
            return Err(McpLegacySseError::new("SSE event exceeded configured size"));
        }
        Ok(events)
    }

    fn finish(self) -> Result<Vec<SseEvent>, McpLegacySseError> {
        if self.line_buffer.is_empty() {
            Ok(Vec::new())
        } else {
            Err(McpLegacySseError::new("SSE stream ended mid-event"))
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpLegacySseError {
    message: String,
}

impl McpLegacySseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for McpLegacySseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for McpLegacySseError {}

struct LegacySseState {
    endpoint: Mutex<Option<reqwest_013::Url>>,
    last_event_id: Mutex<Option<String>>,
    error: Mutex<Option<String>>,
    endpoint_ready: Notify,
    cancel: watch::Sender<bool>,
    closed: AtomicBool,
}

struct LegacySseTransport {
    client: reqwest_013::Client,
    message_url: reqwest_013::Url,
    headers: HeaderMap,
    request_timeout: Duration,
    state: Arc<LegacySseState>,
    incoming: mpsc::Receiver<ServerJsonRpcMessage>,
    stream_task: Option<JoinHandle<()>>,
}

impl LegacySseTransport {
    async fn start(
        config: McpLegacySseConfig,
        request_timeout: Duration,
    ) -> Result<Self, McpLegacySseError> {
        let message_url = reqwest_013::Url::parse(&config.url)
            .map_err(|_| McpLegacySseError::new("invalid MCP SSE URL"))?;
        let headers = build_headers(&config)?;
        let client = crate::providers::http::build_mcp_client().map_err(McpLegacySseError::new)?;
        let (cancel, cancel_rx) = watch::channel(false);
        let state = Arc::new(LegacySseState {
            endpoint: Mutex::new(None),
            last_event_id: Mutex::new(None),
            error: Mutex::new(None),
            endpoint_ready: Notify::new(),
            cancel,
            closed: AtomicBool::new(false),
        });
        let (incoming_tx, incoming) = mpsc::channel(64);
        let task_state = state.clone();
        let task_config = config.clone();
        let task_client = client.clone();
        let stream_task = tokio::spawn(async move {
            run_stream(task_client, task_config, task_state, incoming_tx, cancel_rx).await;
        });
        let endpoint_result =
            tokio::time::timeout(request_timeout, wait_for_endpoint(state.clone()))
                .await
                .map_err(|_| McpLegacySseError::new("MCP SSE endpoint discovery timed out"))?;
        if let Err(error) = endpoint_result {
            state.closed.store(true, Ordering::Release);
            state.cancel.send_replace(true);
            stream_task.abort();
            let _ = stream_task.await;
            return Err(error);
        }
        Ok(Self {
            client,
            message_url,
            headers,
            request_timeout,
            state,
            incoming,
            stream_task: Some(stream_task),
        })
    }
}

impl rmcp::transport::Transport<RoleClient> for LegacySseTransport {
    type Error = McpLegacySseError;

    fn send(
        &mut self,
        item: rmcp::service::TxJsonRpcMessage<RoleClient>,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        let body = serde_json::to_vec(&item)
            .map_err(|_| McpLegacySseError::new("MCP SSE message serialization failed"));
        let client = self.client.clone();
        let headers = self.headers.clone();
        let state = self.state.clone();
        let message_url = self.message_url.clone();
        let request_timeout = self.request_timeout;
        async move {
            let body = body?;
            let endpoint = wait_for_endpoint(state.clone()).await?;
            let request = client
                .post(resolve_message_url(&message_url, &endpoint)?)
                .headers(headers)
                .header(CONTENT_TYPE, "application/json")
                .timeout(request_timeout)
                .body(body);
            let response = request
                .send()
                .await
                .map_err(|_| McpLegacySseError::new("MCP SSE message request failed"))?;
            if !response.status().is_success() {
                return Err(McpLegacySseError::new(format!(
                    "MCP SSE message endpoint returned HTTP {}",
                    response.status().as_u16()
                )));
            }
            Ok(())
        }
    }

    fn receive(
        &mut self,
    ) -> impl std::future::Future<Output = Option<rmcp::service::RxJsonRpcMessage<RoleClient>>> + Send
    {
        self.incoming.recv()
    }

    fn close(&mut self) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        self.state.closed.store(true, Ordering::Release);
        self.state.cancel.send_replace(true);
        let task = self.stream_task.take();
        async move {
            if let Some(task) = task {
                task.abort();
                let _ = task.await;
            }
            Ok(())
        }
    }
}

async fn wait_for_endpoint(
    state: Arc<LegacySseState>,
) -> Result<reqwest_013::Url, McpLegacySseError> {
    loop {
        let notified = state.endpoint_ready.notified();
        if let Some(endpoint) = state.endpoint.lock().await.clone() {
            return Ok(endpoint);
        }
        if let Some(error) = state.error.lock().await.clone() {
            return Err(McpLegacySseError::new(error));
        }
        if state.closed.load(Ordering::Acquire) {
            return Err(McpLegacySseError::new("MCP SSE transport is closed"));
        }
        notified.await;
    }
}

async fn run_stream(
    client: reqwest_013::Client,
    config: McpLegacySseConfig,
    state: Arc<LegacySseState>,
    incoming: mpsc::Sender<ServerJsonRpcMessage>,
    mut cancel: watch::Receiver<bool>,
) {
    let base_url = match reqwest_013::Url::parse(&config.url) {
        Ok(url) => url,
        Err(_) => {
            set_stream_error(&state, "invalid MCP SSE URL").await;
            return;
        }
    };
    let mut retry_attempt = 0usize;
    let mut last_event_id: Option<String> = None;
    let mut server_retry = None;
    loop {
        if *cancel.borrow() || state.closed.load(Ordering::Acquire) {
            return;
        }
        let mut request = client
            .get(base_url.clone())
            .headers(build_runtime_headers(&config));
        request = request.header(ACCEPT, "text/event-stream");
        if let Some(event_id) = last_event_id.as_deref() {
            request = request.header("last-event-id", event_id);
        }
        let response = tokio::select! {
            _ = cancel.changed() => return,
            result = request.send() => result,
        };
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(response) if matches!(response.status().as_u16(), 401 | 403) => {
                set_stream_error(&state, "MCP SSE endpoint authorization failed").await;
                return;
            }
            Ok(_) | Err(_) => {
                if !wait_before_retry(&config.retry, &mut retry_attempt, &mut cancel).await {
                    set_stream_error(&state, "MCP SSE endpoint could not be reached").await;
                    return;
                }
                continue;
            }
        };
        let mut parser = SseParser::default();
        let mut stream = response.bytes_stream();
        loop {
            let next = tokio::select! {
                _ = cancel.changed() => return,
                chunk = stream.next() => chunk,
            };
            let Some(next) = next else { break };
            let chunk = match next {
                Ok(chunk) => chunk,
                Err(_) => {
                    break;
                }
            };
            let events = match parser.push(&chunk, config.max_event_bytes) {
                Ok(events) => events,
                Err(error) => {
                    set_stream_error(&state, &error.to_string()).await;
                    return;
                }
            };
            for event in events {
                if let Some(id) = event.id.clone() {
                    last_event_id = Some(id.clone());
                    *state.last_event_id.lock().await = Some(id);
                }
                if let Some(retry) = event.retry {
                    server_retry = Some(retry);
                }
                if event
                    .event
                    .as_deref()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("endpoint"))
                {
                    match resolve_endpoint(&base_url, &event.data) {
                        Ok(endpoint) => {
                            *state.endpoint.lock().await = Some(endpoint);
                            state.endpoint_ready.notify_waiters();
                        }
                        Err(error) => {
                            set_stream_error(&state, &error.to_string()).await;
                            return;
                        }
                    }
                    continue;
                }
                if event
                    .event
                    .as_deref()
                    .is_some_and(|kind| !kind.is_empty() && kind != "message")
                {
                    continue;
                }
                if event.data.trim().is_empty() {
                    continue;
                }
                let message = match serde_json::from_str::<ServerJsonRpcMessage>(&event.data) {
                    Ok(message) => message,
                    Err(_) => {
                        set_stream_error(&state, "MCP SSE message is not valid JSON-RPC").await;
                        return;
                    }
                };
                if incoming.send(message).await.is_err() {
                    return;
                }
            }
        }
        if let Err(error) = parser.finish() {
            set_stream_error(&state, &error.to_string()).await;
            return;
        }
        let Some(policy_delay) = config.retry.delay_for_attempt(retry_attempt) else {
            set_stream_error(&state, "MCP SSE stream retries exhausted").await;
            return;
        };
        let delay = server_retry.take().unwrap_or(policy_delay);
        retry_attempt = retry_attempt.saturating_add(1);
        if !sleep_with_cancel(delay, &mut cancel).await {
            return;
        }
    }
}

async fn wait_before_retry(
    retry: &McpSseConfig,
    attempt: &mut usize,
    cancel: &mut watch::Receiver<bool>,
) -> bool {
    let Some(delay) = retry.delay_for_attempt(*attempt) else {
        return false;
    };
    *attempt = attempt.saturating_add(1);
    sleep_with_cancel(delay, cancel).await
}

async fn sleep_with_cancel(delay: Duration, cancel: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = cancel.changed() => false,
        _ = tokio::time::sleep(delay) => true,
    }
}

async fn set_stream_error(state: &LegacySseState, message: &str) {
    state.closed.store(true, Ordering::Release);
    *state.error.lock().await = Some(message.to_string());
    state.cancel.send_replace(true);
    state.endpoint_ready.notify_waiters();
}

fn build_headers(config: &McpLegacySseConfig) -> Result<HeaderMap, McpLegacySseError> {
    let mut headers = HeaderMap::new();
    for (name, value) in &config.headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| McpLegacySseError::new("invalid MCP SSE header name"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| McpLegacySseError::new("invalid MCP SSE header value"))?;
        headers.insert(name, value);
    }
    if let Some(token) = config.bearer_token.as_deref() {
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| McpLegacySseError::new("invalid MCP bearer token"))?;
        headers.insert(AUTHORIZATION, value);
    }
    Ok(headers)
}

fn build_runtime_headers(config: &McpLegacySseConfig) -> HeaderMap {
    build_headers(config).expect("validated MCP SSE headers")
}

fn resolve_endpoint(
    base_url: &reqwest_013::Url,
    value: &str,
) -> Result<reqwest_013::Url, McpLegacySseError> {
    if value.len() > MAX_ENDPOINT_BYTES || value.trim().is_empty() {
        return Err(McpLegacySseError::new("MCP SSE endpoint is invalid"));
    }
    let endpoint = base_url
        .join(value.trim())
        .map_err(|_| McpLegacySseError::new("MCP SSE endpoint is invalid"))?;
    if !same_origin(base_url, &endpoint)
        || !matches!(endpoint.scheme(), "http" | "https")
        || endpoint.username() != ""
        || endpoint.password().is_some()
    {
        return Err(McpLegacySseError::new(
            "MCP SSE endpoint must be same-origin and credential-free",
        ));
    }
    Ok(endpoint)
}

fn resolve_message_url(
    base_url: &reqwest_013::Url,
    endpoint: &reqwest_013::Url,
) -> Result<reqwest_013::Url, McpLegacySseError> {
    if !same_origin(base_url, endpoint) {
        return Err(McpLegacySseError::new("MCP SSE endpoint origin changed"));
    }
    Ok(endpoint.clone())
}

fn same_origin(left: &reqwest_013::Url, right: &reqwest_013::Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_does_not_overflow() {
        let config = McpSseConfig::new(Some(128), Duration::from_secs(60));
        assert!(config.delay_for_attempt(127).is_none());
    }

    #[test]
    fn parser_handles_multiline_data_and_crlf() {
        let mut parser = SseParser::default();
        let events = parser
            .push(
                b"id: evt-1\r\nevent: message\r\ndata: {\"a\":\r\ndata: 1}\r\n\r\n",
                1024,
            )
            .expect("SSE frame should parse");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id.as_deref(), Some("evt-1"));
        assert_eq!(events[0].data, "{\"a\":\n1}");
    }

    #[test]
    fn endpoint_resolution_rejects_cross_origin_urls() {
        let base = reqwest_013::Url::parse("https://example.com/sse").unwrap();
        assert!(resolve_endpoint(&base, "/message").is_ok());
        assert!(resolve_endpoint(&base, "https://other.example/message").is_err());
    }

    #[test]
    fn parser_keeps_retry_fields_and_allows_many_small_events_in_one_chunk() {
        let mut parser = SseParser::default();
        let events = parser
            .push(
                b"retry: 1000\n\nid: one\ndata: {}\n\nid: two\ndata: {}\n\n",
                32,
            )
            .expect("small SSE frames should parse");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].retry, Some(Duration::from_secs(1)));
        assert_eq!(events[1].id.as_deref(), Some("one"));
        assert_eq!(events[2].id.as_deref(), Some("two"));
    }
}
