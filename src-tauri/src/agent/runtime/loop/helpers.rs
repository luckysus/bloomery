use super::types::{
    AgentEventSink, AgentLoopError, ContextEntry, EvidenceAttachment, PreparedToolCall,
    ToolExecutionError, ToolRegistration,
};
use crate::agent::context::{ContextReport, ContextSource};
use crate::agent::protocol::{
    AgentError, AgentErrorCategory, AgentEventData, AgentMessageRole, MessageDelta, ToolCompleted,
    ToolOutcome, UsageUpdated,
};
use crate::agent::tool_repair::{repair_tool_call, ToolRepairError, ToolSpec};
use crate::providers::capabilities::{ChatMessage, ChatResponse, ChatToolCall, ChatUsage};
use crate::providers::http::ProviderErrorCode;
use serde_json::{json, Value};
use uuid::Uuid;

pub(super) fn prepare_tool_calls(
    model_calls: &[ChatToolCall],
    specs: &[ToolSpec],
    registrations: &[ToolRegistration],
) -> Result<Vec<PreparedToolCall>, ToolRepairError> {
    let mut prepared = Vec::with_capacity(model_calls.len());
    for model_call in model_calls {
        let raw = serde_json::to_string(&json!({
            "name": model_call.name,
            "arguments": model_call.arguments,
        }))
        .map_err(|error| ToolRepairError::InvalidJson {
            message: error.to_string(),
        })?;
        let repaired = repair_tool_call(&raw, specs)?;
        let registration = registrations
            .iter()
            .find(|registration| registration.spec.id == repaired.tool_id)
            .ok_or_else(|| ToolRepairError::UnknownTool {
                name: repaired.tool_name.clone(),
            })?;
        let model_call_id = if model_call.id.trim().is_empty() {
            format!("tool-{}", Uuid::new_v4())
        } else {
            model_call.id.clone()
        };
        prepared.push(PreparedToolCall {
            model_call_id,
            tool_call_id: Uuid::new_v4(),
            tool_id: repaired.tool_id,
            tool_name: repaired.tool_name,
            arguments: repaired.arguments,
            risk: repaired.risk,
            read_only: registration.read_only,
        });
    }
    Ok(prepared)
}

pub(super) fn tool_definitions(registrations: &[ToolRegistration]) -> Value {
    Value::Array(
        registrations
            .iter()
            .map(|registration| {
                json!({
                    "type": "function",
                    "function": {
                        "name": registration.spec.name,
                        "description": format!("Bloomery tool {}", registration.spec.name),
                        "parameters": registration.spec.input_schema,
                    }
                })
            })
            .collect(),
    )
}

pub(super) fn record_usage(
    sink: &mut dyn AgentEventSink,
    usage: &ChatUsage,
) -> Result<(), AgentLoopError> {
    sink.record(AgentEventData::UsageUpdated(UsageUpdated {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
    }))
    .map_err(AgentLoopError::EventSink)?;
    Ok(())
}

pub(super) fn add_usage(previous: Option<ChatUsage>, current: &ChatUsage) -> ChatUsage {
    let mut total = previous.unwrap_or_default();
    total.prompt_tokens = total.prompt_tokens.saturating_add(current.prompt_tokens);
    total.completion_tokens = total
        .completion_tokens
        .saturating_add(current.completion_tokens);
    total.total_tokens = total.total_tokens.saturating_add(current.total_tokens);
    total
}

pub(super) fn denied_observations(
    sink: &mut dyn AgentEventSink,
    calls: Vec<PreparedToolCall>,
) -> Result<Vec<ChatMessage>, AgentLoopError> {
    calls
        .into_iter()
        .map(|call| {
            let error = AgentError {
                code: "permission_denied".to_string(),
                category: AgentErrorCategory::ToolPermission,
                message: format!("permission denied for tool {}", call.tool_name),
                retryable: false,
                details: None,
            };
            sink.record(AgentEventData::ToolCompleted(ToolCompleted {
                tool_call_id: call.tool_call_id,
                outcome: ToolOutcome::Failed,
                output: None,
                error: Some(error.clone()),
            }))
            .map_err(AgentLoopError::EventSink)?;
            Ok(ChatMessage::tool_result(
                call.model_call_id,
                format!("tool {} was denied: {}", call.tool_name, error.message),
            ))
        })
        .collect()
}

pub(super) fn record_tool_result(
    sink: &mut dyn AgentEventSink,
    call: &PreparedToolCall,
    result: Result<Value, ToolExecutionError>,
) -> Result<ChatMessage, AgentLoopError> {
    match result {
        Ok(value) => {
            let (output, observation) = bounded_tool_output(value)?;
            sink.record(AgentEventData::ToolCompleted(ToolCompleted {
                tool_call_id: call.tool_call_id,
                outcome: ToolOutcome::Succeeded,
                output: Some(output),
                error: None,
            }))
            .map_err(AgentLoopError::EventSink)?;
            Ok(ChatMessage::tool_result(
                call.model_call_id.clone(),
                observation,
            ))
        }
        Err(error) => {
            let outcome = if error.cancelled {
                ToolOutcome::Cancelled
            } else {
                ToolOutcome::Failed
            };
            let agent_error = AgentError {
                code: format!("tool_{}", error.code),
                category: AgentErrorCategory::ToolPermission,
                message: error.message.clone(),
                retryable: false,
                details: None,
            };
            sink.record(AgentEventData::ToolCompleted(ToolCompleted {
                tool_call_id: call.tool_call_id,
                outcome,
                output: None,
                error: Some(agent_error),
            }))
            .map_err(AgentLoopError::EventSink)?;
            Ok(ChatMessage::tool_result(
                call.model_call_id.clone(),
                format!("tool {} failed: {}", call.tool_name, error.message),
            ))
        }
    }
}

fn bounded_tool_output(value: Value) -> Result<(Value, String), AgentLoopError> {
    let serialized = serde_json::to_string(&value).map_err(|error| {
        AgentLoopError::Tool(format!("tool output serialization failed: {error}"))
    })?;
    if serialized.len() <= super::types::MAX_TOOL_OUTPUT_BYTES {
        return Ok((value, serialized));
    }
    let preview = truncate_utf8(&serialized, super::types::MAX_TOOL_OUTPUT_BYTES);
    let bounded = json!({"truncated": true, "bytes": serialized.len(), "preview": preview});
    let observation = serde_json::to_string(&bounded)
        .map_err(|error| AgentLoopError::Tool(format!("bounded tool output failed: {error}")))?;
    Ok((bounded, observation))
}

fn truncate_utf8(value: &str, limit: usize) -> String {
    let mut end = limit.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

pub(super) fn append_response_text(
    mut answer: String,
    response: &ChatResponse,
    streamed_text: &str,
    sink: &mut dyn AgentEventSink,
    message_id: Uuid,
) -> Result<String, AgentLoopError> {
    let trailing = response
        .text
        .strip_prefix(streamed_text)
        .unwrap_or(response.text.as_str());
    if !trailing.is_empty() {
        sink.record(AgentEventData::MessageDelta(MessageDelta {
            message_id,
            role: AgentMessageRole::Assistant,
            delta: trailing.to_string(),
        }))
        .map_err(AgentLoopError::EventSink)?;
    }
    answer.push_str(&response.text);
    Ok(answer)
}

pub(super) fn render_context_messages(
    report: &ContextReport,
    entries: &[ContextEntry],
) -> Vec<ChatMessage> {
    let selected = report
        .included_items
        .iter()
        .filter_map(|item| entries.iter().find(|entry| entry.item.id == item.id))
        .collect::<Vec<_>>();
    let mut messages = selected
        .iter()
        .filter(|entry| {
            !matches!(
                entry.item.source,
                ContextSource::RecentTurn { .. } | ContextSource::CurrentRequest
            )
        })
        .map(|entry| ChatMessage::new(role_text(entry.role), entry.item.content.clone()))
        .collect::<Vec<_>>();
    let mut recent = selected
        .iter()
        .filter_map(|entry| match entry.item.source {
            ContextSource::RecentTurn { newest_first_rank } => Some((newest_first_rank, *entry)),
            _ => None,
        })
        .collect::<Vec<_>>();
    recent.sort_by(|left, right| right.0.cmp(&left.0));
    messages.extend(
        recent
            .into_iter()
            .map(|(_, entry)| ChatMessage::new(role_text(entry.role), entry.item.content.clone())),
    );
    if let Some(entry) = selected
        .iter()
        .find(|entry| entry.item.source == ContextSource::CurrentRequest)
    {
        messages.push(ChatMessage::new(
            role_text(entry.role),
            entry.item.content.clone(),
        ));
    }
    messages
}

fn role_text(role: AgentMessageRole) -> &'static str {
    match role {
        AgentMessageRole::User => "user",
        AgentMessageRole::Assistant => "assistant",
        AgentMessageRole::Tool => "tool",
        AgentMessageRole::System => "system",
    }
}

pub(super) fn validate_citations(
    answer: &str,
    evidence: Option<&EvidenceAttachment>,
) -> Result<(), AgentLoopError> {
    let Some(evidence) = evidence else {
        return Ok(());
    };
    for citation in bracketed_numbers(answer) {
        if !evidence.citation_numbers.contains(&citation) {
            return Err(AgentLoopError::Citation(format!(
                "answer cites unavailable evidence [{citation}]"
            )));
        }
    }
    Ok(())
}

fn bracketed_numbers(text: &str) -> Vec<u32> {
    let mut numbers = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else {
            break;
        };
        if let Ok(number) = after[..end].trim().parse::<u32>() {
            numbers.push(number);
        }
        rest = &after[end + 1..];
    }
    numbers
}

pub(super) fn to_agent_error(error: &AgentLoopError) -> AgentError {
    let (category, code, retryable) = match error {
        AgentLoopError::Context(_) => (
            AgentErrorCategory::ModelCapability,
            "context_budget_exceeded".to_string(),
            false,
        ),
        AgentLoopError::Provider(error) => (
            match error.code() {
                ProviderErrorCode::Authentication => AgentErrorCategory::Authentication,
                ProviderErrorCode::Quota => AgentErrorCategory::Quota,
                ProviderErrorCode::Network | ProviderErrorCode::Timeout => {
                    AgentErrorCategory::Network
                }
                ProviderErrorCode::UnsupportedCapability => AgentErrorCategory::ModelCapability,
                ProviderErrorCode::Cancelled => AgentErrorCategory::Network,
                ProviderErrorCode::ProviderResponse => AgentErrorCategory::Internal,
            },
            format!("provider_{}", error.code().as_str()),
            matches!(
                error.code(),
                ProviderErrorCode::Network | ProviderErrorCode::Timeout
            ),
        ),
        AgentLoopError::ToolRepair(error) => (
            AgentErrorCategory::ToolPermission,
            error.code().to_string(),
            false,
        ),
        AgentLoopError::Citation(_) => (
            AgentErrorCategory::Indexing,
            "citation_invalid".to_string(),
            false,
        ),
        AgentLoopError::Capability(_) => (
            AgentErrorCategory::ModelCapability,
            "provider_capability_missing".to_string(),
            false,
        ),
        AgentLoopError::Tool(_) => (
            AgentErrorCategory::ToolPermission,
            "tool_failed".to_string(),
            false,
        ),
        AgentLoopError::EventSink(_) | AgentLoopError::Internal(_) => (
            AgentErrorCategory::Internal,
            "agent_runtime_failed".to_string(),
            false,
        ),
    };
    AgentError {
        code,
        category,
        message: error.to_string(),
        retryable,
        details: None,
    }
}
