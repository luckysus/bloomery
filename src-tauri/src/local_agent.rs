use crate::agent::context::{
    build_summary_prompt, estimate_summary_tokens, messages_after_covered_id, plan_summary,
    SummaryMessage,
};
#[cfg(test)]
use crate::agent::context::{SummaryPlan, SUMMARY_MIN_FOLD_TOKENS, SUMMARY_TRIGGER_TOKENS};
use crate::agent::session::model::StartRunRequest;
use crate::agent::session::service::SessionService;
use crate::context::build_context_packet;
use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::providers::capabilities::{ChatEvent, ChatProvider, ChatRequest};
use crate::providers::configured_chat_provider;
use crate::providers::profiles::{
    resolve_chat_profile, ProviderCapability, ProviderKind, ProviderProfile,
};
use crate::storage::repositories::settings;
use crate::storage::secrets::SecretValue;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, sync::Mutex};
use tauri::Emitter;
use uuid::Uuid;

const LOCAL_LLM_CONFIG_KEY: &str = "local_llm_config";
const LOCAL_ASK_CONTEXT_LIMIT: usize = 12;
const LOCAL_ASK_CONTEXT_CHAR_LIMIT: usize = 1800;
const LOCAL_SUMMARY_CONTEXT_LIMIT: usize = 64;
const LOCAL_SUMMARY_CONTEXT_CHAR_LIMIT: usize = 2500;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAgentChatRequest {
    pub session_id: Option<String>,
    pub message: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAskRequest {
    pub query: String,
    pub contexts: Vec<String>,
    pub mode: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizeConversationRequest {
    pub conversation_id: String,
    pub covered_message_id: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizeConversationResponse {
    pub summarized: bool,
    pub summary: Option<String>,
    pub covered_message_id: Option<String>,
    pub total_tokens: usize,
    pub folded_tokens: usize,
}

#[derive(Default)]
pub struct LocalAgentState {
    cancelled_runs: Mutex<HashSet<String>>,
}

#[tauri::command]
pub fn desktop_cancel_llm_run(
    state: tauri::State<LocalAgentState>,
    run_id: String,
) -> Result<(), String> {
    let run_id = run_id.trim();
    if run_id.is_empty() {
        return Ok(());
    }
    let mut cancelled = state
        .cancelled_runs
        .lock()
        .map_err(|_| "local agent state poisoned")?;
    cancelled.insert(run_id.to_string());
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LocalLlmConfig {
    provider: String,
    base_url: String,
    model_name: String,
    api_key: String,
}

#[derive(Clone, Debug, Serialize)]
struct LocalAgentDelta {
    run_id: String,
    delta: String,
}

struct ExistingSummary {
    summary: String,
    covered_message_id: Option<String>,
}

struct StreamedLlmAnswer {
    text: String,
    stopped: bool,
}

fn assistant_content_for_stream_result(answer: &StreamedLlmAnswer) -> String {
    if !answer.stopped {
        return answer.text.clone();
    }
    let text = answer.text.trim_end();
    if text.is_empty() {
        "已停止生成。".to_string()
    } else {
        format!("{text}\n\n[已停止生成]")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DesktopIntentKind {
    LocalQa,
    KnowledgeQa,
    OptimizationAdvice,
    OptimizationTask,
    TrainingTask,
    LiteratureTask,
    Clarify,
}

impl DesktopIntentKind {
    fn as_str(self) -> &'static str {
        match self {
            DesktopIntentKind::LocalQa => "local_qa",
            DesktopIntentKind::KnowledgeQa => "knowledge_qa",
            DesktopIntentKind::OptimizationAdvice => "optimization_advice",
            DesktopIntentKind::OptimizationTask => "optimization_task",
            DesktopIntentKind::TrainingTask => "training_task",
            DesktopIntentKind::LiteratureTask => "literature_task",
            DesktopIntentKind::Clarify => "clarify",
        }
    }
}

#[derive(Clone, Debug)]
struct DesktopRoute {
    intent: DesktopIntentKind,
    confidence: f32,
    reason: &'static str,
    unavailable_capability: Option<&'static str>,
}
#[tauri::command]
pub async fn desktop_agent_chat(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: LocalAgentChatRequest,
) -> Result<Value, String> {
    let workspace_id = current_workspace_id();
    let message = request.message.trim().to_string();
    if message.is_empty() {
        return Err("message is required".to_string());
    }

    let run_uuid = request
        .run_id
        .as_deref()
        .map(|value| Uuid::parse_str(value.trim()).map_err(|_| "run_id must be a UUID".to_string()))
        .transpose()?
        .unwrap_or_else(Uuid::new_v4);
    let conversation_uuid = with_conn_mut(&db, |conn| {
        resolve_conversation_in_conn(conn, workspace_id, request.session_id.as_deref(), &message)
    })?;
    let conversation_id = conversation_uuid.to_string();
    let run_id = run_uuid.to_string();

    let route = classify_desktop_intent(&message);
    let packet = build_context_packet(db.clone(), conversation_id.clone(), message.clone())?;
    let mut packet_value = serde_json::to_value(&packet).map_err(|err| err.to_string())?;
    packet_value["desktop_route"] = route_to_json(&route);
    with_conn_mut(&db, |conn| {
        start_agent_run_in_conn(conn, workspace_id, conversation_uuid, run_uuid, &message)
    })?;

    if route.unavailable_capability.is_some() {
        let response =
            build_capability_unavailable_response_json(&run_id, &conversation_id, &route);
        let answer = response["answer"]
            .as_str()
            .unwrap_or("Local capability unavailable.");
        append_local_message(
            &db,
            workspace_id,
            &conversation_id,
            "agent",
            answer,
            Some(response.to_string()),
        )?;
        return Ok(response);
    }

    let llm_config = load_local_llm_config(&db, &workspace_id)?;
    validate_local_llm_config(&llm_config)?;
    let prompt = build_desktop_context_prompt(&packet_value);
    let answer_result = stream_llm_answer(
        &app,
        &agent_state,
        "desktop-agent-delta",
        &run_id,
        &llm_config,
        &prompt,
        &message,
    )
    .await;
    let streamed_answer = match answer_result {
        Ok(answer) => answer,
        Err(err) => {
            clear_cancelled_run(&agent_state, &run_id);
            return Err(err);
        }
    };
    let answer = assistant_content_for_stream_result(&streamed_answer);
    let mut response = build_agent_response_json(
        &run_id,
        &conversation_id,
        &answer,
        &llm_config,
        &packet_value,
        &route,
    );
    if streamed_answer.stopped {
        response["status"] = Value::String("cancelled".to_string());
        response["workflow"]["state"] = Value::String("cancelled".to_string());
        response["workflow"]["summary"] =
            Value::String("desktop local generation stopped by user".to_string());
    }
    append_local_message(
        &db,
        &workspace_id,
        &conversation_id,
        "agent",
        &answer,
        Some(response.to_string()),
    )?;
    if !streamed_answer.stopped {
        let _ = summarize_conversation_for_user(
            &app,
            &db,
            &agent_state,
            &workspace_id,
            &conversation_id,
            None,
            &run_id,
            &llm_config,
        )
        .await;
    }
    clear_cancelled_run(&agent_state, &run_id);
    Ok(response)
}

#[tauri::command]
pub async fn desktop_llm_ask(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: LocalAskRequest,
) -> Result<String, String> {
    let workspace_id = current_workspace_id();
    let query = request.query.trim().to_string();
    if query.is_empty() {
        return Err("query is required".to_string());
    }
    let llm_config = load_local_llm_config(&db, &workspace_id)?;
    validate_local_llm_config(&llm_config)?;
    let mode = request.mode.unwrap_or_else(|| "literature".to_string());
    let prompt = build_local_ask_prompt(&query, &request.contexts, &mode);
    let run_id = request
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let answer = stream_llm_answer(
        &app,
        &agent_state,
        "desktop-ask-delta",
        &run_id,
        &llm_config,
        &prompt,
        &query,
    )
    .await;
    clear_cancelled_run(&agent_state, &run_id);
    answer.map(|answer| assistant_content_for_stream_result(&answer))
}

#[tauri::command]
pub async fn desktop_summarize_conversation(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: SummarizeConversationRequest,
) -> Result<SummarizeConversationResponse, String> {
    let workspace_id = current_workspace_id();
    let conversation_id = request.conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        return Err("conversation_id is required".to_string());
    }

    let llm_config = load_local_llm_config(&db, &workspace_id)?;
    validate_local_llm_config(&llm_config)?;
    let run_id = request
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    summarize_conversation_for_user(
        &app,
        &db,
        &agent_state,
        &workspace_id,
        &conversation_id,
        request.covered_message_id.as_deref(),
        &run_id,
        &llm_config,
    )
    .await
}

fn load_local_llm_config(
    db: &tauri::State<DbState>,
    workspace_id: &str,
) -> Result<LocalLlmConfig, String> {
    let raw = load_setting(db, workspace_id, LOCAL_LLM_CONFIG_KEY)?;
    let Some(raw) = raw else {
        return Ok(LocalLlmConfig::default());
    };
    serde_json::from_str::<LocalLlmConfig>(&raw)
        .map_err(|err| format!("parse local llm config failed: {err}"))
}

fn validate_local_llm_config(config: &LocalLlmConfig) -> Result<(), String> {
    provider_profile_from_config(config).map(|_| ())
}

fn provider_profile_from_config(
    config: &LocalLlmConfig,
) -> Result<(ProviderProfile, Option<SecretValue>), String> {
    if config.model_name.trim().is_empty() {
        return Err("请在桌面端个人中心选择或填写模型名称。".to_string());
    }
    let profile = resolve_chat_profile(&config.provider, &config.base_url, &config.model_name)
        .map_err(|error| {
            if config.base_url.trim().is_empty() {
                "请在桌面端个人中心填写模型 Base URL。".to_string()
            } else {
                error
            }
        })?;
    if config.api_key.trim().is_empty() && profile.kind != ProviderKind::Ollama {
        return Err("请在桌面端个人中心重新保存模型配置，并填写 API Key。普通问答现在由本地客户端后端直接调用模型。".to_string());
    }
    let credential = if config.api_key.trim().is_empty() {
        None
    } else {
        Some(SecretValue::new(config.api_key.trim()).map_err(|error| error.to_string())?)
    };
    Ok((profile, credential))
}

fn load_setting(
    db: &tauri::State<DbState>,
    workspace_id: &str,
    key: &str,
) -> Result<Option<String>, String> {
    with_conn(db, |conn| settings::get(conn, workspace_id, key))
}
fn resolve_conversation_in_conn(
    conn: &mut rusqlite::Connection,
    workspace_id: &str,
    session_id: Option<&str>,
    first_message: &str,
) -> Result<Uuid, String> {
    let mut session = SessionService::new(conn, workspace_id)?;
    if let Some(session_id) = session_id {
        let conversation_id = Uuid::parse_str(session_id.trim())
            .map_err(|_| "session_id must be a UUID".to_string())?;
        session.get_conversation(&conversation_id.to_string())?;
        return Ok(conversation_id);
    }
    let conversation = session.create_conversation(&conversation_title(first_message))?;
    Uuid::parse_str(&conversation.id).map_err(|_| "created conversation id is invalid".to_string())
}

fn start_agent_run_in_conn(
    conn: &mut rusqlite::Connection,
    workspace_id: &str,
    conversation_id: Uuid,
    run_id: Uuid,
    content: &str,
) -> Result<(), String> {
    SessionService::new(conn, workspace_id)?
        .start_run(StartRunRequest {
            conversation_id,
            user_message_id: Uuid::new_v4(),
            run_id,
            event_id: Uuid::new_v4(),
            content: content.to_string(),
            timestamp: Utc::now(),
        })
        .map(|_| ())
}
fn append_local_message(
    db: &tauri::State<DbState>,
    workspace_id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
    response_json: Option<String>,
) -> Result<(), String> {
    with_conn_mut(db, |conn| {
        SessionService::new(conn, workspace_id)?
            .append_message(conversation_id, role, content, response_json)
            .map(|_| ())
    })
}

fn load_summary_state(
    db: &tauri::State<DbState>,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<(Vec<SummaryMessage>, Option<ExistingSummary>), String> {
    with_conn_mut(db, |conn| {
        let snapshot = SessionService::new(conn, workspace_id)?.export_snapshot(conversation_id)?;
        let messages = snapshot
            .messages
            .into_iter()
            .map(|message| SummaryMessage {
                id: message.id,
                role: message.role,
                content: message.content,
                created_at: message.created_at,
            })
            .collect();
        let summary = snapshot.summary.map(|summary| ExistingSummary {
            summary: summary.text,
            covered_message_id: summary.covered_message_id,
        });
        Ok((messages, summary))
    })
}
async fn summarize_conversation_for_user(
    app: &tauri::AppHandle,
    db: &tauri::State<'_, DbState>,
    agent_state: &tauri::State<'_, LocalAgentState>,
    workspace_id: &str,
    conversation_id: &str,
    covered_message_id: Option<&str>,
    run_id: &str,
    llm_config: &LocalLlmConfig,
) -> Result<SummarizeConversationResponse, String> {
    let (mut messages, latest_summary) = load_summary_state(db, workspace_id, conversation_id)?;
    let existing = if covered_message_id.is_none() {
        latest_summary
    } else {
        None
    };
    if covered_message_id.is_none() {
        messages = messages_after_covered_id(
            messages,
            existing
                .as_ref()
                .and_then(|summary| summary.covered_message_id.as_deref()),
        );
    }
    let total_tokens = estimate_summary_tokens(&messages);
    let Some(plan) = plan_summary(messages, covered_message_id)? else {
        return Ok(SummarizeConversationResponse {
            summarized: false,
            summary: None,
            covered_message_id: None,
            total_tokens,
            folded_tokens: 0,
        });
    };

    let (query, contexts) =
        build_summary_prompt(&plan, existing.as_ref().map(|item| item.summary.as_str()));
    let prompt = build_local_ask_prompt(&query, &contexts, "summary");
    let summary_result = stream_llm_answer(
        app,
        agent_state,
        "desktop-ask-delta",
        run_id,
        llm_config,
        &prompt,
        &query,
    )
    .await;
    clear_cancelled_run(agent_state, run_id);
    let summary_answer = summary_result?;
    if summary_answer.stopped {
        return Ok(SummarizeConversationResponse {
            summarized: false,
            summary: None,
            covered_message_id: None,
            total_tokens: plan.total_tokens,
            folded_tokens: plan.folded_tokens,
        });
    }
    let summary = summary_answer.text.trim().to_string();
    if summary.is_empty() {
        return Ok(SummarizeConversationResponse {
            summarized: false,
            summary: None,
            covered_message_id: Some(plan.covered_message_id),
            total_tokens: plan.total_tokens,
            folded_tokens: plan.folded_tokens,
        });
    }
    save_summary_for_user(
        db,
        workspace_id,
        conversation_id,
        &summary,
        Some(plan.covered_message_id.clone()),
    )?;
    Ok(SummarizeConversationResponse {
        summarized: true,
        summary: Some(summary),
        covered_message_id: Some(plan.covered_message_id),
        total_tokens: plan.total_tokens,
        folded_tokens: plan.folded_tokens,
    })
}

fn save_summary_for_user(
    db: &tauri::State<DbState>,
    workspace_id: &str,
    conversation_id: &str,
    summary: &str,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    with_conn_mut(db, |conn| {
        SessionService::new(conn, workspace_id)?.save_summary(
            conversation_id,
            summary,
            covered_message_id,
        )
    })
}
fn conversation_title(message: &str) -> String {
    let title = message.trim().chars().take(28).collect::<String>();
    if title.is_empty() {
        "新对话".to_string()
    } else {
        title
    }
}

fn build_desktop_context_prompt(packet: &Value) -> String {
    let mut sections = vec![
        "你是运行在用户本机的桌面端智能体。普通问答由本机客户端后端主控；优先使用本地上下文、长期记忆、历史对话和会话摘要。".to_string(),
        "回答要直接、专业、可执行；不确定时说明缺口，不要编造来源。".to_string(),
    ];
    push_json_section(
        &mut sections,
        "conversation_summary",
        packet.get("conversation_summary"),
    );
    push_json_section(
        &mut sections,
        "selected_memories",
        packet.get("selected_memories"),
    );
    push_json_section(&mut sections, "history_hits", packet.get("history_hits"));
    push_json_section(
        &mut sections,
        "recent_messages",
        packet.get("recent_messages"),
    );
    push_json_section(&mut sections, "desktop_meta", packet.get("desktop_meta"));
    sections.join("\n\n")
}

fn build_local_ask_prompt(query: &str, contexts: &[String], mode: &str) -> String {
    let mode = mode.trim();
    let compacted_contexts = compact_local_ask_contexts(contexts, mode);
    let mut sections = vec![
        "你是桌面端本地问答后端。你直接调用用户本机配置的模型，不依赖 Web 后端生成回答。".to_string(),
        format!("mode: {mode}"),
        "要求：基于给定上下文回答；没有上下文支撑时要说明不确定；保留钢种、成分、温度、性能、时间、任务 ID 等关键数字。".to_string(),
    ];
    if !compacted_contexts.is_empty() {
        sections.push(format!(
            "contexts_meta: showing {} of {}; char_limit={}",
            compacted_contexts.len(),
            contexts.len(),
            local_ask_context_char_limit(mode)
        ));
        sections.push(
            compacted_contexts
                .iter()
                .enumerate()
                .map(|(index, item)| format!("[context {}]\n{}", index + 1, item))
                .collect::<Vec<_>>()
                .join("\n\n"),
        );
    }
    sections.push(format!("用户问题：\n{}", query.trim()));
    sections.join("\n\n")
}

fn compact_local_ask_contexts(contexts: &[String], mode: &str) -> Vec<String> {
    let limit = if mode.trim() == "summary" {
        LOCAL_SUMMARY_CONTEXT_LIMIT
    } else {
        LOCAL_ASK_CONTEXT_LIMIT
    };
    let char_limit = local_ask_context_char_limit(mode);
    contexts
        .iter()
        .take(limit)
        .map(|item| truncate_text(item, char_limit))
        .collect()
}

fn local_ask_context_char_limit(mode: &str) -> usize {
    if mode.trim() == "summary" {
        LOCAL_SUMMARY_CONTEXT_CHAR_LIMIT
    } else {
        LOCAL_ASK_CONTEXT_CHAR_LIMIT
    }
}

fn push_json_section(sections: &mut Vec<String>, title: &str, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    if value.is_null() || value == "" {
        return;
    }
    if value.as_array().is_some_and(|items| items.is_empty()) {
        return;
    }
    sections.push(format!("{title}:\n{value}"));
}

fn classify_desktop_intent(message: &str) -> DesktopRoute {
    let text = message.trim();
    let lower = text.to_lowercase();
    if text.is_empty() {
        return route(DesktopIntentKind::Clarify, 1.0, "empty message");
    }
    if has_any(
        &lower,
        &[
            "训练",
            "模型训练",
            "retrain",
            "fine tune",
            "finetune",
            "training",
        ],
    ) && has_any(&lower, &["开始", "启动", "重新", "执行", "run", "start"])
    {
        return route(
            DesktopIntentKind::TrainingTask,
            0.9,
            "explicit training task request",
        );
    }
    if has_any(
        &lower,
        &[
            "pdf",
            "文献处理",
            "解析文献",
            "上传文献",
            "入库",
            "literature",
        ],
    ) && has_any(
        &lower,
        &["上传", "解析", "处理", "入库", "process", "upload"],
    ) {
        return route(
            DesktopIntentKind::LiteratureTask,
            0.9,
            "explicit literature processing request",
        );
    }
    if has_any(&lower, &["优化", "寻优", "optimize", "optimization"])
        && has_any(
            &lower,
            &["开始", "启动", "运行", "提交", "执行", "跑", "run", "start"],
        )
    {
        return route(
            DesktopIntentKind::OptimizationTask,
            0.88,
            "explicit optimization task request",
        );
    }
    if has_any(
        &lower,
        &[
            "知识库",
            "文献",
            "论文",
            "检索",
            "标准",
            "资料",
            "引用",
            "出处",
            "rag",
            "search",
        ],
    ) {
        return route(
            DesktopIntentKind::KnowledgeQa,
            0.86,
            "knowledge or citation retrieval requested",
        );
    }
    if has_any(
        &lower,
        &[
            "优化", "工艺", "热轧", "轧制", "屈服", "强度", "成分", "process", "yield", "tensile",
        ],
    ) {
        return route(
            DesktopIntentKind::OptimizationAdvice,
            0.74,
            "steel process advice question",
        );
    }
    route(DesktopIntentKind::LocalQa, 0.7, "default local desktop QA")
}

fn route(intent: DesktopIntentKind, confidence: f32, reason: &'static str) -> DesktopRoute {
    let unavailable_capability = match intent {
        DesktopIntentKind::KnowledgeQa => Some("local_rag"),
        DesktopIntentKind::TrainingTask => Some("local_training"),
        DesktopIntentKind::OptimizationTask => Some("local_optimization"),
        DesktopIntentKind::LiteratureTask => Some("local_literature"),
        _ => None,
    };
    DesktopRoute {
        intent,
        confidence,
        reason,
        unavailable_capability,
    }
}

fn route_to_json(route: &DesktopRoute) -> Value {
    serde_json::json!({
        "intent": route.intent.as_str(),
        "confidence": route.confidence,
        "reason": route.reason,
        "unavailable_capability": route.unavailable_capability,
    })
}
fn has_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn truncate_text(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push('…');
    }
    result
}

async fn stream_llm_answer(
    app: &tauri::AppHandle,
    agent_state: &tauri::State<'_, LocalAgentState>,
    event_name: &str,
    run_id: &str,
    config: &LocalLlmConfig,
    context_prompt: &str,
    user_message: &str,
) -> Result<StreamedLlmAnswer, String> {
    stream_llm_answer_core(
        config,
        context_prompt,
        user_message,
        || is_run_cancelled(agent_state, run_id),
        |delta| {
            let _ = app.emit(
                event_name,
                LocalAgentDelta {
                    run_id: run_id.to_string(),
                    delta: delta.to_string(),
                },
            );
        },
    )
    .await
}

async fn stream_llm_answer_core(
    config: &LocalLlmConfig,
    context_prompt: &str,
    user_message: &str,
    is_cancelled: impl Fn() -> Result<bool, String> + Send + Sync,
    mut on_delta: impl FnMut(&str) + Send,
) -> Result<StreamedLlmAnswer, String> {
    if is_cancelled()? {
        return Err("LLM run cancelled".to_string());
    }
    let (profile, credential) = provider_profile_from_config(config)?;
    let request = ChatRequest::single_turn(context_prompt, user_message);
    let cancellation_error = Mutex::new(None);
    let cancelled = || match is_cancelled() {
        Ok(cancelled) => cancelled,
        Err(error) => {
            if let Ok(mut slot) = cancellation_error.lock() {
                *slot = Some(error);
            }
            true
        }
    };
    let mut on_event = |event| {
        if let ChatEvent::TextDelta(delta) = event {
            on_delta(&delta);
        }
    };
    let provider =
        configured_chat_provider(profile, credential).map_err(|error| error.to_string())?;
    provider
        .capabilities()
        .require(ProviderCapability::Chat)
        .map_err(|error| error.to_string())?;
    let response = provider.chat(request, &mut on_event, &cancelled).await;
    if let Some(error) = cancellation_error
        .lock()
        .map_err(|_| "local agent cancellation state poisoned".to_string())?
        .take()
    {
        return Err(error);
    }
    let response = response.map_err(|error| error.to_string())?;
    Ok(StreamedLlmAnswer {
        text: response.text,
        stopped: response.cancelled,
    })
}

fn build_agent_response_json(
    run_id: &str,
    conversation_id: &str,
    answer: &str,
    config: &LocalLlmConfig,
    _packet: &Value,
    route: &DesktopRoute,
) -> Value {
    serde_json::json!({
        "run_id": run_id,
        "session_id": conversation_id,
        "status": "completed",
        "answer": answer,
        "follow_up_questions": [],
        "plan_steps": [],
        "tool_calls": [],
        "evidence": [],
        "recommendations": [],
        "verification": {
            "confidence": "medium",
            "citation_count": 0,
            "missing_citations": [],
            "numeric_warnings": [],
            "unsupported_claims": [],
            "summary": "Generated locally from the current desktop context."
        },
        "memory": {
            "session_id": conversation_id,
            "notes": []
        },
        "pending_confirmations": [],
        "intent": {
            "intent_type": route.intent.as_str(),
            "domain": "steel",
            "risk_level": "medium",
            "needs_evidence": false,
            "needs_tools": [],
            "unavailable_capability": route.unavailable_capability,
            "missing_slots": [],
            "answer_policy": "desktop_local",
            "reason": route.reason
        },
        "workflow_trace": {
            "route": "desktop_local_agent",
            "tools_selected": [],
            "tools_skipped": [],
            "evidence_policy": "local_context_first",
            "answer_policy": "desktop_local",
            "model_provider": config.provider,
            "model_name": config.model_name,
            "notes": ["Context is assembled from local SQLite data."]
        },
        "workflow": {
            "run_id": run_id,
            "state": "completed",
            "nodes": [],
            "edges": [],
            "events": [],
            "summary": "desktop local agent",
            "started_at": null,
            "ended_at": null
        },
        "llm_config": {
            "provider": config.provider,
            "base_url": config.base_url,
            "model_name": config.model_name,
            "has_api_key": !config.api_key.trim().is_empty()
        }
    })
}

fn build_capability_unavailable_response_json(
    run_id: &str,
    conversation_id: &str,
    route: &DesktopRoute,
) -> Value {
    let capability = route.unavailable_capability.unwrap_or("unknown");
    serde_json::json!({
        "run_id": run_id,
        "session_id": conversation_id,
        "status": "capability_unavailable",
        "answer": format!("Local capability is not available yet: {capability}."),
        "follow_up_questions": [],
        "plan_steps": [],
        "tool_calls": [],
        "evidence": [],
        "recommendations": [],
        "verification": {
            "confidence": "high",
            "citation_count": 0,
            "missing_citations": [],
            "numeric_warnings": [],
            "unsupported_claims": [],
            "summary": "The requested local capability is unavailable."
        },
        "memory": {
            "session_id": conversation_id,
            "notes": []
        },
        "pending_confirmations": [],
        "intent": {
            "intent_type": route.intent.as_str(),
            "domain": "steel",
            "risk_level": "low",
            "needs_evidence": false,
            "needs_tools": [],
            "unavailable_capability": capability,
            "missing_slots": [],
            "answer_policy": "capability_unavailable",
            "reason": route.reason
        },
        "workflow_trace": {
            "route": "desktop_local_agent",
            "tools_selected": [],
            "tools_skipped": [capability],
            "evidence_policy": "none",
            "answer_policy": "capability_unavailable",
            "model_provider": "",
            "model_name": "",
            "notes": []
        },
        "workflow": {
            "run_id": run_id,
            "state": "blocked",
            "nodes": [],
            "edges": [],
            "events": [],
            "summary": "local capability unavailable",
            "started_at": null,
            "ended_at": null
        },
        "llm_config": {
            "provider": "",
            "base_url": "",
            "model_name": "",
            "has_api_key": false
        }
    })
}
fn is_run_cancelled(
    state: &tauri::State<'_, LocalAgentState>,
    run_id: &str,
) -> Result<bool, String> {
    let cancelled = state
        .cancelled_runs
        .lock()
        .map_err(|_| "local agent state poisoned")?;
    Ok(cancelled.contains(run_id))
}

fn clear_cancelled_run(state: &tauri::State<'_, LocalAgentState>, run_id: &str) {
    if let Ok(mut cancelled) = state.cancelled_runs.lock() {
        cancelled.remove(run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn row_count(conn: &rusqlite::Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count rows")
    }

    #[test]
    fn local_agent_start_helper_atomically_persists_message_run_and_first_event() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("sqlite");
        crate::storage::migrations::migrate(&mut conn).expect("migrate schema");
        let conversation = crate::agent::session::service::SessionService::new(&mut conn, "local")
            .unwrap()
            .create_conversation("atomic local agent")
            .unwrap();
        let conversation_id = Uuid::parse_str(&conversation.id).unwrap();
        let run_id = Uuid::new_v4();
        conn.execute_batch(
            "CREATE TRIGGER fail_local_agent_first_event
             BEFORE INSERT ON agent_run_events
             BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;",
        )
        .unwrap();

        let error = start_agent_run_in_conn(
            &mut conn,
            "local",
            conversation_id,
            run_id,
            "atomic request",
        )
        .expect_err("event failure must roll back the local agent start");
        assert!(error.contains("agent_event_storage_failed"));
        assert_eq!(row_count(&conn, "messages"), 0);
        assert_eq!(row_count(&conn, "agent_runs"), 0);
        assert_eq!(row_count(&conn, "agent_run_events"), 0);

        conn.execute_batch("DROP TRIGGER fail_local_agent_first_event")
            .unwrap();
        start_agent_run_in_conn(
            &mut conn,
            "local",
            conversation_id,
            run_id,
            "atomic request",
        )
        .expect("persist local agent start");

        assert_eq!(row_count(&conn, "messages"), 1);
        assert_eq!(row_count(&conn, "agent_runs"), 1);
        assert_eq!(row_count(&conn, "agent_run_events"), 1);
        let persisted_run_id: String = conn
            .query_row("SELECT id FROM agent_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(persisted_run_id, run_id.to_string());
    }

    #[test]
    fn ollama_config_uses_local_default_without_api_key() {
        let config = LocalLlmConfig {
            provider: "ollama".to_string(),
            base_url: String::new(),
            model_name: "qwen3".to_string(),
            api_key: String::new(),
        };

        validate_local_llm_config(&config).expect("valid Ollama config");
        let (profile, credential) = provider_profile_from_config(&config).expect("profile");

        assert_eq!(profile.kind, ProviderKind::Ollama);
        assert_eq!(profile.base_url, "http://127.0.0.1:11434");
        assert!(credential.is_none());
    }

    #[test]
    fn stream_llm_answer_reads_openai_compatible_sse_from_local_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock llm");
        let addr = listener.local_addr().expect("mock llm addr");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept mock llm request");
            let request = read_http_request(&mut stream);
            let request_text = String::from_utf8_lossy(&request);
            let lower_request = request_text.to_ascii_lowercase();
            assert!(request_text.starts_with("POST /v1/chat/completions "));
            assert!(lower_request.contains("authorization: bearer sk-test"));
            assert!(request_text.contains(r#""model":"mock-model""#));
            assert!(request_text.contains(r#""stream":true"#));

            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"hello \"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write mock llm response");
        });
        let config = LocalLlmConfig {
            provider: "mock".to_string(),
            base_url: format!("http://{addr}/v1"),
            model_name: "mock-model".to_string(),
            api_key: "sk-test".to_string(),
        };
        let mut deltas = Vec::new();

        let answer = tauri::async_runtime::block_on(stream_llm_answer_core(
            &config,
            "system prompt",
            "user prompt",
            || Ok(false),
            |delta| deltas.push(delta.to_string()),
        ))
        .expect("stream answer");

        assert_eq!(answer.text, "hello world");
        assert!(!answer.stopped);
        assert_eq!(deltas, vec!["hello ".to_string(), "world".to_string()]);
        server.join().expect("mock llm server");
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        let header_end = loop {
            let read = stream.read(&mut buffer).expect("read mock request");
            assert_ne!(read, 0, "mock request ended before headers");
            request.extend_from_slice(&buffer[..read]);
            if let Some(index) = request.windows(4).position(|item| item == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let read = stream.read(&mut buffer).expect("read mock body");
            assert_ne!(read, 0, "mock request ended before body");
            request.extend_from_slice(&buffer[..read]);
        }
        request
    }

    #[test]
    fn local_prompt_includes_desktop_context() {
        let packet = serde_json::json!({
            "conversation_summary": "用户长期关注热轧屈服波动。",
            "selected_memories": [{"title": "客户偏好", "body": "回答要先给结论。"}],
            "history_hits": [{"conversation_title": "历史讨论", "content": "上次提到 Q355B。"}],
            "recent_messages": [{"role": "user", "content": "最近问过库存。"}],
            "desktop_meta": {"budget_meta": {"estimated_context_tokens": 1234}}
        });

        let prompt = build_desktop_context_prompt(&packet);

        assert!(prompt.contains("用户长期关注热轧屈服波动。"));
        assert!(prompt.contains("回答要先给结论。"));
        assert!(prompt.contains("上次提到 Q355B。"));
        assert!(prompt.contains("estimated_context_tokens"));
    }

    #[test]
    fn domain_requests_report_local_capability_gaps() {
        for (input, capability) in [
            ("search Q355B literature", "local_rag"),
            ("start training model", "local_training"),
            ("run optimization process", "local_optimization"),
            ("upload PDF and process literature", "local_literature"),
        ] {
            let route = classify_desktop_intent(input);
            assert_eq!(
                route.unavailable_capability,
                Some(capability),
                "unexpected route for {input}"
            );
        }
    }

    #[test]
    fn unavailable_local_capability_returns_structured_response() {
        let route = classify_desktop_intent("search Q355B literature");
        let response =
            build_capability_unavailable_response_json("run-1", "conversation-1", &route);

        assert_eq!(response["status"], "capability_unavailable");
        assert_eq!(response["intent"]["unavailable_capability"], "local_rag");
        assert_eq!(response["pending_confirmations"], serde_json::json!([]));
        assert_eq!(
            response["workflow_trace"]["tools_selected"],
            serde_json::json!([])
        );
    }

    #[test]
    fn classifies_desktop_agent_intents() {
        assert_eq!(
            classify_desktop_intent("总结一下这个客户之前的偏好").intent,
            DesktopIntentKind::LocalQa
        );
        assert_eq!(
            classify_desktop_intent("查一下知识库里 Q355B 热轧标准").intent,
            DesktopIntentKind::KnowledgeQa
        );
        assert_eq!(
            classify_desktop_intent("这批钢屈服偏低，工艺上怎么优化？").intent,
            DesktopIntentKind::OptimizationAdvice
        );
        assert_eq!(
            classify_desktop_intent("按当前数据启动工艺优化").intent,
            DesktopIntentKind::OptimizationTask
        );
        assert_eq!(
            classify_desktop_intent("开始重新训练模型 v2").intent,
            DesktopIntentKind::TrainingTask
        );
        assert_eq!(
            classify_desktop_intent("上传这篇 PDF 并解析入库").intent,
            DesktopIntentKind::LiteratureTask
        );
    }

    #[test]
    fn local_ask_prompt_includes_contexts() {
        let prompt = build_local_ask_prompt(
            "请总结这些生产数据",
            &["Q355B 屈服 420MPa".to_string(), "热轧温度 880C".to_string()],
            "advice",
        );

        assert!(prompt.contains("Q355B 屈服 420MPa"));
        assert!(prompt.contains("热轧温度 880C"));
        assert!(prompt.contains("advice"));
    }

    #[test]
    fn local_ask_prompt_bounds_frontend_contexts() {
        let long_context = "x".repeat(LOCAL_ASK_CONTEXT_CHAR_LIMIT + 40);
        let contexts = (0..20)
            .map(|index| format!("context-{index}-{long_context}"))
            .collect::<Vec<_>>();

        let prompt = build_local_ask_prompt("answer with bounded contexts", &contexts, "advice");

        assert!(prompt.contains("contexts_meta: showing 12 of 20"));
        assert!(prompt.contains("context-0"));
        assert!(prompt.contains("context-11"));
        assert!(!prompt.contains("context-12"));
        assert!(prompt.contains('…'));
    }

    #[test]
    fn local_summary_prompt_keeps_larger_context_budget() {
        let contexts = (0..20)
            .map(|index| format!("summary-context-{index}"))
            .collect::<Vec<_>>();

        let prompt = build_local_ask_prompt("summarize", &contexts, "summary");

        assert!(prompt.contains("contexts_meta: showing 20 of 20"));
        assert!(prompt.contains("summary-context-19"));
    }

    #[test]
    fn plans_automatic_summary_when_history_exceeds_budget() {
        let messages = (0..20)
            .map(|index| SummaryMessage {
                id: format!("m-{index}"),
                role: "user".to_string(),
                content: "Q355B hot rolling 880C yield 420MPa ".repeat(120),
                created_at: format!("t-{index}"),
            })
            .collect::<Vec<_>>();

        let plan = plan_summary(messages, None)
            .expect("plan")
            .expect("some plan");

        assert!(plan.total_tokens >= SUMMARY_TRIGGER_TOKENS);
        assert!(plan.folded_tokens >= SUMMARY_MIN_FOLD_TOKENS);
        assert!(plan.older_messages.len() < 20);
    }

    #[test]
    fn plans_manual_summary_to_covered_message() {
        let messages = (0..5)
            .map(|index| SummaryMessage {
                id: format!("m-{index}"),
                role: "user".to_string(),
                content: format!("message {index}"),
                created_at: format!("t-{index}"),
            })
            .collect::<Vec<_>>();

        let plan = plan_summary(messages, Some("m-2"))
            .expect("plan")
            .expect("some plan");

        assert_eq!(plan.covered_message_id, "m-2");
        assert_eq!(plan.older_messages.len(), 3);
    }

    #[test]
    fn auto_summary_skips_messages_already_covered() {
        let messages = (0..5)
            .map(|index| SummaryMessage {
                id: format!("m-{index}"),
                role: "user".to_string(),
                content: format!("message {index}"),
                created_at: format!("t-{index}"),
            })
            .collect::<Vec<_>>();

        let remaining = messages_after_covered_id(messages, Some("m-2"));

        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].id, "m-3");
        assert_eq!(remaining[1].id, "m-4");
    }

    #[test]
    fn summary_prompt_keeps_existing_summary_context() {
        let plan = SummaryPlan {
            older_messages: vec![SummaryMessage {
                id: "m-1".to_string(),
                role: "user".to_string(),
                content: "new facts".to_string(),
                created_at: "t-1".to_string(),
            }],
            covered_message_id: "m-1".to_string(),
            total_tokens: 100,
            folded_tokens: 80,
        };

        let (_query, contexts) = build_summary_prompt(&plan, Some("old summary facts"));

        assert!(contexts[0].contains("existing conversation summary"));
        assert!(contexts[0].contains("old summary facts"));
        assert!(contexts[1].contains("new facts"));
    }

    #[test]
    fn stopped_stream_answer_is_marked_as_partial() {
        let answer = assistant_content_for_stream_result(&StreamedLlmAnswer {
            text: "partial answer".to_string(),
            stopped: true,
        });

        assert!(answer.contains("partial answer"));
        assert!(answer.contains("已停止生成"));
    }

    #[test]
    fn completed_stream_answer_is_not_changed() {
        let answer = assistant_content_for_stream_result(&StreamedLlmAnswer {
            text: "complete answer".to_string(),
            stopped: false,
        });

        assert_eq!(answer, "complete answer");
    }
}
