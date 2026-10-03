use crate::diagnostics::redaction::Redactor;
use crate::providers::capabilities::{
    ChatEvent, ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatToolCall, ChatUsage,
    ProviderCapabilities, ToolCallDelta,
};
use crate::providers::http::{build_client, HttpClientConfig, ProviderError, ProviderErrorCode};
use crate::providers::profiles::{
    validate_bearer_transport, ProviderCapability, ProviderKind, ProviderProfile,
};
use crate::storage::secrets::SecretValue;
use futures_util::StreamExt;
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::timeout;

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(50);
const MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_SSE_BUFFER_BYTES: usize = 1024 * 1024;
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: usize = 4096;

/// Native Anthropic Messages API client.
///
/// Anthropic's API is close enough to OpenAI chat completions to share the
/// public ChatProvider contract, but the wire format is different: system
/// prompts are top-level, tools are input_schema objects, tool results are
/// user content blocks, and the streaming protocol uses typed events.
pub struct AnthropicProvider {
    profile: ProviderProfile,
    credential: Option<SecretValue>,
    client: Client,
    messages_url: String,
    capabilities: ProviderCapabilities,
}

impl AnthropicProvider {
    pub fn new(
        profile: ProviderProfile,
        credential: Option<SecretValue>,
    ) -> Result<Self, ProviderError> {
        if profile.kind != ProviderKind::Anthropic {
            return Err(ProviderError::new(
                ProviderErrorCode::ProviderResponse,
                None,
                "Anthropic provider requires an Anthropic profile",
            ));
        }
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
                "Anthropic model ID is required",
            )
        })?;
        let client = build_client(&HttpClientConfig::default())?;
        let mut capabilities = ProviderCapabilities::chat(profile.kind, model_id);
        // Messages supports structured tool input, but not OpenAI's
        // response_format/json_schema request field.
        capabilities.json_schema = false;
        Ok(Self {
            messages_url: normalize_anthropic_messages_url(&profile.base_url),
            profile,
            credential,
            client,
            capabilities,
        })
    }

    pub fn profile(&self) -> &ProviderProfile {
        &self.profile
    }
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    messages: Vec<AnthropicMessage>,
    max_tokens: usize,
    stream: bool,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<&'a [String]>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: Value,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    description: String,
    input_schema: Value,
}

impl ChatProvider for AnthropicProvider {
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
        if request.response_format.is_some() {
            return Err(ProviderError::new(
                ProviderErrorCode::UnsupportedCapability,
                None,
                "Anthropic Messages does not support response_format",
            ));
        }
        if is_cancelled() {
            return Ok(ChatResponse {
                cancelled: true,
                ..ChatResponse::default()
            });
        }
        let (system, messages) = anthropic_messages(&request.messages)?;
        let tools = request
            .tools
            .as_ref()
            .map(anthropic_tools)
            .transpose()?
            .unwrap_or_default();
        let body = AnthropicRequest {
            model: &self.capabilities.model_id,
            messages,
            max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            stream: true,
            temperature: request.temperature,
            system,
            tools,
            stop_sequences: request.stop.as_deref(),
        };
        let mut redactor = Redactor::new();
        let mut outbound = self
            .client
            .post(&self.messages_url)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .json(&body);
        if let Some(credential) = &self.credential {
            crate::diagnostics::observability::register_secret(credential);
            redactor = redactor.with_secret(credential);
            outbound = outbound.header("x-api-key", credential.expose());
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
            let headers = response.headers().clone();
            let body = match read_bounded_body(response, is_cancelled).await? {
                BodyRead::Complete(body) => body,
                BodyRead::Cancelled => {
                    return Ok(ChatResponse {
                        cancelled: true,
                        ..ChatResponse::default()
                    })
                }
            };
            return Err(ProviderError::from_status_with_headers(
                status,
                &headers,
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
                    })
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

fn anthropic_messages(
    messages: &[ChatMessage],
) -> Result<(Option<String>, Vec<AnthropicMessage>), ProviderError> {
    let mut system = Vec::new();
    let mut output = Vec::with_capacity(messages.len());
    for message in messages {
        match message.role.as_str() {
            "system" => {
                if !message.content.trim().is_empty() {
                    system.push(message.content.clone());
                }
            }
            "tool" => {
                let tool_call_id = message.tool_call_id.as_deref().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::ProviderResponse,
                        None,
                        "Anthropic tool result is missing tool_use_id",
                    )
                })?;
                output.push(AnthropicMessage {
                    role: "user",
                    content: json!([{
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": message.content,
                    }]),
                });
            }
            "assistant" => {
                let mut blocks = Vec::new();
                if !message.content.is_empty() {
                    blocks.push(json!({"type": "text", "text": message.content}));
                }
                blocks.extend(message.tool_calls.iter().map(|call| {
                    let input = serde_json::from_str::<Value>(&call.arguments)
                        .unwrap_or_else(|_| Value::String(call.arguments.clone()));
                    json!({"type": "tool_use", "id": call.id, "name": call.name, "input": input})
                }));
                output.push(AnthropicMessage {
                    role: "assistant",
                    content: if blocks.is_empty() {
                        Value::String(String::new())
                    } else {
                        Value::Array(blocks)
                    },
                });
            }
            "user" => {
                output.push(AnthropicMessage {
                    role: "user",
                    content: anthropic_user_content(message),
                });
            }
            role => {
                return Err(ProviderError::new(
                    ProviderErrorCode::ProviderResponse,
                    None,
                    format!("Anthropic Messages does not support role {role}"),
                ));
            }
        }
    }
    Ok(((!system.is_empty()).then(|| system.join("\n\n")), output))
}

fn anthropic_user_content(message: &ChatMessage) -> Value {
    if message.images.is_empty() {
        return Value::String(message.content.clone());
    }
    let mut blocks = Vec::with_capacity(message.images.len() + 1);
    if !message.content.is_empty() {
        blocks.push(json!({"type": "text", "text": message.content}));
    }
    blocks.extend(message.images.iter().map(|image| {
        json!({
            "type": "image",
            "source": {"type": "base64", "media_type": image.mime, "data": image.data},
        })
    }));
    Value::Array(blocks)
}

fn anthropic_tools(tools: &Value) -> Result<Vec<AnthropicTool>, ProviderError> {
    let Some(entries) = tools.as_array() else {
        return Err(ProviderError::new(
            ProviderErrorCode::ProviderResponse,
            None,
            "Anthropic tools must be an array",
        ));
    };
    entries
        .iter()
        .map(|tool| {
            let function = if tool["type"].as_str() == Some("function") {
                &tool["function"]
            } else {
                tool
            };
            let name = function["name"]
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorCode::ProviderResponse,
                        None,
                        "Anthropic tool name is required",
                    )
                })?;
            let schema = function["parameters"].clone();
            if !schema.is_object() {
                return Err(ProviderError::new(
                    ProviderErrorCode::ProviderResponse,
                    None,
                    "Anthropic tool input_schema must be an object",
                ));
            }
            Ok(AnthropicTool {
                name: name.to_string(),
                description: function["description"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                input_schema: schema,
            })
        })
        .collect()
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
            reasoning: String::new(),
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
    let mut event_name = String::new();
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
        received_bytes = received_bytes.saturating_add(chunk.len());
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
            &mut event_name,
            &mut event_data,
            &mut accumulator,
            on_event,
            redactor,
        )? {
            return Ok(accumulator.response(false));
        }
    }
    if !buffer.is_empty() {
        process_sse_bytes(
            b"\n",
            &mut buffer,
            &mut event_name,
            &mut event_data,
            &mut accumulator,
            on_event,
            redactor,
        )?;
    }
    if !event_data.is_empty() {
        if process_sse_event(
            &mut event_name,
            &mut event_data,
            &mut accumulator,
            on_event,
            redactor,
        )? {
            return Ok(accumulator.response(false));
        }
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
    event_name: &mut String,
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
            if process_sse_event(event_name, event_data, accumulator, on_event, redactor)? {
                return Ok(true);
            }
        } else if let Some(value) = line.strip_prefix("event:") {
            *event_name = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            event_data.push(value.strip_prefix(' ').unwrap_or(value).to_string());
        }
    }
    Ok(false)
}

fn process_sse_event(
    event_name: &mut String,
    event_data: &mut Vec<String>,
    accumulator: &mut ChatAccumulator,
    on_event: &mut (dyn FnMut(ChatEvent) + Send),
    redactor: &Redactor,
) -> Result<bool, ProviderError> {
    if event_data.is_empty() {
        event_name.clear();
        return Ok(false);
    }
    let data = std::mem::take(event_data).join("\n");
    let name = std::mem::take(event_name);
    let data = data.trim();
    if data.is_empty() {
        return Ok(false);
    }
    let value = serde_json::from_str::<Value>(data).map_err(|error| {
        ProviderError::new(
            ProviderErrorCode::ProviderResponse,
            None,
            format!("provider returned malformed Anthropic SSE JSON: {error}"),
        )
    })?;
    apply_anthropic_value(&name, &value, accumulator, on_event, redactor)?;
    Ok(name == "message_stop" || value["type"] == "message_stop")
}

fn apply_anthropic_value(
    event_name: &str,
    value: &Value,
    accumulator: &mut ChatAccumulator,
    on_event: &mut (dyn FnMut(ChatEvent) + Send),
    redactor: &Redactor,
) -> Result<(), ProviderError> {
    if value["type"] == "error" || event_name == "error" {
        return Err(ProviderError::new(
            ProviderErrorCode::ProviderResponse,
            None,
            redactor.redact_json(&value["error"]).to_string(),
        ));
    }
    match value["type"].as_str().unwrap_or(event_name) {
        "message_start" => {
            if let Some(usage) = usage_from(&value["message"]["usage"]) {
                accumulator.usage = Some(usage.clone());
                on_event(ChatEvent::Usage(usage));
            }
        }
        "content_block_start" => {
            let index = value["index"].as_u64().unwrap_or(0) as usize;
            let block = &value["content_block"];
            if block["type"] == "tool_use" {
                let call = ChatToolCall {
                    id: block["id"].as_str().unwrap_or_default().to_string(),
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    arguments: String::new(),
                };
                let id = (!call.id.is_empty()).then_some(call.id.clone());
                let name = (!call.name.is_empty()).then_some(call.name.clone());
                accumulator.tool_calls.insert(index, call);
                on_event(ChatEvent::ToolCallDelta(ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments: String::new(),
                }));
            } else if block["type"] == "text" {
                if let Some(text) = block["text"].as_str().filter(|text| !text.is_empty()) {
                    accumulator.text.push_str(text);
                    on_event(ChatEvent::TextDelta(text.to_string()));
                }
            }
        }
        "content_block_delta" => {
            let index = value["index"].as_u64().unwrap_or(0) as usize;
            let delta = &value["delta"];
            match delta["type"].as_str() {
                Some("text_delta") => {
                    if let Some(text) = delta["text"].as_str() {
                        accumulator.text.push_str(text);
                        on_event(ChatEvent::TextDelta(text.to_string()));
                    }
                }
                Some("input_json_delta") => {
                    let arguments = delta["partial_json"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    let call = accumulator.tool_calls.entry(index).or_default();
                    call.arguments.push_str(&arguments);
                    on_event(ChatEvent::ToolCallDelta(ToolCallDelta {
                        index,
                        id: None,
                        name: None,
                        arguments,
                    }));
                }
                _ => {}
            }
        }
        "message_delta" => {
            if let Some(reason) = value["delta"]["stop_reason"].as_str() {
                accumulator.finish_reason = Some(reason.to_string());
            }
            if let Some(output) = value["usage"]["output_tokens"].as_u64() {
                let mut usage = accumulator.usage.clone().unwrap_or_default();
                usage.completion_tokens = output;
                usage.total_tokens = usage.prompt_tokens.saturating_add(output);
                accumulator.usage = Some(usage.clone());
                on_event(ChatEvent::Usage(usage));
            }
        }
        _ => {}
    }
    Ok(())
}

fn usage_from(value: &Value) -> Option<ChatUsage> {
    let prompt = value["input_tokens"].as_u64()?;
    let cache_read = value["cache_read_input_tokens"].as_u64().unwrap_or(0);
    Some(ChatUsage {
        prompt_tokens: prompt,
        completion_tokens: 0,
        total_tokens: prompt,
        cache_read_tokens: cache_read,
        reasoning_tokens: 0,
    })
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
    if value["type"] == "error" {
        return Err(ProviderError::new(
            ProviderErrorCode::ProviderResponse,
            None,
            redactor.redact_json(&value["error"]).to_string(),
        ));
    }
    let mut accumulator = ChatAccumulator::default();
    for (index, block) in value["content"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        match block["type"].as_str() {
            Some("text") => {
                if let Some(text) = block["text"].as_str() {
                    accumulator.text.push_str(text);
                    on_event(ChatEvent::TextDelta(text.to_string()));
                }
            }
            Some("tool_use") => {
                let input = block["input"].to_string();
                let call = ChatToolCall {
                    id: block["id"].as_str().unwrap_or_default().to_string(),
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    arguments: input,
                };
                on_event(ChatEvent::ToolCallDelta(ToolCallDelta {
                    index,
                    id: Some(call.id.clone()),
                    name: Some(call.name.clone()),
                    arguments: call.arguments.clone(),
                }));
                accumulator.tool_calls.insert(index, call);
            }
            _ => {}
        }
    }
    accumulator.finish_reason = value["stop_reason"].as_str().map(str::to_string);
    accumulator.usage = usage_from(&value["usage"]);
    if let Some(usage) = accumulator.usage.clone() {
        on_event(ChatEvent::Usage(usage));
    }
    Ok(accumulator.response(false))
}

pub fn normalize_anthropic_messages_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/messages") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::capabilities::{ChatImage, ChatMessage};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::thread;

    #[test]
    fn normalizes_messages_endpoint() {
        assert_eq!(
            normalize_anthropic_messages_url("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            normalize_anthropic_messages_url("https://example.test"),
            "https://example.test/v1/messages"
        );
        assert_eq!(
            normalize_anthropic_messages_url("https://example.test/v1/messages/"),
            "https://example.test/v1/messages"
        );
    }

    #[test]
    fn converts_system_tools_and_images_to_messages_format() {
        let messages = vec![
            ChatMessage::new("system", "be precise"),
            ChatMessage::with_images(
                "user",
                "inspect",
                vec![ChatImage {
                    data: "ZmFrZQ==".to_string(),
                    mime: "image/png".to_string(),
                }],
            ),
        ];
        let (system, messages) = anthropic_messages(&messages).unwrap();
        assert_eq!(system.as_deref(), Some("be precise"));
        assert_eq!(messages[0].content[0]["type"], "text");
        assert_eq!(messages[0].content[1]["source"]["media_type"], "image/png");
        let tools = anthropic_tools(&json!([{"type":"function","function":{"name":"lookup","description":"find","parameters":{"type":"object"}}}])).unwrap();
        let payload = serde_json::to_value(&tools[0]).unwrap();
        assert_eq!(payload["input_schema"]["type"], "object");
    }

    #[test]
    fn parses_anthropic_stream_tool_and_text_events() {
        let mut buffer = Vec::new();
        let mut event_name = String::new();
        let mut event_data = Vec::new();
        let mut accumulator = ChatAccumulator::default();
        let mut events = Vec::new();
        let redactor = Redactor::new();
        let stream = concat!(
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool_1\",\"name\":\"lookup\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"q\\\":1}\"}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":2}}\n\n",
        );
        process_sse_bytes(
            stream.as_bytes(),
            &mut buffer,
            &mut event_name,
            &mut event_data,
            &mut accumulator,
            &mut |event| events.push(event),
            &redactor,
        )
        .unwrap();
        assert_eq!(accumulator.tool_calls[&0].id, "tool_1");
        assert_eq!(accumulator.tool_calls[&0].arguments, "{\"q\":1}");
        assert_eq!(accumulator.finish_reason.as_deref(), Some("tool_use"));
        assert!(events
            .iter()
            .any(|event| matches!(event, ChatEvent::ToolCallDelta(_))));
    }

    #[test]
    fn chat_uses_native_messages_headers_stream_and_tool_schema() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind Anthropic test server");
        let address = listener.local_addr().expect("read test server address");
        let body = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":3}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool_1\",\"name\":\"lookup\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"q\\\":1}\"}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept Anthropic request");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let size = stream.read(&mut chunk).expect("read Anthropic request");
                request.extend_from_slice(&chunk[..size]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") || size == 0 {
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{}",
                body.len(), body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write Anthropic response");
            String::from_utf8_lossy(&request).to_string()
        });
        let provider = AnthropicProvider::new(
            ProviderProfile {
                id: uuid::Uuid::new_v4(),
                kind: ProviderKind::Anthropic,
                display_name: "Anthropic test".to_string(),
                base_url: format!("http://{address}/v1"),
                model_id: Some("claude-test".to_string()),
                secret_ref: Some("api_key".to_string()),
                enabled: true,
            },
            Some(SecretValue::new("sk-anthropic-test").expect("secret")),
        )
        .expect("build Anthropic provider");
        let request = ChatRequest {
            messages: vec![
                ChatMessage::new("system", "be precise"),
                ChatMessage::new("user", "hello"),
            ],
            temperature: 0.2,
            tools: Some(
                json!([{"type":"function","function":{"name":"lookup","description":"find","parameters":{"type":"object"}}}]),
            ),
            response_format: None,
            reasoning_effort: None,
            max_tokens: Some(128),
            stop: None,
        };
        let mut events = Vec::new();
        let response = tauri::async_runtime::block_on(provider.chat(
            request,
            &mut |event| events.push(event),
            &|| false,
        ))
        .expect("Anthropic chat");
        let request_text = server.join().expect("join Anthropic test server");
        assert!(request_text.starts_with("POST /v1/messages HTTP/1.1"));
        assert!(request_text
            .to_ascii_lowercase()
            .contains("x-api-key: sk-anthropic-test"));
        assert!(request_text
            .to_ascii_lowercase()
            .contains("anthropic-version: 2023-06-01"));
        assert!(!request_text
            .to_ascii_lowercase()
            .contains("authorization: bearer"));
        assert!(request_text.contains("\"input_schema\""));
        assert!(request_text.contains("\"system\":\"be precise\""));
        assert_eq!(response.text, "ok");
        assert_eq!(response.tool_calls[0].arguments, "{\"q\":1}");
        assert_eq!(response.usage.unwrap().total_tokens, 5);
        assert!(events
            .iter()
            .any(|event| matches!(event, ChatEvent::TextDelta(delta) if delta == "ok")));
    }

    #[test]
    fn chat_returns_cancelled_while_request_is_pending() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind cancellation test server");
        let address = listener
            .local_addr()
            .expect("read cancellation server address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept cancellation request");
            thread::sleep(Duration::from_millis(300));
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\n\r\n{}",
            );
        });
        let provider = AnthropicProvider::new(
            ProviderProfile {
                id: uuid::Uuid::new_v4(),
                kind: ProviderKind::Anthropic,
                display_name: "Anthropic cancellation".to_string(),
                base_url: format!("http://{address}/v1"),
                model_id: Some("claude-test".to_string()),
                secret_ref: None,
                enabled: true,
            },
            None,
        )
        .expect("build cancellation provider");
        let calls = Arc::new(AtomicUsize::new(0));
        let cancellation_calls = Arc::clone(&calls);
        let request = ChatRequest::single_turn("system", "hello");
        let response =
            tauri::async_runtime::block_on(provider.chat(request, &mut |_| {}, &move || {
                cancellation_calls.fetch_add(1, Ordering::SeqCst) > 1
            }))
            .expect("cancelled chat");
        server.join().expect("join cancellation server");
        assert!(response.cancelled);
        assert!(calls.load(Ordering::SeqCst) > 1);
    }
}
