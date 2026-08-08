use crate::diagnostics::redaction::Redactor;
use crate::providers::capabilities::{
    ChatEvent, ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatToolCall, ChatUsage,
    ProviderCapabilities, ToolCallDelta,
};
use crate::providers::http::{build_client, HttpClientConfig, ProviderError, ProviderErrorCode};
use crate::providers::profiles::{validate_bearer_transport, ProviderCapability, ProviderProfile};
use crate::storage::secrets::SecretValue;
use futures_util::StreamExt;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::timeout;

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(50);
const MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_SSE_BUFFER_BYTES: usize = 1024 * 1024;

pub struct OpenAiProvider {
    profile: ProviderProfile,
    credential: Option<SecretValue>,
    client: Client,
    chat_url: String,
    capabilities: ProviderCapabilities,
}

impl OpenAiProvider {
    pub fn new(
        profile: ProviderProfile,
        credential: Option<SecretValue>,
    ) -> Result<Self, ProviderError> {
        let chat_url = normalize_openai_chat_url(&profile.base_url);
        Self::with_endpoint(profile, credential, chat_url)
    }

    pub(crate) fn with_endpoint(
        profile: ProviderProfile,
        credential: Option<SecretValue>,
        chat_url: String,
    ) -> Result<Self, ProviderError> {
        let profile = profile.validate().map_err(|message| {
            ProviderError::new(ProviderErrorCode::ProviderResponse, None, message)
        })?;
        validate_bearer_transport(&profile.base_url, credential.is_some()).map_err(|message| {
            ProviderError::new(ProviderErrorCode::ProviderResponse, None, message)
        })?;
        if !profile.kind.supports(ProviderCapability::Chat) {
            return Err(ProviderError::new(
                ProviderErrorCode::UnsupportedCapability,
                None,
                "provider profile does not support chat",
            ));
        }
        let model_id = profile.model_id.clone().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorCode::ProviderResponse,
                None,
                "chat provider model ID is required",
            )
        })?;
        let client = build_client(&HttpClientConfig::default())?;
        let capabilities = ProviderCapabilities::chat(profile.kind, model_id);
        Ok(Self {
            profile,
            credential,
            client,
            chat_url,
            capabilities,
        })
    }

    pub fn profile(&self) -> &ProviderProfile {
        &self.profile
    }
}

#[derive(Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: &'a [OpenAiChatMessage<'a>],
    stream: bool,
    temperature: f32,
    stream_options: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<&'a Value>,
}

#[derive(Serialize)]
struct OpenAiChatMessage<'a> {
    role: &'a str,
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OpenAiToolCall<'a>>,
}

#[derive(Serialize)]
struct OpenAiToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: OpenAiFunction<'a>,
}

#[derive(Serialize)]
struct OpenAiFunction<'a> {
    name: &'a str,
    arguments: &'a str,
}

impl ChatProvider for OpenAiProvider {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    async fn chat(
        &self,
        request: ChatRequest,
        on_event: &mut (dyn FnMut(ChatEvent) + Send),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<ChatResponse, ProviderError> {
        self.capabilities.require(ProviderCapability::Chat)?;
        if is_cancelled() {
            return Ok(ChatResponse {
                cancelled: true,
                ..ChatResponse::default()
            });
        }

        let messages = request
            .messages
            .iter()
            .map(openai_message)
            .collect::<Vec<_>>();
        let body = OpenAiChatRequest {
            model: &self.capabilities.model_id,
            messages: &messages,
            stream: true,
            temperature: request.temperature,
            stream_options: serde_json::json!({"include_usage": true}),
            tools: request.tools.as_ref(),
            response_format: request.response_format.as_ref(),
        };
        let mut redactor = Redactor::new();
        let mut outbound = self.client.post(&self.chat_url).json(&body);
        if let Some(credential) = &self.credential {
            crate::diagnostics::observability::register_secret(credential);
            redactor = redactor.with_secret(credential);
            outbound = outbound.bearer_auth(credential.expose());
        }
        let mut pending_response = Box::pin(outbound.send());
        let response = loop {
            if is_cancelled() {
                return Ok(ChatResponse {
                    cancelled: true,
                    ..ChatResponse::default()
                });
            }
            if let Ok(response) = timeout(CANCELLATION_POLL_INTERVAL, &mut pending_response).await {
                break response.map_err(|error| ProviderError::from_reqwest(&error))?;
            }
        };
        let status = response.status();
        if !status.is_success() {
            let body = match read_bounded_body(response, is_cancelled).await? {
                BodyRead::Complete(body) => body,
                BodyRead::Cancelled => {
                    return Ok(ChatResponse {
                        cancelled: true,
                        ..ChatResponse::default()
                    });
                }
            };
            return Err(ProviderError::from_status(
                status,
                &String::from_utf8_lossy(&body),
                &redactor,
            ));
        }

        let is_event_stream = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"));
        if is_event_stream {
            read_sse_response(response, on_event, is_cancelled, &redactor).await
        } else {
            let body = match read_bounded_body(response, is_cancelled).await? {
                BodyRead::Complete(body) => body,
                BodyRead::Cancelled => {
                    return Ok(ChatResponse {
                        cancelled: true,
                        ..ChatResponse::default()
                    });
                }
            };
            let body = std::str::from_utf8(&body).map_err(|_| {
                ProviderError::new(
                    ProviderErrorCode::ProviderResponse,
                    None,
                    "provider returned non-UTF-8 JSON",
                )
            })?;
            read_json_response(body, on_event, &redactor)
        }
    }
}

fn openai_message(message: &ChatMessage) -> OpenAiChatMessage<'_> {
    OpenAiChatMessage {
        role: &message.role,
        content: &message.content,
        tool_call_id: message.tool_call_id.as_deref(),
        tool_calls: message
            .tool_calls
            .iter()
            .map(|call| OpenAiToolCall {
                id: &call.id,
                kind: "function",
                function: OpenAiFunction {
                    name: &call.name,
                    arguments: &call.arguments,
                },
            })
            .collect(),
    }
}

enum BodyRead {
    Complete(Vec<u8>),
    Cancelled,
}

async fn read_bounded_body(
    response: reqwest::Response,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<BodyRead, ProviderError> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    loop {
        if is_cancelled() {
            return Ok(BodyRead::Cancelled);
        }
        let chunk = match timeout(CANCELLATION_POLL_INTERVAL, stream.next()).await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => return Ok(BodyRead::Complete(body)),
            Err(_) => continue,
        };
        let chunk = chunk.map_err(|error| ProviderError::from_reqwest(&error))?;
        append_bounded(&mut body, &chunk, MAX_RESPONSE_BODY_BYTES)?;
    }
}

fn append_bounded(buffer: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<(), ProviderError> {
    if bytes.len() > limit.saturating_sub(buffer.len()) {
        return Err(ProviderError::new(
            ProviderErrorCode::ProviderResponse,
            None,
            "provider response exceeded size limit",
        ));
    }
    buffer.extend_from_slice(bytes);
    Ok(())
}

#[derive(Default)]
struct ChatAccumulator {
    text: String,
    tool_calls: BTreeMap<usize, ChatToolCall>,
    usage: Option<ChatUsage>,
    finish_reason: Option<String>,
}

impl ChatAccumulator {
    fn response(self, cancelled: bool) -> ChatResponse {
        ChatResponse {
            text: self.text,
            tool_calls: self.tool_calls.into_values().collect(),
            usage: self.usage,
            finish_reason: self.finish_reason,
            cancelled,
        }
    }
}

async fn read_sse_response(
    response: reqwest::Response,
    on_event: &mut (dyn FnMut(ChatEvent) + Send),
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    redactor: &Redactor,
) -> Result<ChatResponse, ProviderError> {
    let mut accumulator = ChatAccumulator::default();
    let mut buffer = Vec::new();
    let mut event_data = Vec::new();
    let mut received_bytes = 0usize;
    let mut stream = response.bytes_stream();
    loop {
        if is_cancelled() {
            return Ok(accumulator.response(true));
        }
        let chunk = match timeout(CANCELLATION_POLL_INTERVAL, stream.next()).await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(_) => continue,
        };
        let chunk = chunk.map_err(|error| ProviderError::from_reqwest(&error))?;
        received_bytes = received_bytes.checked_add(chunk.len()).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorCode::ProviderResponse,
                None,
                "provider response exceeded size limit",
            )
        })?;
        if received_bytes > MAX_RESPONSE_BODY_BYTES {
            return Err(ProviderError::new(
                ProviderErrorCode::ProviderResponse,
                None,
                "provider response exceeded size limit",
            ));
        }
        if process_sse_bytes(
            &chunk,
            &mut buffer,
            &mut event_data,
            &mut accumulator,
            on_event,
            redactor,
        )? {
            return Ok(accumulator.response(false));
        }
    }
    if !buffer.is_empty()
        && process_sse_bytes(
            b"\n",
            &mut buffer,
            &mut event_data,
            &mut accumulator,
            on_event,
            redactor,
        )?
    {
        return Ok(accumulator.response(false));
    }
    if !event_data.is_empty()
        && process_sse_event(&mut event_data, &mut accumulator, on_event, redactor)?
    {
        return Ok(accumulator.response(false));
    }
    if accumulator.finish_reason.is_some() {
        Ok(accumulator.response(false))
    } else {
        Err(ProviderError::new(
            ProviderErrorCode::ProviderResponse,
            None,
            "provider SSE stream ended before completion",
        ))
    }
}

fn process_sse_bytes(
    bytes: &[u8],
    buffer: &mut Vec<u8>,
    event_data: &mut Vec<String>,
    accumulator: &mut ChatAccumulator,
    on_event: &mut (dyn FnMut(ChatEvent) + Send),
    redactor: &Redactor,
) -> Result<bool, ProviderError> {
    append_bounded(buffer, bytes, MAX_SSE_BUFFER_BYTES)?;
    while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
        let mut line = buffer.drain(..=index).collect::<Vec<_>>();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let Ok(line) = std::str::from_utf8(&line) else {
            continue;
        };
        if line.is_empty() {
            if process_sse_event(event_data, accumulator, on_event, redactor)? {
                return Ok(true);
            }
        } else if let Some(data) = line.strip_prefix("data:") {
            event_data.push(data.strip_prefix(' ').unwrap_or(data).to_string());
        }
    }
    Ok(false)
}

fn process_sse_event(
    event_data: &mut Vec<String>,
    accumulator: &mut ChatAccumulator,
    on_event: &mut (dyn FnMut(ChatEvent) + Send),
    redactor: &Redactor,
) -> Result<bool, ProviderError> {
    if event_data.is_empty() {
        return Ok(false);
    }
    let data = std::mem::take(event_data).join("\n");
    let data = data.trim();
    if data.is_empty() {
        return Ok(false);
    }
    if data == "[DONE]" {
        return Ok(true);
    }
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return Ok(false);
    };
    apply_openai_value(&value, accumulator, on_event, redactor)?;
    Ok(false)
}

fn read_json_response(
    body: &str,
    on_event: &mut (dyn FnMut(ChatEvent) + Send),
    redactor: &Redactor,
) -> Result<ChatResponse, ProviderError> {
    let value = serde_json::from_str::<Value>(body).map_err(|_| {
        ProviderError::new(
            ProviderErrorCode::ProviderResponse,
            None,
            "provider returned invalid JSON",
        )
    })?;
    let mut accumulator = ChatAccumulator::default();
    apply_openai_value(&value, &mut accumulator, on_event, redactor)?;
    Ok(accumulator.response(false))
}

fn apply_openai_value(
    value: &Value,
    accumulator: &mut ChatAccumulator,
    on_event: &mut (dyn FnMut(ChatEvent) + Send),
    redactor: &Redactor,
) -> Result<(), ProviderError> {
    if !value["error"].is_null() {
        return Err(ProviderError::new(
            ProviderErrorCode::ProviderResponse,
            None,
            redactor.redact_json(&value["error"]).to_string(),
        ));
    }

    let choice = &value["choices"][0];
    let delta = if choice["delta"].is_object() {
        &choice["delta"]
    } else {
        &choice["message"]
    };
    if let Some(content) = delta["content"]
        .as_str()
        .or_else(|| choice["text"].as_str())
    {
        accumulator.text.push_str(content);
        on_event(ChatEvent::TextDelta(content.to_string()));
    }

    if let Some(tool_calls) = delta["tool_calls"].as_array() {
        for (position, tool_call) in tool_calls.iter().enumerate() {
            let index = tool_call["index"]
                .as_u64()
                .map_or(position, |value| value as usize);
            let id = tool_call["id"].as_str().map(str::to_string);
            let name = tool_call["function"]["name"].as_str().map(str::to_string);
            let arguments = tool_call["function"]["arguments"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let accumulated = accumulator.tool_calls.entry(index).or_default();
            if let Some(id) = &id {
                accumulated.id.push_str(id);
            }
            if let Some(name) = &name {
                accumulated.name.push_str(name);
            }
            accumulated.arguments.push_str(&arguments);
            on_event(ChatEvent::ToolCallDelta(ToolCallDelta {
                index,
                id,
                name,
                arguments,
            }));
        }
    }

    if let Some(reason) = choice["finish_reason"].as_str() {
        accumulator.finish_reason = Some(reason.to_string());
    }
    if value["usage"].is_object() {
        let usage = ChatUsage {
            prompt_tokens: value["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            completion_tokens: value["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            total_tokens: value["usage"]["total_tokens"].as_u64().unwrap_or(0),
        };
        accumulator.usage = Some(usage.clone());
        on_event(ChatEvent::Usage(usage));
    }
    Ok(())
}

pub fn normalize_openai_chat_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

pub fn default_openai_base_url(provider: &str) -> Option<&'static str> {
    let provider = provider.trim();
    if provider.eq_ignore_ascii_case("deepseek") {
        Some("https://api.deepseek.com")
    } else if provider.eq_ignore_ascii_case("openai") {
        Some("https://api.openai.com/v1")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_decoder_preserves_utf8_split_across_chunks() {
        let line = "data: {\"choices\":[{\"delta\":{\"content\":\"钢\"}}]}\n\n";
        let bytes = line.as_bytes();
        let split = bytes
            .windows(3)
            .position(|window| window == "钢".as_bytes())
            .unwrap()
            + 1;
        let mut buffer = Vec::new();
        let mut event_data = Vec::new();
        let mut accumulator = ChatAccumulator::default();
        let mut events = Vec::new();
        let redactor = Redactor::new();

        assert!(!process_sse_bytes(
            &bytes[..split],
            &mut buffer,
            &mut event_data,
            &mut accumulator,
            &mut |event| events.push(event),
            &redactor,
        )
        .unwrap());
        assert!(!process_sse_bytes(
            &bytes[split..],
            &mut buffer,
            &mut event_data,
            &mut accumulator,
            &mut |event| events.push(event),
            &redactor,
        )
        .unwrap());

        assert_eq!(accumulator.text, "钢");
        assert_eq!(events, vec![ChatEvent::TextDelta("钢".to_string())]);
    }

    #[test]
    fn bounded_buffers_reject_oversized_provider_bodies() {
        let mut buffer = vec![1, 2];

        append_bounded(&mut buffer, &[3, 4], 4).unwrap();
        let error = append_bounded(&mut buffer, &[5], 4).unwrap_err();

        assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
        assert_eq!(buffer, vec![1, 2, 3, 4]);
    }

    #[test]
    fn sse_decoder_joins_multiline_data_events() {
        let bytes = concat!(
            "data: {\"choices\":[\n",
            "data: {\"delta\":{\"content\":\"joined\"}}]}\n\n"
        );
        let mut buffer = Vec::new();
        let mut event_data = Vec::new();
        let mut accumulator = ChatAccumulator::default();
        let redactor = Redactor::new();

        process_sse_bytes(
            bytes.as_bytes(),
            &mut buffer,
            &mut event_data,
            &mut accumulator,
            &mut |_| {},
            &redactor,
        )
        .unwrap();

        assert_eq!(accumulator.text, "joined");
    }
}
