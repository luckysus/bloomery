use super::types::{
    AgentEventSink, AgentLoopAttachment, AgentLoopError, ContextEntry, EvidenceAttachment,
    PreparedToolCall, ToolExecutionError, ToolRegistration,
};
use crate::agent::context::{
    estimate_tokens, ContextBudgetError, ContextReport, ContextSource, DEFAULT_MODEL_LIMIT,
};
use crate::agent::protocol::{
    AgentError, AgentErrorCategory, AgentEventData, AgentMessageRole, MessageDelta, ToolCompleted,
    ToolOutcome, UsageUpdated,
};
use crate::agent::tool_repair::{repair_tool_call, ToolRepairError, ToolSpec};
use crate::providers::capabilities::{
    ChatImage, ChatMessage, ChatResponse, ChatToolCall, ChatUsage,
};
use crate::providers::http::ProviderErrorCode;
use base64::{engine::general_purpose::STANDARD, Engine as _};
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
            concurrency: registration.concurrency,
            timeout: registration.timeout,
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
                        "description": tool_description(&registration.spec.name),
                        "parameters": registration.spec.input_schema,
                    }
                })
            })
            .collect(),
    )
}

fn tool_description(name: &str) -> String {
    match name {
        "search_literature" => "混合检索本地知识库中的文献片段、结论、文献配图和金相照片；优先走本地 hybrid RAG，必要时降级到 FTS。".to_string(),
        "read_literature_section" => "读取本地知识库中已解析文档的目录、摘要、参考文献或指定章节原文。".to_string(),
        "query_production_data" => "查询本地导入的生产数据集，包含钢卷批次、实测成分、实测力学性能和实际工艺参数；这不是标准查询。".to_string(),
        "query_composition_standard" => "查询本地知识库中的成分标准，例如钢级、牌号、出钢记号对应的元素含量范围。".to_string(),
        "query_process_standard" => "查询本地知识库中的工艺标准，例如轧制温度、卷取温度、冷却制度等标准工艺参数范围。".to_string(),
        "ask_llm_with_context" => "整理当前证据给本地 AgentLoop 继续合成最终中文回答；不要在工具内部递归调用模型。".to_string(),
        "predict_performance" => "基于 Suna 本地已完成训练的模型预测力学性能；需要本地 datasetId、trainingTaskId 和 featureValues，不能调用 Web 云模型。".to_string(),
        "optimize_process" => "基于 Suna 本地已完成训练的模型执行工艺/参数寻优；需要用户确认，不能调用 Web 云优化服务。".to_string(),
        "match_coil" => "按目标屈服强度、抗拉强度或延伸率，在本地生产数据中匹配性能相近的历史钢卷。".to_string(),
        "get_model_status" => "查看本地已注册的钢铁模型、版本和激活状态；只读，不做预测、不训练、不优化。".to_string(),
        "start_training" => "启动本地模型训练任务；高风险操作，只有用户明确要求训练/重新训练/更新模型时才调用。".to_string(),
        "process_literature" => "把本地 PDF、Markdown、Office 文档加入 Suna 知识库；需要用户确认，并使用本地配置的 MinerU/Embedding provider。".to_string(),
        "export_data" => "识别导出意图并提示用户在结果区手动导出，避免 Agent 后台自动写文件。".to_string(),
        "remember_memory" => "保存用户确认的长期偏好、稳定事实、任务状态或纠正。".to_string(),
        "read_memory" => "按记忆 ID 读取当前本地工作区的一条长期记忆。".to_string(),
        "search_memory" => "搜索当前本地工作区的长期记忆摘要。".to_string(),
        "list_memory" => "列出当前本地工作区的长期记忆，可按类型或关键词过滤。".to_string(),
        "forget_memory" => "让一条长期记忆不再被召回；需要用户确认。".to_string(),
        _ => format!("Suna tool {name}"),
    }
}

pub(super) fn record_usage(
    sink: &mut dyn AgentEventSink,
    usage: &ChatUsage,
) -> Result<(), AgentLoopError> {
    sink.record(AgentEventData::UsageUpdated(UsageUpdated {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        reasoning_tokens: usage.reasoning_tokens,
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
    total.cache_read_tokens = total
        .cache_read_tokens
        .saturating_add(current.cache_read_tokens);
    total.reasoning_tokens = total
        .reasoning_tokens
        .saturating_add(current.reasoning_tokens);
    total
}

pub(super) fn denied_observations(
    sink: &mut dyn AgentEventSink,
    calls: Vec<(PreparedToolCall, Option<String>)>,
) -> Result<Vec<ChatMessage>, AgentLoopError> {
    calls
        .into_iter()
        .map(|(call, reason)| {
            let message =
                reason.unwrap_or_else(|| format!("permission denied for tool {}", call.tool_name));
            let error = AgentError {
                code: "permission_denied".to_string(),
                category: AgentErrorCategory::ToolPermission,
                message: message.clone(),
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
                format!("tool {} was denied: {}", call.tool_name, message),
            ))
        })
        .collect()
}

pub(super) fn record_tool_result(
    sink: &mut dyn AgentEventSink,
    call: &PreparedToolCall,
    artifact_store: Option<&dyn crate::tools::ArtifactStore>,
    result: Result<Value, ToolExecutionError>,
) -> Result<ChatMessage, AgentLoopError> {
    match result {
        Ok(value) => {
            let (output, observation) = bounded_tool_output(value, artifact_store)?;
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
                details: error
                    .details
                    .clone()
                    .map(crate::tools::redact_sensitive_value),
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

fn bounded_tool_output(
    value: Value,
    artifact_store: Option<&dyn crate::tools::ArtifactStore>,
) -> Result<(Value, String), AgentLoopError> {
    let value = crate::tools::redact_sensitive_value(value);
    let serialized = serde_json::to_string(&value).map_err(|error| {
        AgentLoopError::Tool(format!("tool output serialization failed: {error}"))
    })?;
    if serialized.len() <= super::types::MAX_TOOL_OUTPUT_BYTES {
        return Ok((value, serialized));
    }
    if let Some(store) = artifact_store {
        let output = crate::tools::bound_output(value, store)
            .map_err(|error| AgentLoopError::Tool(error.to_string()))?;
        if let Some(artifact) = output.artifact {
            let file_name = artifact
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| AgentLoopError::Tool("tool artifact path is invalid".to_string()))?;
            let bounded = json!({
                "truncated": true,
                "bytes": serialized.len(),
                "artifact_id": artifact.id,
                "path": format!(".agent/artifacts/{file_name}"),
                "message": "完整工具结果已持久化，可按 path 读取"
            });
            let observation = serde_json::to_string(&bounded).map_err(|error| {
                AgentLoopError::Tool(format!("bounded tool output failed: {error}"))
            })?;
            return Ok((output.model_output, observation));
        }
        return Ok((output.model_output, serialized));
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
    attachments: &[AgentLoopAttachment],
) -> Vec<ChatMessage> {
    let selected = report
        .included_items
        .iter()
        .filter_map(|item| {
            entries
                .iter()
                .find(|entry| entry.item.id == item.id)
                .map(|entry| {
                    let mut rendered = entry.clone();
                    rendered.item = item.clone();
                    rendered
                })
        })
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
            ContextSource::RecentTurn { newest_first_rank } => Some((newest_first_rank, entry)),
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
        let mut content = entry.item.content.clone();
        let mut images = Vec::new();
        for attachment in attachments {
            if attachment.mime.to_ascii_lowercase().starts_with("image/") {
                images.push(ChatImage {
                    data: attachment.data.clone(),
                    mime: attachment.mime.clone(),
                });
                continue;
            }
            let attachment_note = if let Some(path) = attachment
                .path
                .as_deref()
                .filter(|path| !path.trim().is_empty())
            {
                format!(
                    "附件已登记在本地路径：{path}。如需读取，请使用受 Agent 权限约束的文件工具。"
                )
            } else {
                match STANDARD.decode(&attachment.data) {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(text) if !text.trim().is_empty() => {
                            let bounded = text.chars().take(120_000).collect::<String>();
                            format!("附件内容：\n{bounded}")
                        }
                        _ => "二进制附件已添加。请使用可用的文献或文件工具读取它。".to_string(),
                    },
                    Err(_) => {
                        "附件内容无法直接预览，请使用可用的文献或文件工具读取它。".to_string()
                    }
                }
            };
            content.push_str(&format!(
                "\n\n[附件：{}，类型：{}]\n{}",
                attachment.name, attachment.mime, attachment_note
            ));
        }
        messages.push(if images.is_empty() {
            ChatMessage::new(role_text(entry.role), content)
        } else {
            ChatMessage::with_images(role_text(entry.role), content, images)
        });
    }
    messages
}

/// Build the provider-facing context for every model call.
///
/// The durable loop history stays intact. Only the request view is compacted,
/// matching Vetta's host-owned `transformContext` contract.
pub(super) fn budget_chat_messages(
    messages: Vec<ChatMessage>,
    model_limit: Option<usize>,
    output_reservation: usize,
    reasoning_reservation: usize,
    tools: Option<&Value>,
) -> Result<Vec<ChatMessage>, ContextBudgetError> {
    let model_limit = model_limit.unwrap_or(DEFAULT_MODEL_LIMIT);
    let completion_reservation = output_reservation.saturating_add(reasoning_reservation);
    if completion_reservation > model_limit {
        return Err(ContextBudgetError::CompletionReservationExceedsModelLimit {
            output_reservation,
            reasoning_reservation,
            model_limit,
        });
    }
    let input_limit = model_limit.saturating_sub(completion_reservation);
    let tool_tokens = tools
        .map(|value| estimate_tokens(&value.to_string()))
        .unwrap_or_default();
    if tool_tokens > input_limit {
        return Err(ContextBudgetError::ToolSchemaExceedsLimit {
            tool_tokens,
            input_limit,
            model_limit,
        });
    }
    let message_limit = input_limit.saturating_sub(tool_tokens);
    if messages
        .iter()
        .map(estimate_chat_message_tokens)
        .sum::<usize>()
        <= message_limit
    {
        return Ok(messages);
    }

    let mut selected = Vec::new();
    let mut remaining = message_limit;
    for (index, message) in messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.role == "system")
    {
        let cost = estimate_chat_message_tokens(message);
        if cost > remaining {
            return Err(ContextBudgetError::MessageBlockExceedsLimit {
                block_tokens: cost,
                input_limit: message_limit,
            });
        }
        selected.push((index, message.clone()));
        remaining = remaining.saturating_sub(cost);
    }

    let mut index = messages.len();
    while index > 0 {
        let end = index;
        let start = message_block_start(&messages, end);
        let block = &messages[start..end];
        if block.iter().all(|message| message.role == "system") {
            index = start;
            continue;
        }
        let cost = block
            .iter()
            .map(estimate_chat_message_tokens)
            .sum::<usize>();
        if cost <= remaining {
            for (offset, message) in block.iter().enumerate() {
                if !selected
                    .iter()
                    .any(|(selected_index, _)| *selected_index == start + offset)
                {
                    selected.push((start + offset, message.clone()));
                }
            }
            remaining = remaining.saturating_sub(cost);
        } else if (end == messages.len() && block_is_tool_call_block(block))
            || selected.is_empty()
            || (end == messages.len() && !block_is_tool_call_block(block))
        {
            if block_is_tool_call_block(block) {
                let compacted = compact_tool_block(block, remaining).ok_or_else(|| {
                    ContextBudgetError::MessageBlockExceedsLimit {
                        block_tokens: cost,
                        input_limit: message_limit,
                    }
                })?;
                let used = compacted
                    .iter()
                    .map(estimate_chat_message_tokens)
                    .sum::<usize>();
                selected.extend(
                    compacted
                        .into_iter()
                        .enumerate()
                        .map(|(offset, message)| (start + offset, message)),
                );
                remaining = remaining.saturating_sub(used);
                index = start;
                continue;
            }
            if remaining == 0 {
                return Err(ContextBudgetError::MessageBlockExceedsLimit {
                    block_tokens: cost,
                    input_limit: message_limit,
                });
            }
            let mut message = messages[end - 1].clone();
            let content = std::mem::take(&mut message.content);
            let overhead = estimate_chat_message_tokens(&message);
            if overhead >= remaining {
                return Err(ContextBudgetError::MessageBlockExceedsLimit {
                    block_tokens: cost,
                    input_limit: message_limit,
                });
            }
            message.content = truncate_to_tokens(&content, remaining.saturating_sub(overhead));
            selected.push((end - 1, message));
            break;
        }
        index = start;
    }

    selected.sort_by_key(|(index, _)| *index);
    Ok(selected.into_iter().map(|(_, message)| message).collect())
}

fn compact_tool_block(block: &[ChatMessage], budget: usize) -> Option<Vec<ChatMessage>> {
    let mut result = Vec::with_capacity(block.len());
    let mut remaining = budget;
    for message in block {
        let mut compact = message.clone();
        if compact.role == "tool" && estimate_chat_message_tokens(&compact) > remaining {
            let content = std::mem::take(&mut compact.content);
            let overhead = estimate_chat_message_tokens(&compact);
            if overhead >= remaining {
                return None;
            }
            let marker = "\n[tool output truncated to fit context budget]";
            let marker_tokens = estimate_tokens(marker);
            if overhead.saturating_add(marker_tokens) >= remaining {
                return None;
            }
            let content_budget = remaining - overhead - marker_tokens;
            let mut preview = truncate_to_tokens(&content, content_budget);
            compact.content = format!("{preview}{marker}");
            // The token estimator is intentionally conservative and framing can
            // change at the truncation boundary. Trim until the complete tool
            // message, including the marker, fits the remaining request budget.
            while estimate_chat_message_tokens(&compact) > remaining {
                let preview_tokens = estimate_tokens(&preview);
                if preview_tokens == 0 {
                    return None;
                }
                preview = truncate_to_tokens(&preview, preview_tokens - 1);
                compact.content = format!("{preview}{marker}");
            }
        }
        let cost = estimate_chat_message_tokens(&compact);
        if cost > remaining {
            return None;
        }
        remaining -= cost;
        result.push(compact);
    }
    Some(result)
}

fn message_block_start(messages: &[ChatMessage], end: usize) -> usize {
    let mut start = end.saturating_sub(1);
    if messages[start].role == "tool" {
        while start > 0 && messages[start - 1].role == "tool" {
            start -= 1;
        }
        if start > 0
            && messages[start - 1].role == "assistant"
            && !messages[start - 1].tool_calls.is_empty()
        {
            start -= 1;
        }
    }
    start
}

fn block_is_tool_call_block(block: &[ChatMessage]) -> bool {
    block.iter().any(|message| {
        message.role == "tool" || (message.role == "assistant" && !message.tool_calls.is_empty())
    })
}

pub(super) fn checkpoint_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .cloned()
        .map(|mut message| {
            // Image payloads are request-scoped and may contain sensitive or
            // very large base64 data; the durable checkpoint keeps text/tool state.
            message.images.clear();
            message
        })
        .collect()
}

fn estimate_chat_message_tokens(message: &ChatMessage) -> usize {
    let mut text = message.role.clone();
    text.push_str(&message.content);
    if let Some(reasoning) = &message.reasoning_content {
        text.push_str(reasoning);
    }
    for call in &message.tool_calls {
        text.push_str(&call.id);
        text.push_str(&call.name);
        text.push_str(&call.arguments);
    }
    text.push_str(&message.tool_call_id.clone().unwrap_or_default());
    estimate_tokens(&text)
        .saturating_add(16)
        .saturating_add(message.tool_calls.len().saturating_mul(32))
        .saturating_add(message.images.len().saturating_mul(4_096))
}

fn truncate_to_tokens(value: &str, limit: usize) -> String {
    crate::agent::context::truncate_to_tokens(value, limit)
}

pub(super) fn append_input_messages(messages: &mut Vec<ChatMessage>, entries: Vec<ContextEntry>) {
    messages.extend(
        entries
            .into_iter()
            .map(|entry| ChatMessage::new(role_text(entry.role), entry.item.content)),
    );
}

pub(super) fn role_text(role: AgentMessageRole) -> &'static str {
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
                ProviderErrorCode::ContextLimit => AgentErrorCategory::ModelCapability,
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
        AgentLoopError::Limit { .. } => (
            AgentErrorCategory::Internal,
            "agent_loop_limit_exceeded".to_string(),
            false,
        ),
        AgentLoopError::CheckpointTimeout { .. } => (
            AgentErrorCategory::Database,
            "agent_checkpoint_timeout".to_string(),
            true,
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
        details: match error {
            AgentLoopError::Provider(provider) => provider.details(),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{budget_chat_messages, render_context_messages, tool_description};
    use crate::agent::context::ContextBudgetError;
    use crate::providers::capabilities::{ChatMessage, ChatToolCall};
    use serde_json::json;

    #[test]
    fn provider_budget_keeps_tool_call_blocks_atomic() {
        let messages = vec![
            ChatMessage::new("system", "s"),
            ChatMessage::new("user", "old"),
            ChatMessage::assistant_tool_calls(vec![ChatToolCall {
                id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: "{}".to_string(),
            }]),
            ChatMessage::tool_result("call-1", "x".repeat(256)),
            ChatMessage::new("user", "now"),
        ];
        let bounded = budget_chat_messages(messages, Some(64), 0, 0, None)
            .expect("oversized tool block should be omitted as a whole");
        assert!(bounded.iter().all(|message| {
            message.role != "tool"
                && !(message.role == "assistant" && !message.tool_calls.is_empty())
        }));
        assert!(bounded.iter().any(|message| message.content == "now"));
    }

    #[test]
    fn provider_budget_reports_tool_schema_overflow() {
        let tools = json!({"schema": "x".repeat(256)});
        let error = budget_chat_messages(
            vec![ChatMessage::new("user", "request")],
            Some(16),
            0,
            0,
            Some(&tools),
        )
        .expect_err("tool schema overflow must be explicit");
        assert!(matches!(
            error,
            ContextBudgetError::ToolSchemaExceedsLimit { .. }
        ));
    }

    #[test]
    fn provider_budget_never_discards_the_latest_tool_observation() {
        let messages = vec![
            ChatMessage::new("system", "s"),
            ChatMessage::assistant_tool_calls(vec![ChatToolCall {
                id: "call-1".into(),
                name: "train".into(),
                arguments: "{}".into(),
            }]),
            ChatMessage::tool_result("call-1", "x".repeat(256)),
        ];
        assert!(matches!(
            budget_chat_messages(messages, Some(64), 0, 0, None),
            Err(ContextBudgetError::MessageBlockExceedsLimit { .. })
        ));
    }

    #[test]
    fn provider_budget_reserves_images_and_message_framing() {
        let message = ChatMessage::with_images(
            "user",
            "look",
            vec![crate::providers::capabilities::ChatImage {
                data: "aGVsbG8=".into(),
                mime: "image/png".into(),
            }],
        );
        assert!(matches!(
            budget_chat_messages(vec![message.clone()], Some(2_048), 0, 0, None),
            Err(ContextBudgetError::MessageBlockExceedsLimit { .. })
        ));
        assert_eq!(
            budget_chat_messages(vec![message], Some(8_192), 1_024, 0, None).unwrap()[0]
                .images
                .len(),
            1
        );
    }

    #[test]
    fn rendered_context_uses_the_budgeted_optional_content() {
        use crate::agent::context::{budget_context, estimate_tokens, ContextItem, ContextSource};
        use crate::agent::runtime::ContextEntry;
        let entries = vec![
            ContextEntry::new(ContextItem::new(
                "request",
                ContextSource::CurrentRequest,
                "answer",
            )),
            ContextEntry::new(ContextItem::new(
                "memory",
                ContextSource::ExplicitMemory,
                "x".repeat(500),
            )),
        ];
        let report = budget_context(
            &entries
                .iter()
                .map(|entry| entry.item.clone())
                .collect::<Vec<_>>(),
            Some(20),
            0,
            0,
        )
        .unwrap();
        let rendered = render_context_messages(&report, &entries, &[]);
        let memory = rendered
            .iter()
            .find(|message| message.role == "system")
            .unwrap();
        assert!(memory.content.len() < 500);
        assert_eq!(
            memory.content,
            report
                .included_items
                .iter()
                .find(|item| item.id == "memory")
                .unwrap()
                .content
        );
        assert!(
            rendered
                .iter()
                .map(|message| estimate_tokens(&message.content))
                .sum::<usize>()
                <= 20
        );
    }

    #[test]
    fn rendered_context_keeps_document_attachments_out_of_model_image_payloads() {
        use crate::agent::context::{budget_context, ContextItem, ContextSource};
        use crate::agent::runtime::{AgentLoopAttachment, ContextEntry};
        let entry = ContextEntry::new(ContextItem::new(
            "request",
            ContextSource::CurrentRequest,
            "请阅读附件",
        ));
        let report = budget_context(std::slice::from_ref(&entry.item), Some(2_048), 0, 0)
            .expect("request fits");
        let rendered = render_context_messages(
            &report,
            &[entry],
            &[AgentLoopAttachment {
                data: String::new(),
                mime: "application/pdf".to_string(),
                name: "标准.pdf".to_string(),
                path: Some("C:/research/标准.pdf".to_string()),
            }],
        );
        assert!(rendered[0].images.is_empty());
        assert!(rendered[0].content.contains("标准.pdf"));
        assert!(rendered[0].content.contains("C:/research/标准.pdf"));
    }

    #[test]
    fn steel_tool_descriptions_are_domain_specific() {
        assert!(tool_description("search_literature").contains("本地 hybrid RAG"));
        assert!(tool_description("predict_performance").contains("不能调用 Web 云模型"));
        assert_eq!(tool_description("custom_tool"), "Suna tool custom_tool");
    }
}
