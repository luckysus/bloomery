use super::types::{
    AgentEventSink, AgentLoopAttachment, AgentLoopError, ContextEntry, EvidenceAttachment,
    PreparedToolCall, ToolExecutionError, ToolRegistration,
};
use crate::agent::context::{ContextReport, ContextSource};
use crate::agent::protocol::{
    AgentError, AgentErrorCategory, AgentEventData, AgentMessageRole, MessageDelta, ToolCompleted,
    ToolOutcome, UsageUpdated,
};
use crate::agent::tool_repair::{repair_tool_call, ToolRepairError, ToolSpec};
use crate::providers::capabilities::{
    ChatImage, ChatMessage, ChatResponse, ChatToolCall, ChatUsage,
};
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
        "predict_performance" => "基于 Bloomery 本地已完成训练的模型预测力学性能；需要本地 datasetId、trainingTaskId 和 featureValues，不能调用 Web 云模型。".to_string(),
        "optimize_process" => "基于 Bloomery 本地已完成训练的模型执行工艺/参数寻优；需要用户确认，不能调用 Web 云优化服务。".to_string(),
        "match_coil" => "按目标屈服强度、抗拉强度或延伸率，在本地生产数据中匹配性能相近的历史钢卷。".to_string(),
        "get_model_status" => "查看本地已注册的钢铁模型、版本和激活状态；只读，不做预测、不训练、不优化。".to_string(),
        "start_training" => "启动本地模型训练任务；高风险操作，只有用户明确要求训练/重新训练/更新模型时才调用。".to_string(),
        "process_literature" => "把本地 PDF、Markdown、Office 文档加入 Bloomery 知识库；需要用户确认，并使用本地配置的 MinerU/Embedding provider。".to_string(),
        "export_data" => "识别导出意图并提示用户在结果区手动导出，避免 Agent 后台自动写文件。".to_string(),
        "remember_memory" => "保存用户确认的长期偏好、稳定事实、任务状态或纠正。".to_string(),
        "read_memory" => "按记忆 ID 读取当前本地工作区的一条长期记忆。".to_string(),
        "search_memory" => "搜索当前本地工作区的长期记忆摘要。".to_string(),
        "list_memory" => "列出当前本地工作区的长期记忆，可按类型或关键词过滤。".to_string(),
        "forget_memory" => "让一条长期记忆不再被召回；需要用户确认。".to_string(),
        _ => format!("Bloomery tool {name}"),
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
    attachments: &[AgentLoopAttachment],
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
        messages.push(ChatMessage::with_images(
            role_text(entry.role),
            entry.item.content.clone(),
            attachments
                .iter()
                .map(|attachment| ChatImage {
                    data: attachment.data.clone(),
                    mime: attachment.mime.clone(),
                })
                .collect(),
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

#[cfg(test)]
mod tests {
    use super::tool_description;

    #[test]
    fn steel_tool_descriptions_are_domain_specific() {
        assert!(tool_description("search_literature").contains("本地 hybrid RAG"));
        assert!(tool_description("predict_performance").contains("不能调用 Web 云模型"));
        assert_eq!(tool_description("custom_tool"), "Bloomery tool custom_tool");
    }
}
