use crate::auth::AuthState;
use crate::context::build_context_packet;
use crate::db::{current_user_id, now, upsert_cloud_job_for_user, with_conn, DbState};
use crate::models::CloudJobInput;
use crate::retrieval::estimate_text_tokens;
use chrono::Utc;
use futures_util::StreamExt;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::HashSet, sync::Mutex};
use tauri::Emitter;
use uuid::Uuid;

const LOCAL_LLM_CONFIG_KEY: &str = "local_llm_config";
const CLOUD_API_BASE_KEY: &str = "cloud_api_base";
const CLOUD_API_BASE_NOT_CONFIGURED: &str =
    "云端服务未配置：请在设置页填写云端 API 地址后再使用云任务功能";
const SUMMARY_TRIGGER_TOKENS: usize = 9000;
const SUMMARY_KEEP_TAIL_TOKENS: usize = 3200;
const SUMMARY_MIN_FOLD_TOKENS: usize = 1200;
const CLOUD_KNOWLEDGE_RESULT_LIMIT: usize = 5;
const CLOUD_KNOWLEDGE_STRING_CHAR_LIMIT: usize = 1200;
const CLOUD_KNOWLEDGE_NESTED_ARRAY_LIMIT: usize = 8;
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
pub struct ConfirmCloudJobRequest {
    pub conversation_id: String,
    pub action_id: String,
    pub task_type: String,
    pub message: String,
    pub approved: bool,
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

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatCompletionMessage>,
    stream: bool,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct ChatCompletionMessage {
    role: String,
    content: String,
}

#[derive(Clone, Debug)]
struct SummaryMessage {
    id: String,
    role: String,
    content: String,
    created_at: String,
}

struct SummaryPlan {
    older_messages: Vec<SummaryMessage>,
    covered_message_id: String,
    total_tokens: usize,
    folded_tokens: usize,
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
    needs_cloud_search: bool,
    needs_cloud_job: bool,
    task_type: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfirmedCloudTask {
    Training,
    Optimization,
    Literature,
}

impl ConfirmedCloudTask {
    fn as_str(self) -> &'static str {
        match self {
            ConfirmedCloudTask::Training => "training",
            ConfirmedCloudTask::Optimization => "optimization",
            ConfirmedCloudTask::Literature => "literature",
        }
    }
}

struct CloudJobOutcome {
    job_id: String,
    status: String,
    answer: String,
}

#[tauri::command]
pub async fn desktop_agent_chat(
    app: tauri::AppHandle,
    auth: tauri::State<'_, AuthState>,
    db: tauri::State<'_, DbState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: LocalAgentChatRequest,
) -> Result<Value, String> {
    let session = auth.current_session()?;
    let user_id = current_user_id(&auth)?;
    let message = request.message.trim().to_string();
    if message.is_empty() {
        return Err("message is required".to_string());
    }

    let conversation_id = request
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let run_id = request
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    ensure_conversation(&db, &user_id, &conversation_id, &message)?;
    let route = classify_desktop_intent(&message);
    let packet = build_context_packet(auth, db.clone(), conversation_id.clone(), message.clone())?;
    let mut packet_value = serde_json::to_value(&packet).map_err(|err| err.to_string())?;
    packet_value["desktop_route"] = route_to_json(&route);

    if route.needs_cloud_job {
        append_local_message(&db, &user_id, &conversation_id, "user", &message, None)?;
        let llm_config = load_local_llm_config(&db, &user_id).unwrap_or_default();
        let response = build_cloud_job_confirmation_response_json(
            &run_id,
            &conversation_id,
            &llm_config,
            &packet_value,
            &route,
            &message,
        );
        let answer = response
            .get("answer")
            .and_then(Value::as_str)
            .unwrap_or("Cloud task confirmation required.");
        append_local_message(
            &db,
            &user_id,
            &conversation_id,
            "agent",
            answer,
            Some(response.to_string()),
        )?;
        return Ok(response);
    }

    append_local_message(&db, &user_id, &conversation_id, "user", &message, None)?;
    if route.needs_cloud_search {
        if let Some(cloud_contexts) =
            fetch_cloud_knowledge(&db, &user_id, session.token.as_str(), &message).await?
        {
            packet_value["cloud_knowledge"] = cloud_contexts;
        }
    }

    let llm_config = load_local_llm_config(&db, &user_id)?;
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
        &user_id,
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
            &user_id,
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
pub async fn desktop_confirm_cloud_job(
    auth: tauri::State<'_, AuthState>,
    db: tauri::State<'_, DbState>,
    request: ConfirmCloudJobRequest,
) -> Result<Value, String> {
    let session = auth.current_session()?;
    let user_id = current_user_id(&auth)?;
    let conversation_id = request.conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        return Err("conversation_id is required".to_string());
    }
    let action_id = request.action_id.trim().to_string();
    if action_id.is_empty() {
        return Err("action_id is required".to_string());
    }
    let task = normalize_cloud_task_type(&request.task_type)?;
    let run_id = format!("confirm-{}", Uuid::new_v4());

    if !request.approved {
        let answer = format!("Cancelled cloud task request: {}.", task.as_str());
        let response = build_cloud_job_result_response_json(
            &run_id,
            &conversation_id,
            "cancelled",
            &answer,
            task.as_str(),
            None,
        );
        append_local_message(
            &db,
            &user_id,
            &conversation_id,
            "agent",
            &answer,
            Some(response.to_string()),
        )?;
        return Ok(response);
    }

    let outcome = match task {
        ConfirmedCloudTask::Training => {
            start_confirmed_training_job(&db, &user_id, session.token.as_str(), &request).await?
        }
        ConfirmedCloudTask::Optimization => {
            if let Some(payload) = structured_cloud_task_payload(task, &request.message) {
                start_confirmed_json_cloud_job(
                    &db,
                    &user_id,
                    session.token.as_str(),
                    &request,
                    task,
                    "api/optimize",
                    payload,
                )
                .await?
            } else {
                save_needs_input_cloud_job(&db, &user_id, &conversation_id, &request, task)?
            }
        }
        ConfirmedCloudTask::Literature => {
            if let Some(payload) = structured_cloud_task_payload(task, &request.message) {
                start_confirmed_json_cloud_job(
                    &db,
                    &user_id,
                    session.token.as_str(),
                    &request,
                    task,
                    "api/literature/process",
                    payload,
                )
                .await?
            } else {
                save_needs_input_cloud_job(&db, &user_id, &conversation_id, &request, task)?
            }
        }
    };
    let response = build_cloud_job_result_response_json(
        &run_id,
        &conversation_id,
        &outcome.status,
        &outcome.answer,
        task.as_str(),
        Some(&outcome.job_id),
    );
    append_local_message(
        &db,
        &user_id,
        &conversation_id,
        "agent",
        &outcome.answer,
        Some(response.to_string()),
    )?;
    Ok(response)
}

#[tauri::command]
pub async fn desktop_llm_ask(
    app: tauri::AppHandle,
    auth: tauri::State<'_, AuthState>,
    db: tauri::State<'_, DbState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: LocalAskRequest,
) -> Result<String, String> {
    let user_id = current_user_id(&auth)?;
    let query = request.query.trim().to_string();
    if query.is_empty() {
        return Err("query is required".to_string());
    }
    let llm_config = load_local_llm_config(&db, &user_id)?;
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
    auth: tauri::State<'_, AuthState>,
    db: tauri::State<'_, DbState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: SummarizeConversationRequest,
) -> Result<SummarizeConversationResponse, String> {
    let user_id = current_user_id(&auth)?;
    let conversation_id = request.conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        return Err("conversation_id is required".to_string());
    }

    let llm_config = load_local_llm_config(&db, &user_id)?;
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
        &user_id,
        &conversation_id,
        request.covered_message_id.as_deref(),
        &run_id,
        &llm_config,
    )
    .await
}

fn load_local_llm_config(
    db: &tauri::State<DbState>,
    user_id: &str,
) -> Result<LocalLlmConfig, String> {
    let raw = load_setting(db, user_id, LOCAL_LLM_CONFIG_KEY)?;
    let Some(raw) = raw else {
        return Ok(LocalLlmConfig::default());
    };
    serde_json::from_str::<LocalLlmConfig>(&raw)
        .map_err(|err| format!("parse local llm config failed: {err}"))
}

fn validate_local_llm_config(config: &LocalLlmConfig) -> Result<(), String> {
    if config.api_key.trim().is_empty() {
        return Err("请在桌面端个人中心重新保存模型配置，并填写 API Key。普通问答现在由本地客户端后端直接调用模型。".to_string());
    }
    if config.model_name.trim().is_empty() {
        return Err("请在桌面端个人中心选择或填写模型名称。".to_string());
    }
    if config.base_url.trim().is_empty() && default_base_url(&config.provider).is_empty() {
        return Err("请在桌面端个人中心填写模型 Base URL。".to_string());
    }
    Ok(())
}

fn load_setting(
    db: &tauri::State<DbState>,
    user_id: &str,
    key: &str,
) -> Result<Option<String>, String> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT value_json FROM settings WHERE user_id = ?1 AND key = ?2",
            params![user_id, key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())
    })
}

fn ensure_conversation(
    db: &tauri::State<DbState>,
    user_id: &str,
    conversation_id: &str,
    first_message: &str,
) -> Result<(), String> {
    let ts = now();
    with_conn(db, |conn| {
        ensure_conversation_in_conn(conn, user_id, conversation_id, first_message, &ts)
    })
}

fn ensure_conversation_in_conn(
    conn: &rusqlite::Connection,
    user_id: &str,
    conversation_id: &str,
    first_message: &str,
    ts: &str,
) -> Result<(), String> {
    let owner = conn
        .query_row(
            "SELECT user_id FROM conversations WHERE id = ?1",
            params![conversation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    if let Some(owner) = owner {
        if owner == user_id {
            return Ok(());
        }
        return Err("conversation does not belong to current user".to_string());
    }

    let title = conversation_title(first_message);
    conn.execute(
        "INSERT INTO conversations (id, user_id, title, created_at, updated_at, pinned, archived)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
        params![conversation_id, user_id, title, ts, ts],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn append_local_message(
    db: &tauri::State<DbState>,
    user_id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
    response_json: Option<String>,
) -> Result<(), String> {
    let ts = now();
    with_conn(db, |conn| {
        append_local_message_to_conn(
            conn,
            user_id,
            conversation_id,
            role,
            content,
            response_json,
            &ts,
        )
    })
}

fn append_local_message_to_conn(
    conn: &rusqlite::Connection,
    user_id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
    response_json: Option<String>,
    ts: &str,
) -> Result<(), String> {
    let belongs = conn
        .query_row(
            "SELECT 1 FROM conversations WHERE user_id = ?1 AND id = ?2",
            params![user_id, conversation_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|err| err.to_string())?
        .is_some();
    if !belongs {
        return Err("conversation does not belong to current user".to_string());
    }
    conn.execute(
        "INSERT INTO messages (id, user_id, conversation_id, role, content, response_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![Uuid::new_v4().to_string(), user_id, conversation_id, role, content, response_json, ts],
    )
    .map_err(|err| err.to_string())?;
    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE user_id = ?2 AND id = ?3",
        params![ts, user_id, conversation_id],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn load_summary_messages(
    db: &tauri::State<DbState>,
    user_id: &str,
    conversation_id: &str,
) -> Result<Vec<SummaryMessage>, String> {
    with_conn(db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, role, content, created_at
                 FROM messages
                 WHERE user_id = ?1 AND conversation_id = ?2
                 ORDER BY created_at ASC",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params![user_id, conversation_id], |row| {
                Ok(SummaryMessage {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|err| err.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())
    })
}

fn load_latest_summary(
    db: &tauri::State<DbState>,
    user_id: &str,
    conversation_id: &str,
) -> Result<Option<ExistingSummary>, String> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT summary, covered_message_id
             FROM conversation_summaries
             WHERE user_id = ?1 AND conversation_id = ?2
             ORDER BY updated_at DESC
             LIMIT 1",
            params![user_id, conversation_id],
            |row| {
                Ok(ExistingSummary {
                    summary: row.get(0)?,
                    covered_message_id: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|err| err.to_string())
    })
}

fn messages_after_covered_id(
    messages: Vec<SummaryMessage>,
    covered_message_id: Option<&str>,
) -> Vec<SummaryMessage> {
    let Some(covered_message_id) = covered_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return messages;
    };
    let Some(index) = messages
        .iter()
        .position(|message| message.id == covered_message_id)
    else {
        return messages;
    };
    messages.into_iter().skip(index + 1).collect()
}

fn plan_summary(
    messages: Vec<SummaryMessage>,
    covered_message_id: Option<&str>,
) -> Result<Option<SummaryPlan>, String> {
    if messages.is_empty() {
        return Ok(None);
    }
    let total_tokens = messages
        .iter()
        .map(estimate_summary_message_tokens)
        .sum::<usize>();
    if let Some(anchor_id) = covered_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let index = messages
            .iter()
            .position(|message| message.id == anchor_id)
            .ok_or_else(|| "covered message not found".to_string())?;
        let older_messages = messages[..=index].to_vec();
        let folded_tokens = older_messages
            .iter()
            .map(estimate_summary_message_tokens)
            .sum::<usize>();
        return Ok(Some(SummaryPlan {
            older_messages,
            covered_message_id: anchor_id.to_string(),
            total_tokens: folded_tokens,
            folded_tokens,
        }));
    }
    if total_tokens < SUMMARY_TRIGGER_TOKENS {
        return Ok(None);
    }

    let mut tail_tokens = 0usize;
    let mut split_index = messages.len();
    for index in (0..messages.len()).rev() {
        let cost = estimate_summary_message_tokens(&messages[index]);
        if tail_tokens >= SUMMARY_KEEP_TAIL_TOKENS && messages.len() - index >= 8 {
            split_index = index + 1;
            break;
        }
        tail_tokens += cost;
        split_index = index;
    }

    let older_messages = messages[..split_index].to_vec();
    let folded_tokens = older_messages
        .iter()
        .map(estimate_summary_message_tokens)
        .sum::<usize>();
    let Some(covered_message_id) = older_messages.last().map(|message| message.id.clone()) else {
        return Ok(None);
    };
    if folded_tokens < SUMMARY_MIN_FOLD_TOKENS {
        return Ok(None);
    }
    Ok(Some(SummaryPlan {
        older_messages,
        covered_message_id,
        total_tokens,
        folded_tokens,
    }))
}

async fn summarize_conversation_for_user(
    app: &tauri::AppHandle,
    db: &tauri::State<'_, DbState>,
    agent_state: &tauri::State<'_, LocalAgentState>,
    user_id: &str,
    conversation_id: &str,
    covered_message_id: Option<&str>,
    run_id: &str,
    llm_config: &LocalLlmConfig,
) -> Result<SummarizeConversationResponse, String> {
    let existing = if covered_message_id.is_none() {
        load_latest_summary(db, user_id, conversation_id)?
    } else {
        None
    };
    let mut messages = load_summary_messages(db, user_id, conversation_id)?;
    if covered_message_id.is_none() {
        messages = messages_after_covered_id(
            messages,
            existing
                .as_ref()
                .and_then(|summary| summary.covered_message_id.as_deref()),
        );
    }
    let total_tokens = messages.iter().map(estimate_summary_message_tokens).sum();
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
        user_id,
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

fn estimate_summary_message_tokens(message: &SummaryMessage) -> usize {
    estimate_text_tokens(&message.role) + estimate_text_tokens(&message.content) + 4
}

fn build_summary_prompt(
    plan: &SummaryPlan,
    existing_summary: Option<&str>,
) -> (String, Vec<String>) {
    let mut contexts = existing_summary
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|summary| vec![format!("[existing conversation summary]\n{summary}")])
        .unwrap_or_default();
    contexts.extend(
        plan.older_messages
            .iter()
            .map(|item| {
                format!(
                    "[{} id={} at={}]\n{}",
                    item.role, item.id, item.created_at, item.content
                )
            })
            .collect::<Vec<_>>(),
    );
    let query = vec![
        "请把这些较早的桌面端智能体对话压缩成事实性摘要。".to_string(),
        "必须使用以下小标题；没有内容的小标题可以省略：".to_string(),
        "## 用户长期要求与硬约束".to_string(),
        "## 当前研究或生产目标".to_string(),
        "## 材料体系、钢种、成分与工艺参数".to_string(),
        "## 已讨论结论与关键假设".to_string(),
        "## 引用过的文献、数据来源或云端结果".to_string(),
        "## 已失败或被否定的方向".to_string(),
        "## 待解决问题与下一步".to_string(),
        "规则：保留具体数字、成分、温度、时间、钢种、文件名、任务 ID 和用户明确偏好；不要编造未知事实；使用短句和项目符号。".to_string(),
        format!("本次折叠约 {} tokens，总历史约 {} tokens。", plan.folded_tokens, plan.total_tokens),
    ]
    .join("\n");
    (query, contexts)
}

fn save_summary_for_user(
    db: &tauri::State<DbState>,
    user_id: &str,
    conversation_id: &str,
    summary: &str,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    let id = Uuid::new_v4().to_string();
    let ts = now();
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO conversation_summaries (id, user_id, conversation_id, summary, covered_message_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, user_id, conversation_id, summary, covered_message_id, ts, ts],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
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
    push_json_section(
        &mut sections,
        "cloud_knowledge",
        packet.get("cloud_knowledge"),
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
        return route(
            DesktopIntentKind::Clarify,
            1.0,
            "empty message",
            false,
            false,
            None,
        );
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
            false,
            true,
            Some("training"),
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
            false,
            true,
            Some("literature"),
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
            false,
            true,
            Some("optimization"),
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
            true,
            false,
            None,
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
            false,
            false,
            None,
        );
    }
    route(
        DesktopIntentKind::LocalQa,
        0.7,
        "default local desktop QA",
        false,
        false,
        None,
    )
}

fn route(
    intent: DesktopIntentKind,
    confidence: f32,
    reason: &'static str,
    needs_cloud_search: bool,
    needs_cloud_job: bool,
    task_type: Option<&'static str>,
) -> DesktopRoute {
    DesktopRoute {
        intent,
        confidence,
        reason,
        needs_cloud_search,
        needs_cloud_job,
        task_type,
    }
}

fn route_to_json(route: &DesktopRoute) -> Value {
    serde_json::json!({
        "intent": route.intent.as_str(),
        "confidence": route.confidence,
        "reason": route.reason,
        "needs_cloud_search": route.needs_cloud_search,
        "needs_cloud_job": route.needs_cloud_job,
        "task_type": route.task_type,
    })
}

fn has_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

async fn fetch_cloud_knowledge(
    db: &tauri::State<'_, DbState>,
    user_id: &str,
    token: &str,
    query: &str,
) -> Result<Option<Value>, String> {
    let cloud_base = load_setting(db, user_id, CLOUD_API_BASE_KEY)?
        .map(|raw| parse_setting_string(&raw))
        .unwrap_or_default();
    fetch_cloud_knowledge_from_base(&cloud_base, token, query).await
}

async fn fetch_cloud_knowledge_from_base(
    cloud_base: &str,
    token: &str,
    query: &str,
) -> Result<Option<Value>, String> {
    if cloud_base.trim().is_empty() || token.trim().is_empty() {
        return Ok(Some(serde_json::json!({
            "warning": "云知识库检索未执行：缺少云端 API Base 或登录 token。"
        })));
    }
    let url = format!("{}/api/search", cloud_base.trim_end_matches('/'));
    let body = serde_json::json!({
        "query_text": query,
        "top_k": 5,
        "include_production": false,
        "slab_width_min": 0,
        "slab_width_max": 99999,
        "slab_thickness_min": 0,
        "slab_thickness_max": 99999,
        "yield_rp02_min": 0,
        "yield_rp02_max": 99999,
        "tensile_strength_min": 0,
        "tensile_strength_max": 99999,
        "elongation_min": 0,
        "elongation_max": 99999,
        "steel_mark": "",
        "steel_grade": "",
        "advice_mode": ""
    });
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await;
    let resp = match resp {
        Ok(resp) => resp,
        Err(err) => {
            return Ok(Some(serde_json::json!({
                "warning": format!("云知识库检索失败：{err}")
            })));
        }
    };
    if !resp.status().is_success() {
        return Ok(Some(serde_json::json!({
            "warning": format!("云知识库检索失败：HTTP {}", resp.status())
        })));
    }
    let value = match resp.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return Ok(Some(serde_json::json!({
                "warning": format!("云知识库响应解析失败：{err}")
            })));
        }
    };
    Ok(Some(compact_cloud_knowledge(&value)))
}

fn compact_cloud_knowledge(value: &Value) -> Value {
    let literature_items = value
        .get("literature_results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let advice_items = value
        .get("advice_contexts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let literature_results = literature_items
        .iter()
        .take(CLOUD_KNOWLEDGE_RESULT_LIMIT)
        .map(compact_cloud_value)
        .collect::<Vec<_>>();
    let advice_contexts = advice_items
        .iter()
        .take(CLOUD_KNOWLEDGE_RESULT_LIMIT)
        .map(compact_cloud_value)
        .collect::<Vec<_>>();
    let literature_result_count = literature_results.len();
    let advice_context_count = advice_contexts.len();
    let truncated = literature_items.len() > literature_result_count
        || advice_items.len() > advice_context_count
        || value_has_long_string(value);

    serde_json::json!({
        "literature_results": literature_results,
        "advice_contexts": advice_contexts,
        "tool_meta": {
            "source": "cloud_search",
            "literature_result_count": literature_result_count,
            "literature_result_total_count": literature_items.len(),
            "advice_context_count": advice_context_count,
            "advice_context_total_count": advice_items.len(),
            "string_char_limit": CLOUD_KNOWLEDGE_STRING_CHAR_LIMIT,
            "truncated": truncated,
        }
    })
}

fn compact_cloud_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(truncate_cloud_text(text)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(CLOUD_KNOWLEDGE_NESTED_ARRAY_LIMIT)
                .map(compact_cloud_value)
                .collect(),
        ),
        Value::Object(map) => {
            let mut next = Map::new();
            for (key, value) in map {
                next.insert(key.clone(), compact_cloud_value(value));
            }
            Value::Object(next)
        }
        _ => value.clone(),
    }
}

fn truncate_cloud_text(value: &str) -> String {
    truncate_text(value, CLOUD_KNOWLEDGE_STRING_CHAR_LIMIT)
}

fn truncate_text(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push('…');
    }
    result
}

fn value_has_long_string(value: &Value) -> bool {
    match value {
        Value::String(text) => text.chars().count() > CLOUD_KNOWLEDGE_STRING_CHAR_LIMIT,
        Value::Array(items) => items.iter().any(value_has_long_string),
        Value::Object(map) => map.values().any(value_has_long_string),
        _ => false,
    }
}

fn normalize_cloud_task_type(value: &str) -> Result<ConfirmedCloudTask, String> {
    match value.trim().to_lowercase().as_str() {
        "training" | "training_task" => Ok(ConfirmedCloudTask::Training),
        "optimization" | "optimize" | "optimization_task" => Ok(ConfirmedCloudTask::Optimization),
        "literature" | "literature_task" => Ok(ConfirmedCloudTask::Literature),
        _ => Err("unsupported cloud task type".to_string()),
    }
}

async fn start_confirmed_training_job(
    db: &tauri::State<'_, DbState>,
    user_id: &str,
    token: &str,
    request: &ConfirmCloudJobRequest,
) -> Result<CloudJobOutcome, String> {
    let cloud_base = load_setting(db, user_id, CLOUD_API_BASE_KEY)?
        .map(|raw| parse_setting_string(&raw))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CLOUD_API_BASE_NOT_CONFIGURED.to_string())?;
    let model_version = extract_training_model_version(&request.message)
        .unwrap_or_else(default_training_model_version);
    let payload = serde_json::json!({
        "model_version": model_version,
        "max_rows": null
    });
    let submitting = save_submitting_cloud_job(
        db,
        user_id,
        &request.conversation_id,
        request,
        ConfirmedCloudTask::Training,
        &payload,
    )?;
    let url = format!("{}/api/training/start", cloud_base.trim_end_matches('/'));
    let mut builder = reqwest::Client::new().post(url).json(&payload);
    if !token.trim().is_empty() {
        builder = builder.bearer_auth(token.trim());
    }
    let response = match builder.send().await {
        Ok(response) => response,
        Err(err) => {
            return cloud_task_request_failed_outcome(
                db,
                user_id,
                &submitting.id,
                request,
                ConfirmedCloudTask::Training,
                &payload,
                &format!("cloud training request failed: {err}"),
            );
        }
    };
    let status_code = response.status();
    let ok = status_code.is_success();
    let body_text = response.text().await.map_err(|err| err.to_string())?;
    let body_value = serde_json::from_str::<Value>(&body_text)
        .unwrap_or_else(|_| serde_json::json!({ "body": body_text }));
    let cloud_job_id = extract_response_job_id(&body_value)
        .unwrap_or_else(|| format!("pending-{}", Uuid::new_v4()));
    let job_status = extract_response_status(&body_value).unwrap_or_else(|| {
        if ok {
            "running".to_string()
        } else {
            "failed".to_string()
        }
    });
    let saved = upsert_cloud_job_for_user(
        db,
        user_id,
        CloudJobInput {
            id: Some(submitting.id),
            conversation_id: Some(request.conversation_id.trim().to_string()),
            cloud_job_id: cloud_job_id.clone(),
            r#type: ConfirmedCloudTask::Training.as_str().to_string(),
            status: job_status.clone(),
            payload_json: Some(
                serde_json::json!({
                    "source": "desktop_chat_confirmation",
                    "action_id": request.action_id.as_str(),
                    "payload": payload
                })
                .to_string(),
            ),
            result_json: Some(body_value.to_string()),
        },
    )?;
    let answer = if ok {
        format!(
            "Cloud training submitted. Job: {}. Local mirror: {}.",
            cloud_job_id, saved.id
        )
    } else {
        format!(
            "Cloud training request failed with HTTP {}. Local mirror saved as {}.",
            status_code, saved.id
        )
    };
    Ok(CloudJobOutcome {
        job_id: saved.id,
        status: job_status,
        answer,
    })
}

async fn start_confirmed_json_cloud_job(
    db: &tauri::State<'_, DbState>,
    user_id: &str,
    token: &str,
    request: &ConfirmCloudJobRequest,
    task: ConfirmedCloudTask,
    path: &str,
    payload: Value,
) -> Result<CloudJobOutcome, String> {
    let cloud_base = load_setting(db, user_id, CLOUD_API_BASE_KEY)?
        .map(|raw| parse_setting_string(&raw))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CLOUD_API_BASE_NOT_CONFIGURED.to_string())?;
    let submitting = save_submitting_cloud_job(
        db,
        user_id,
        &request.conversation_id,
        request,
        task,
        &payload,
    )?;
    let url = format!("{}/{}", cloud_base.trim_end_matches('/'), path);
    let mut builder = reqwest::Client::new().post(url).json(&payload);
    if !token.trim().is_empty() {
        builder = builder.bearer_auth(token.trim());
    }
    let response = match builder.send().await {
        Ok(response) => response,
        Err(err) => {
            return cloud_task_request_failed_outcome(
                db,
                user_id,
                &submitting.id,
                request,
                task,
                &payload,
                &format!("cloud {} request failed: {err}", task.as_str()),
            );
        }
    };
    let status_code = response.status();
    let ok = status_code.is_success();
    let body_text = response.text().await.map_err(|err| err.to_string())?;
    let body_value = serde_json::from_str::<Value>(&body_text)
        .unwrap_or_else(|_| serde_json::json!({ "body": body_text }));
    let cloud_job_id =
        extract_response_job_id(&body_value).unwrap_or_else(|| submitting.cloud_job_id.clone());
    let job_status = extract_response_status(&body_value).unwrap_or_else(|| {
        if ok {
            default_cloud_task_success_status(task)
        } else {
            "failed".to_string()
        }
    });
    let saved = upsert_cloud_job_for_user(
        db,
        user_id,
        CloudJobInput {
            id: Some(submitting.id),
            conversation_id: Some(request.conversation_id.trim().to_string()),
            cloud_job_id: cloud_job_id.clone(),
            r#type: task.as_str().to_string(),
            status: job_status.clone(),
            payload_json: Some(
                serde_json::json!({
                    "source": "desktop_chat_confirmation",
                    "action_id": request.action_id.as_str(),
                    "path": path,
                    "payload": payload
                })
                .to_string(),
            ),
            result_json: Some(body_value.to_string()),
        },
    )?;
    let answer = if ok {
        format!(
            "Cloud {} task submitted. Job: {}. Local mirror: {}.",
            task.as_str(),
            cloud_job_id,
            saved.id
        )
    } else {
        format!(
            "Cloud {} request failed with HTTP {}. Local mirror saved as {}.",
            task.as_str(),
            status_code,
            saved.id
        )
    };
    Ok(CloudJobOutcome {
        job_id: saved.id,
        status: job_status,
        answer,
    })
}

fn save_submitting_cloud_job(
    db: &tauri::State<'_, DbState>,
    user_id: &str,
    conversation_id: &str,
    request: &ConfirmCloudJobRequest,
    task: ConfirmedCloudTask,
    payload: &Value,
) -> Result<crate::models::CloudJob, String> {
    upsert_cloud_job_for_user(
        db,
        user_id,
        CloudJobInput {
            id: None,
            conversation_id: Some(conversation_id.trim().to_string()),
            cloud_job_id: format!("pending-{}", Uuid::new_v4()),
            r#type: task.as_str().to_string(),
            status: "submitting".to_string(),
            payload_json: Some(
                serde_json::json!({
                    "source": "desktop_chat_confirmation",
                    "action_id": request.action_id.as_str(),
                    "payload": payload
                })
                .to_string(),
            ),
            result_json: None,
        },
    )
}

fn cloud_task_request_failed_outcome(
    db: &tauri::State<'_, DbState>,
    user_id: &str,
    local_job_id: &str,
    request: &ConfirmCloudJobRequest,
    task: ConfirmedCloudTask,
    payload: &Value,
    error: &str,
) -> Result<CloudJobOutcome, String> {
    let saved = upsert_cloud_job_for_user(
        db,
        user_id,
        CloudJobInput {
            id: Some(local_job_id.to_string()),
            conversation_id: Some(request.conversation_id.trim().to_string()),
            cloud_job_id: format!("pending-{}", local_job_id),
            r#type: task.as_str().to_string(),
            status: "failed".to_string(),
            payload_json: Some(
                serde_json::json!({
                    "source": "desktop_chat_confirmation",
                    "action_id": request.action_id.as_str(),
                    "payload": payload
                })
                .to_string(),
            ),
            result_json: Some(serde_json::json!({ "error": error }).to_string()),
        },
    )?;
    Ok(CloudJobOutcome {
        job_id: saved.id.clone(),
        status: "failed".to_string(),
        answer: format!(
            "Cloud {} request failed before submission. Local mirror saved as {}. {}",
            task.as_str(),
            saved.id,
            error
        ),
    })
}

fn save_needs_input_cloud_job(
    db: &tauri::State<'_, DbState>,
    user_id: &str,
    conversation_id: &str,
    request: &ConfirmCloudJobRequest,
    task: ConfirmedCloudTask,
) -> Result<CloudJobOutcome, String> {
    let cloud_job_id = format!("pending-{}", Uuid::new_v4());
    let payload = serde_json::json!({
        "source": "desktop_chat_confirmation",
        "action_id": request.action_id.as_str(),
        "message": request.message.as_str(),
        "next": task_page_hint(task)
    });
    let saved = upsert_cloud_job_for_user(
        db,
        user_id,
        CloudJobInput {
            id: None,
            conversation_id: Some(conversation_id.to_string()),
            cloud_job_id,
            r#type: task.as_str().to_string(),
            status: "needs_input".to_string(),
            payload_json: Some(payload.to_string()),
            result_json: None,
        },
    )?;
    Ok(CloudJobOutcome {
        job_id: saved.id,
        status: "needs_input".to_string(),
        answer: format!(
            "Saved a {} task intent locally. {}",
            task.as_str(),
            task_page_hint(task)
        ),
    })
}

fn extract_training_model_version(message: &str) -> Option<String> {
    let ascii = message
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>();
    ascii
        .split_whitespace()
        .map(str::trim)
        .find(|token| {
            token.len() <= 40
                && token.chars().any(|ch| ch.is_ascii_digit())
                && (token.starts_with('v')
                    || token.starts_with('V')
                    || token.to_lowercase().starts_with("model"))
        })
        .map(str::to_string)
}

fn default_training_model_version() -> String {
    format!("desktop-{}", Utc::now().format("%Y%m%d%H%M%S"))
}

fn extract_response_job_id(value: &Value) -> Option<String> {
    value
        .get("job_id")
        .or_else(|| value.get("remote_job_id"))
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn extract_response_status(value: &Value) -> Option<String> {
    value
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn task_page_hint(task: ConfirmedCloudTask) -> &'static str {
    match task {
        ConfirmedCloudTask::Training => "Track it in the desktop task mirror.",
        ConfirmedCloudTask::Optimization => {
            "Open the optimizer page to fill process parameters before submitting."
        }
        ConfirmedCloudTask::Literature => {
            "Open the knowledge/literature page to upload PDFs or choose a folder."
        }
    }
}

fn default_cloud_task_success_status(task: ConfirmedCloudTask) -> String {
    match task {
        ConfirmedCloudTask::Optimization => "completed",
        ConfirmedCloudTask::Training | ConfirmedCloudTask::Literature => "running",
    }
    .to_string()
}

fn structured_cloud_task_payload(task: ConfirmedCloudTask, message: &str) -> Option<Value> {
    match task {
        ConfirmedCloudTask::Training => None,
        ConfirmedCloudTask::Optimization => extract_json_object_from_message(message),
        ConfirmedCloudTask::Literature => {
            let payload = extract_json_object_from_message(message).or_else(|| {
                extract_marker_value(message, "folder=").map(|folder| {
                    serde_json::json!({
                        "folder": folder
                    })
                })
            })?;
            payload
                .get("folder")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some(payload)
        }
    }
}

fn extract_json_object_from_message(message: &str) -> Option<Value> {
    let start = message.find('{')?;
    let end = message.rfind('}')?;
    if end <= start {
        return None;
    }
    let value = serde_json::from_str::<Value>(&message[start..=end]).ok()?;
    if value.is_object() {
        Some(value)
    } else {
        None
    }
}

fn extract_marker_value(message: &str, marker: &str) -> Option<String> {
    let lower = message.to_lowercase();
    let start = lower.find(marker)? + marker.len();
    let value = message
        .get(start..)?
        .trim_start()
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';'))
        .next()?
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`'))
        .trim()
        .to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
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
    mut is_cancelled: impl FnMut() -> Result<bool, String>,
    mut on_delta: impl FnMut(&str),
) -> Result<StreamedLlmAnswer, String> {
    if is_cancelled()? {
        return Err("LLM run cancelled".to_string());
    }
    let base_url = if config.base_url.trim().is_empty() {
        default_base_url(&config.provider).to_string()
    } else {
        config.base_url.trim().to_string()
    };
    let url = normalize_chat_completions_url(&base_url);
    let body = ChatCompletionRequest {
        model: config.model_name.trim().to_string(),
        stream: true,
        temperature: 0.2,
        messages: vec![
            ChatCompletionMessage {
                role: "system".to_string(),
                content: context_prompt.to_string(),
            },
            ChatCompletionMessage {
                role: "user".to_string(),
                content: user_message.to_string(),
            },
        ],
    };
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(config.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("LLM 请求失败：{err}"))?;
    if is_cancelled()? {
        return Err("LLM run cancelled".to_string());
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "LLM 请求失败（HTTP {status}）：{}",
            text.chars().take(500).collect::<String>()
        ));
    }

    let mut answer = String::new();
    let mut buffer = String::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if is_cancelled()? {
            return Ok(StreamedLlmAnswer {
                text: answer,
                stopped: true,
            });
        }
        let chunk = chunk.map_err(|err| format!("读取 LLM 流失败：{err}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end_matches('\r').to_string();
            buffer = buffer[pos + 1..].to_string();
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                return Ok(StreamedLlmAnswer {
                    text: answer,
                    stopped: false,
                });
            }
            let value = match serde_json::from_str::<Value>(data) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let delta = extract_chat_delta(&value);
            if delta.is_empty() {
                continue;
            }
            if is_cancelled()? {
                return Ok(StreamedLlmAnswer {
                    text: answer,
                    stopped: true,
                });
            }
            answer.push_str(&delta);
            on_delta(&delta);
        }
    }
    Ok(StreamedLlmAnswer {
        text: answer,
        stopped: false,
    })
}

fn extract_chat_delta(value: &Value) -> String {
    let choice = &value["choices"][0];
    choice["delta"]["content"]
        .as_str()
        .or_else(|| choice["message"]["content"].as_str())
        .or_else(|| choice["text"].as_str())
        .unwrap_or("")
        .to_string()
}

fn build_agent_response_json(
    run_id: &str,
    conversation_id: &str,
    answer: &str,
    config: &LocalLlmConfig,
    packet: &Value,
    route: &DesktopRoute,
) -> Value {
    let selected_tools = route_tools_json(route);
    let evidence = packet
        .get("cloud_knowledge")
        .map(|cloud| {
            serde_json::json!([{
                "evidence_id": "cloud_knowledge",
                "type": "cloud_knowledge",
                "title": "云知识库检索结果",
                "content": cloud.to_string(),
                "evidence_level": "direct"
            }])
        })
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::json!({
        "run_id": run_id,
        "session_id": conversation_id,
        "status": "completed",
        "answer": answer,
        "follow_up_questions": [],
        "plan_steps": [],
        "tool_calls": [],
        "evidence": evidence,
        "recommendations": [],
        "verification": {
            "confidence": "medium",
            "citation_count": 0,
            "missing_citations": [],
            "numeric_warnings": [],
            "unsupported_claims": [],
            "summary": "本地桌面端根据当前上下文生成。"
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
            "needs_evidence": route.needs_cloud_search,
            "needs_tools": selected_tools.clone(),
            "needs_cloud_job": route.needs_cloud_job,
            "task_type": route.task_type,
            "missing_slots": [],
            "answer_policy": "desktop_local",
            "reason": route.reason
        },
        "workflow_trace": {
            "route": "desktop_local_agent",
            "tools_selected": selected_tools,
            "tools_skipped": [],
            "evidence_policy": "local_context_first",
            "answer_policy": "desktop_local",
            "model_provider": config.provider,
            "model_name": config.model_name,
            "notes": ["conversation_summary、recent_messages、selected_memories、history_hits 来自本地 SQLite。"]
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

fn build_cloud_job_confirmation_response_json(
    run_id: &str,
    conversation_id: &str,
    config: &LocalLlmConfig,
    _packet: &Value,
    route: &DesktopRoute,
    message: &str,
) -> Value {
    let task_type = route.task_type.unwrap_or("cloud_task");
    let action_id = format!("desktop:{task_type}:{run_id}");
    let permission = cloud_task_permission(task_type);
    let warning = cloud_task_warning(task_type);
    let selected_tools = serde_json::json!([task_type]);
    let answer = format!(
        "Detected a cloud {} task. Confirm before the desktop client sends it to the cloud backend.",
        task_type
    );
    serde_json::json!({
        "run_id": run_id,
        "session_id": conversation_id,
        "status": "needs_confirmation",
        "answer": answer,
        "follow_up_questions": [],
        "plan_steps": [{
            "step_id": "confirm_cloud_task",
            "title": "Confirm cloud task",
            "description": warning,
            "tool_name": task_type,
            "status": "needs_confirmation"
        }],
        "tool_calls": [{
            "call_id": action_id,
            "action_id": action_id,
            "tool_name": task_type,
            "title": cloud_task_title(task_type),
            "permission": permission,
            "arguments": {
                "task_type": task_type,
                "conversation_id": conversation_id,
                "user_message": message,
                "estimated_time": cloud_task_estimated_time(task_type),
                "resource_usage": cloud_task_resource_usage(task_type)
            },
            "status": "needs_confirmation"
        }],
        "evidence": [],
        "recommendations": [],
        "verification": {
            "confidence": "medium",
            "citation_count": 0,
            "missing_citations": [],
            "numeric_warnings": [],
            "unsupported_claims": [],
            "summary": "Cloud task is waiting for explicit desktop confirmation."
        },
        "memory": {
            "session_id": conversation_id,
            "notes": []
        },
        "pending_confirmations": [{
            "action_id": action_id,
            "tool_name": task_type,
            "title": cloud_task_title(task_type),
            "permission": permission,
            "arguments": {
                "task_type": task_type,
                "conversation_id": conversation_id,
                "user_message": message,
                "estimated_time": cloud_task_estimated_time(task_type),
                "resource_usage": cloud_task_resource_usage(task_type)
            },
            "warning": warning
        }],
        "intent": {
            "intent_type": route.intent.as_str(),
            "domain": "steel",
            "risk_level": if permission == "danger" { "high" } else { "medium" },
            "needs_evidence": false,
            "needs_tools": selected_tools.clone(),
            "needs_cloud_job": true,
            "task_type": task_type,
            "missing_slots": [],
            "answer_policy": "desktop_confirm_before_cloud",
            "reason": route.reason,
            "confidence": route.confidence
        },
        "workflow_trace": {
            "route": "desktop_local_agent",
            "tools_selected": selected_tools,
            "tools_skipped": [],
            "evidence_policy": "local_context_first",
            "answer_policy": "confirm_before_cloud_job",
            "model_provider": config.provider,
            "model_name": config.model_name,
            "notes": ["Cloud jobs are not started from chat without confirmation."]
        },
        "workflow": {
            "run_id": run_id,
            "state": "waiting_confirmation",
            "nodes": [],
            "edges": [],
            "events": [],
            "summary": "waiting for cloud task confirmation",
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

fn build_cloud_job_result_response_json(
    run_id: &str,
    conversation_id: &str,
    status: &str,
    answer: &str,
    task_type: &str,
    job_id: Option<&str>,
) -> Value {
    serde_json::json!({
        "run_id": run_id,
        "session_id": conversation_id,
        "status": status,
        "answer": answer,
        "follow_up_questions": [],
        "plan_steps": [],
        "tool_calls": [{
            "call_id": format!("desktop:{task_type}:{run_id}"),
            "action_id": format!("desktop:{task_type}:{run_id}"),
            "tool_name": task_type,
            "title": cloud_task_title(task_type),
            "permission": "auto",
            "arguments": {
                "task_type": task_type,
                "job_id": job_id
            },
            "status": status
        }],
        "evidence": [],
        "recommendations": [],
        "verification": {
            "confidence": "medium",
            "citation_count": 0,
            "missing_citations": [],
            "numeric_warnings": [],
            "unsupported_claims": [],
            "summary": "Desktop cloud task confirmation handled."
        },
        "memory": {
            "session_id": conversation_id,
            "notes": []
        },
        "pending_confirmations": [],
        "intent": {
            "intent_type": format!("{}_task", task_type),
            "domain": "steel",
            "risk_level": "medium",
            "needs_evidence": false,
            "needs_tools": [task_type],
            "needs_cloud_job": true,
            "task_type": task_type,
            "missing_slots": [],
            "answer_policy": "desktop_cloud_task_result",
            "reason": "confirmed desktop cloud task"
        },
        "workflow_trace": {
            "route": "desktop_cloud_task_confirmation",
            "tools_selected": [task_type],
            "tools_skipped": [],
            "evidence_policy": "none",
            "answer_policy": "desktop_cloud_task_result",
            "model_provider": "",
            "model_name": "",
            "notes": []
        },
        "workflow": {
            "run_id": run_id,
            "state": status,
            "nodes": [],
            "edges": [],
            "events": [],
            "summary": "desktop cloud task confirmation handled",
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

fn cloud_task_title(task_type: &str) -> &'static str {
    match task_type {
        "training" => "Start cloud model training",
        "optimization" => "Prepare cloud process optimization",
        "literature" => "Prepare cloud literature processing",
        _ => "Run cloud task",
    }
}

fn cloud_task_permission(task_type: &str) -> &'static str {
    if task_type == "training" {
        "danger"
    } else {
        "confirm"
    }
}

fn cloud_task_warning(task_type: &str) -> &'static str {
    match task_type {
        "training" => "Training can consume cloud compute and may take a long time.",
        "optimization" => "Optimization needs structured process parameters before it can run.",
        "literature" => {
            "Literature processing needs a target folder or uploaded PDF before it can run."
        }
        _ => "This task will use the cloud backend.",
    }
}

fn cloud_task_estimated_time(task_type: &str) -> &'static str {
    match task_type {
        "training" => "minutes to hours",
        "optimization" => "depends on parameter size",
        "literature" => "depends on PDF count",
        _ => "unknown",
    }
}

fn cloud_task_resource_usage(task_type: &str) -> &'static str {
    match task_type {
        "training" => "cloud training worker",
        "optimization" => "cloud optimizer",
        "literature" => "cloud parser",
        _ => "cloud backend",
    }
}

fn route_tools_json(route: &DesktopRoute) -> Value {
    let mut tools = Vec::new();
    if route.needs_cloud_search {
        tools.push("cloud_knowledge_search");
    }
    if let Some(task_type) = route.task_type {
        tools.push(task_type);
    }
    serde_json::json!(tools)
}

fn normalize_chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        return trimmed.to_string();
    }
    format!("{trimmed}/chat/completions")
}

fn default_base_url(provider: &str) -> &'static str {
    match provider {
        "deepseek" => "https://api.deepseek.com",
        "openai" => "https://api.openai.com/v1",
        _ => "",
    }
}

fn parse_setting_string(raw: &str) -> String {
    serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_string())
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

    #[test]
    fn normalizes_openai_compatible_chat_url() {
        assert_eq!(
            normalize_chat_completions_url("https://api.deepseek.com"),
            "https://api.deepseek.com/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("https://example.test/v1/chat/completions"),
            "https://example.test/v1/chat/completions"
        );
    }

    #[test]
    fn extracts_openai_compatible_stream_deltas() {
        assert_eq!(
            extract_chat_delta(&serde_json::json!({"choices": [{"delta": {"content": "a"}}]})),
            "a"
        );
        assert_eq!(
            extract_chat_delta(&serde_json::json!({"choices": [{"message": {"content": "b"}}]})),
            "b"
        );
        assert_eq!(
            extract_chat_delta(&serde_json::json!({"choices": [{"text": "c"}]})),
            "c"
        );
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

    #[test]
    fn fetch_cloud_knowledge_posts_search_tool_and_compacts_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock cloud search");
        let addr = listener.local_addr().expect("mock cloud search addr");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept mock cloud search request");
            let request = read_http_request(&mut stream);
            let request_text = String::from_utf8_lossy(&request);
            let lower_request = request_text.to_ascii_lowercase();
            assert!(request_text.starts_with("POST /api/search "));
            assert!(lower_request.contains("authorization: bearer cloud-token"));
            assert!(request_text.contains(r#""query_text":"Q355B 标准""#));
            assert!(request_text.contains(r#""top_k":5"#));

            let body = serde_json::json!({
                "literature_results": [
                    {"paper_name": "Q355B 标准", "content": "云知识库片段"}
                ],
                "advice_contexts": ["建议片段"]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write mock cloud search response");
        });

        let cloud = tauri::async_runtime::block_on(fetch_cloud_knowledge_from_base(
            &format!("http://{addr}"),
            "cloud-token",
            "Q355B 标准",
        ))
        .expect("cloud search")
        .expect("cloud search context");

        assert_eq!(cloud["literature_results"][0]["paper_name"], "Q355B 标准");
        assert_eq!(cloud["advice_contexts"][0], "建议片段");
        assert_eq!(cloud["tool_meta"]["literature_result_total_count"], 1);
        assert_eq!(cloud["tool_meta"]["advice_context_total_count"], 1);
        server.join().expect("mock cloud search server");
    }

    #[test]
    fn local_agent_message_append_requires_user_owned_conversation() {
        let conn = rusqlite::Connection::open_in_memory().expect("sqlite");
        conn.execute_batch(include_str!("schema.sql"))
            .expect("schema");
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
            params!["conversation-1", "user-a", "title", "t1", "t1"],
        )
        .expect("insert conversation");

        let err = append_local_message_to_conn(
            &conn,
            "user-b",
            "conversation-1",
            "user",
            "cross-user message",
            None,
            "t2",
        )
        .expect_err("cross-user append should fail");
        assert!(err.contains("conversation does not belong"));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .expect("count messages");
        assert_eq!(count, 0);

        append_local_message_to_conn(
            &conn,
            "user-a",
            "conversation-1",
            "user",
            "owned message",
            None,
            "t3",
        )
        .expect("owned append");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .expect("count messages");
        assert_eq!(count, 1);
    }

    #[test]
    fn local_agent_ensure_conversation_rejects_cross_user_id() {
        let conn = rusqlite::Connection::open_in_memory().expect("sqlite");
        conn.execute_batch(include_str!("schema.sql"))
            .expect("schema");
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
            params!["conversation-1", "user-a", "title", "t1", "t1"],
        )
        .expect("insert conversation");

        let err =
            ensure_conversation_in_conn(&conn, "user-b", "conversation-1", "new message", "t2")
                .expect_err("cross-user conversation id should fail");
        assert!(err.contains("conversation does not belong"));

        ensure_conversation_in_conn(&conn, "user-a", "conversation-1", "same owner", "t3")
            .expect("same user can reuse conversation");
        ensure_conversation_in_conn(&conn, "user-b", "conversation-2", "new owner", "t4")
            .expect("new conversation can be created");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
            .expect("count conversations");
        assert_eq!(count, 2);
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
    fn local_prompt_includes_cloud_knowledge_as_tool_context() {
        let packet = serde_json::json!({
            "cloud_knowledge": {
                "literature_results": [{"paper_name": "Q355B 热轧标准", "content": "终轧温度 860C"}],
                "advice_contexts": ["云知识库片段"]
            },
            "desktop_meta": {"context_version": 2}
        });

        let prompt = build_desktop_context_prompt(&packet);

        assert!(prompt.contains("cloud_knowledge"));
        assert!(prompt.contains("Q355B 热轧标准"));
        assert!(prompt.contains("云知识库片段"));
    }

    #[test]
    fn compact_cloud_knowledge_bounds_cloud_search_context() {
        let long_text = "x".repeat(CLOUD_KNOWLEDGE_STRING_CHAR_LIMIT + 50);
        let literature_results = (0..10)
            .map(|index| {
                serde_json::json!({
                    "paper_name": format!("paper-{index}"),
                    "content": long_text.clone(),
                    "nested": (0..12).map(|item| serde_json::json!(format!("nested-{item}"))).collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let advice_contexts = (0..10)
            .map(|index| serde_json::json!(format!("advice-{index}-{long_text}")))
            .collect::<Vec<_>>();
        let compact = compact_cloud_knowledge(&serde_json::json!({
            "literature_results": literature_results,
            "advice_contexts": advice_contexts,
        }));

        assert_eq!(
            compact["literature_results"]
                .as_array()
                .expect("literature")
                .len(),
            CLOUD_KNOWLEDGE_RESULT_LIMIT
        );
        assert_eq!(
            compact["advice_contexts"].as_array().expect("advice").len(),
            CLOUD_KNOWLEDGE_RESULT_LIMIT
        );
        assert_eq!(compact["tool_meta"]["truncated"], true);
        assert_eq!(
            compact["literature_results"][0]["nested"]
                .as_array()
                .expect("nested")
                .len(),
            CLOUD_KNOWLEDGE_NESTED_ARRAY_LIMIT
        );
        assert!(
            compact["literature_results"][0]["content"]
                .as_str()
                .expect("content")
                .chars()
                .count()
                <= CLOUD_KNOWLEDGE_STRING_CHAR_LIMIT + 1
        );
        assert!(!compact.to_string().contains("paper-9"));
    }

    #[test]
    fn local_agent_response_marks_desktop_controller_and_cloud_search_tool() {
        let route = route(
            DesktopIntentKind::KnowledgeQa,
            0.86,
            "knowledge or citation retrieval requested",
            true,
            false,
            None,
        );
        let response = build_agent_response_json(
            "run-1",
            "conversation-1",
            "answer",
            &LocalLlmConfig {
                provider: "deepseek".to_string(),
                base_url: "https://api.deepseek.com".to_string(),
                model_name: "deepseek-chat".to_string(),
                api_key: "sk-test".to_string(),
            },
            &serde_json::json!({
                "cloud_knowledge": {"advice_contexts": ["片段"]},
                "desktop_meta": {"context_version": 2}
            }),
            &route,
        );

        assert_eq!(response["intent"]["answer_policy"], "desktop_local");
        assert_eq!(response["workflow_trace"]["route"], "desktop_local_agent");
        assert_eq!(
            response["workflow_trace"]["tools_selected"][0],
            "cloud_knowledge_search"
        );
        assert_eq!(response["evidence"][0]["type"], "cloud_knowledge");
    }

    #[test]
    fn knowledge_keywords_trigger_cloud_search_tool() {
        assert!(classify_desktop_intent("帮我检索知识库里的热轧标准").needs_cloud_search);
        assert!(classify_desktop_intent("找一下相关论文和文献").needs_cloud_search);
        assert!(!classify_desktop_intent("根据我的记忆总结一下这个客户偏好").needs_cloud_search);
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
    fn cloud_job_intents_return_pending_confirmation_response() {
        let route = route(
            DesktopIntentKind::TrainingTask,
            0.9,
            "explicit training task request",
            false,
            true,
            Some("training"),
        );
        let response = build_cloud_job_confirmation_response_json(
            "run-1",
            "conversation-1",
            &LocalLlmConfig::default(),
            &serde_json::json!({"desktop_meta": {"context_version": 2}}),
            &route,
            "start training v2",
        );

        assert_eq!(response["status"], "needs_confirmation");
        assert_eq!(
            response["pending_confirmations"][0]["arguments"]["task_type"],
            "training"
        );
        assert_eq!(
            response["pending_confirmations"][0]["arguments"]["conversation_id"],
            "conversation-1"
        );
        assert_eq!(response["pending_confirmations"][0]["permission"], "danger");
    }

    #[test]
    fn structured_cloud_payload_accepts_explicit_json_only_for_optimization() {
        let payload = structured_cloud_task_payload(
            ConfirmedCloudTask::Optimization,
            r#"run optimization {"maxiter": 20, "popsize": 8}"#,
        )
        .expect("payload");

        assert_eq!(payload["maxiter"], 20);
        assert!(structured_cloud_task_payload(
            ConfirmedCloudTask::Optimization,
            "run optimization from current chat"
        )
        .is_none());
    }

    #[test]
    fn structured_cloud_payload_requires_literature_folder() {
        let from_marker = structured_cloud_task_payload(
            ConfirmedCloudTask::Literature,
            "process literature folder=Q355B_papers",
        )
        .expect("folder payload");
        let from_json = structured_cloud_task_payload(
            ConfirmedCloudTask::Literature,
            r#"process literature {"folder": "hot_roll"}"#,
        )
        .expect("json payload");

        assert_eq!(from_marker["folder"], "Q355B_papers");
        assert_eq!(from_json["folder"], "hot_roll");
        assert!(structured_cloud_task_payload(
            ConfirmedCloudTask::Literature,
            r#"{"parse_mode": "auto"}"#
        )
        .is_none());
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
