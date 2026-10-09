use super::model::{
    load_agent_preferences, AgentPreferences, DesktopRoute, LocalAgentAttachment,
    LocalAgentChatRequest, LocalLlmConfig, SummarizeConversationResponse, SummaryPreparation,
};
use crate::agent::context::{
    build_summary_prompt, estimate_summary_tokens, messages_after_covered_id, plan_summary,
};
use crate::agent::context::{ContextItem, ContextSource};
use crate::agent::runtime::{
    AgentInputQueue, AgentLoopAttachment, AgentLoopLimits, AgentLoopRequest, ContextEntry,
    EvidenceAttachment,
};
use crate::context::build_context_packet_for_connection;
use crate::rag::citation::{load_evidence_pack, EvidencePack};
use crate::skills::SkillContext;
use crate::storage::secrets::SecretStore;
use crate::tasks::mailbox::MailboxMessage;
use chrono::Utc;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use uuid::Uuid;

pub struct ChatPreparation {
    pub run_id: Uuid,
    pub conversation_id: Uuid,
    pub message: String,
    pub smart_search_enabled: bool,
    pub route: DesktopRoute,
    pub prompt: String,
    pub config: LocalLlmConfig,
    pub evidence_pack: Option<EvidencePack>,
    pub attachments: Vec<LocalAgentAttachment>,
    pub attachment_roots: Vec<PathBuf>,
    pub skills: SkillContext,
    pub active_domains: Vec<crate::domains::DomainManifest>,
    pub selected_memories: Vec<Value>,
    pub unavailable_response: Option<Value>,
    pub agent_preferences: AgentPreferences,
}

pub fn build_agent_loop_request_with_attachments(
    assistant_message_id: Uuid,
    prompt: &str,
    message: &str,
    evidence_pack: Option<&EvidencePack>,
    attachments: &[LocalAgentAttachment],
) -> AgentLoopRequest {
    AgentLoopRequest {
        assistant_message_id,
        context: vec![
            ContextEntry::new(ContextItem::new(
                "desktop-system",
                ContextSource::System,
                prompt,
            )),
            ContextEntry::new(ContextItem::new(
                "desktop-current-request",
                ContextSource::CurrentRequest,
                message,
            )),
        ],
        output_reservation: 2_048,
        reasoning_reservation: crate::agent::context::DEFAULT_REASONING_RESERVATION,
        evidence: evidence_pack.map(|pack| EvidenceAttachment {
            evidence_pack_id: pack.id,
            citation_numbers: pack
                .evidence
                .iter()
                .map(|item| item.citation_number)
                .collect(),
        }),
        attachments: attachments
            .iter()
            .map(|attachment| AgentLoopAttachment {
                data: attachment.data.clone(),
                mime: attachment.mime.clone(),
                name: attachment.name.clone(),
                path: attachment.path.clone(),
            })
            .collect(),
        limits: AgentLoopLimits::default(),
        input_queue: AgentInputQueue::default(),
        resume: None,
    }
}

pub fn add_mailbox_context(request: &mut AgentLoopRequest, message: &MailboxMessage) {
    request.context.insert(
        1,
        ContextEntry::with_role(
            ContextItem::new(
                format!("mailbox-{}", message.id),
                ContextSource::Mailbox,
                format!("来自 {} 的协作消息：{}", message.sender, message.content),
            ),
            crate::agent::protocol::AgentMessageRole::User,
        ),
    );
}

pub fn prepare_chat(
    conn: &mut Connection,
    workspace_id: &str,
    request: LocalAgentChatRequest,
    secrets: &dyn SecretStore,
) -> Result<ChatPreparation, String> {
    let message = request.message.trim().to_string();
    if message.is_empty() {
        return Err("message is required".to_string());
    }
    let run_id = request
        .run_id
        .as_deref()
        .map(|value| Uuid::parse_str(value.trim()).map_err(|_| "run_id must be a UUID".to_string()))
        .transpose()?
        .unwrap_or_else(Uuid::new_v4);
    let scheduled_request_cancelled: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM cron_outbox WHERE workspace_id = ?1 AND run_id = ?2 AND acknowledged_at IS NOT NULL)",
        rusqlite::params![workspace_id, run_id.to_string()], |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    if scheduled_request_cancelled {
        return Err("schedule slot was cancelled before Agent startup".to_string());
    }
    let mut agent_preferences = load_agent_preferences(conn, workspace_id)?;
    if let Some(agent_id) = request.agent_id.as_deref() {
        crate::tasks::model::validate_identifier("agent_id", agent_id)
            .map_err(|error| error.to_string())?;
        let profile = crate::agent::profiles::get(conn, workspace_id, agent_id)?
            .ok_or_else(|| "requested Agent profile was not found".to_string())?;
        if !profile.enabled {
            return Err("requested Agent profile is disabled".to_string());
        }
        agent_preferences.default_agent = agent_id.to_string();
        agent_preferences.auto_select_agent = false;
    }
    if !agent_preferences.allow_file_access && !request.attachments.is_empty() {
        return Err("文件访问已在 Agent 设置中关闭，不能处理附件".to_string());
    }
    let attachment_roots = request
        .attachments
        .iter()
        .filter_map(|attachment| attachment.path.as_deref())
        .map(|path| {
            let candidate = PathBuf::from(path.trim());
            if !candidate.is_absolute() {
                return Err("附件路径必须是绝对路径".to_string());
            }
            let metadata =
                std::fs::metadata(&candidate).map_err(|error| format!("无法读取附件：{error}"))?;
            if !metadata.is_file() {
                return Err("附件路径必须指向文件".to_string());
            }
            candidate
                .parent()
                .map(|parent| parent.to_path_buf())
                .ok_or_else(|| "附件路径没有可用的父目录".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let conversation_id = super::session::resolve_conversation(
        conn,
        workspace_id,
        request.session_id.as_deref(),
        &message,
    )?;
    let conversation_id_text = conversation_id.to_string();
    let classified_route = super::routing::classify_desktop_intent(&message);
    let packet = serde_json::to_value(build_context_packet_for_connection(
        conn,
        workspace_id,
        &conversation_id_text,
        &message,
    )?)
    .map_err(|error| error.to_string())?;
    let mut packet = packet;
    let evidence_pack = load_evidence_pack_reference(conn, workspace_id, request.evidence_pack_id)?;
    let route = super::routing::route_with_evidence_pack(classified_route, evidence_pack.is_some());
    let selected_agent = super::routing::selected_agent_id(
        &route,
        &agent_preferences.default_agent,
        agent_preferences.auto_select_agent,
    );
    packet["desktop_route"] = super::routing::route_to_json(&route);
    packet["agent_profile"] = serde_json::json!({
        "default_id": agent_preferences.default_agent,
        "auto_select": agent_preferences.auto_select_agent,
        "selected_id": selected_agent,
        "auto_plan": agent_preferences.auto_plan,
        "auto_knowledge": agent_preferences.auto_knowledge,
        "auto_tools": agent_preferences.auto_tools,
    });
    if agent_preferences.auto_plan {
        packet["agent_plan"] = serde_json::json!({
            "enabled": true,
            "steps": super::routing::plan_steps_for_route(&route),
        });
    }
    if let Some(pack) = &evidence_pack {
        packet["evidence_pack"] = serde_json::to_value(pack).map_err(|error| error.to_string())?;
    }
    let skills = crate::skills::load_context_for_query(
        conn,
        workspace_id,
        env!("CARGO_PKG_VERSION"),
        &message,
    )?;
    packet["skills"] = serde_json::json!({
        "available": skills.summaries,
        "enabled_versions": skills.rendered.enabled_versions,
        "loaded": skills.rendered.loaded,
        "prompt": skills.rendered.prompt,
    });
    let selected_memories = packet
        .get("selected_memories")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unavailable_response = route.unavailable_capability.map(|_| {
        super::routing::build_capability_unavailable_response_json(
            &run_id.to_string(),
            &conversation_id_text,
            &route,
            &skills.rendered.enabled_versions,
            &skills.summaries,
            &skills.rendered.loaded,
            &selected_memories,
        )
    });
    let (config, prompt, active_domains) = if unavailable_response.is_some() {
        (LocalLlmConfig::default(), String::new(), Vec::new())
    } else {
        let config = super::provider::load_local_llm_config(conn, workspace_id, secrets)?;
        super::provider::validate_local_llm_config(&config)?;
        let active_domains =
            crate::storage::repositories::domains::active_manifests(conn, workspace_id)?;
        let context_prompt =
            super::prompt::build_desktop_context_prompt_for_domains(&packet, &active_domains);
        let prompt = format!("{}\n\n{}", agent_preferences.system_prompt, context_prompt);
        (config, prompt, active_domains)
    };
    // Validate and prepare the provider before creating the durable run. A
    // missing credential or malformed model configuration must not leave an
    // active `created` run that the recovery service later treats as work.
    super::session::start_agent_run(conn, workspace_id, conversation_id, run_id, &message)?;
    Ok(ChatPreparation {
        run_id,
        conversation_id,
        message,
        smart_search_enabled: request.smart_search_enabled,
        route,
        prompt,
        config,
        evidence_pack,
        attachments: request.attachments,
        attachment_roots,
        skills,
        active_domains,
        selected_memories,
        unavailable_response,
        agent_preferences,
    })
}

pub fn recover_active_runs_if_allowed(
    connection: &mut Connection,
    workspace_id: &str,
) -> Result<Vec<crate::agent::runtime::RecoveredRun>, String> {
    if !load_agent_preferences(connection, workspace_id)?.allow_recovery {
        return Ok(Vec::new());
    }
    let mut recovery = crate::agent::runtime::AgentRecoveryService::new(connection, workspace_id)?;
    recovery.recover_active(&HashSet::new(), Utc::now())
}

fn load_evidence_pack_reference(
    conn: &Connection,
    workspace_id: &str,
    evidence_pack_id: Option<String>,
) -> Result<Option<EvidencePack>, String> {
    let Some(raw_id) = evidence_pack_id else {
        return Ok(None);
    };
    let audit_id = Uuid::parse_str(raw_id.trim())
        .map_err(|_| "evidence_pack_id must be a UUID".to_string())?;
    load_evidence_pack(conn, workspace_id, audit_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "evidence pack not found".to_string())
        .map(Some)
}

pub fn prepare_summary(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    covered_message_id: Option<&str>,
    secrets: &dyn SecretStore,
) -> Result<Result<SummaryPreparation, SummarizeConversationResponse>, String> {
    if conversation_id.trim().is_empty() {
        return Err("conversation_id is required".to_string());
    }
    let config = super::provider::load_local_llm_config(conn, workspace_id, secrets)?;
    super::provider::validate_local_llm_config(&config)?;
    let (mut messages, latest_summary) =
        super::session::load_summary_state(conn, workspace_id, conversation_id)?;
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
        return Ok(Err(SummarizeConversationResponse {
            summarized: false,
            summary: None,
            covered_message_id: None,
            total_tokens,
            folded_tokens: 0,
        }));
    };
    let (query, contexts) =
        build_summary_prompt(&plan, existing.as_ref().map(|item| item.summary.as_str()));
    Ok(Ok(SummaryPreparation {
        config,
        prompt: super::prompt::build_summary_prompt(&query, &contexts),
        plan,
    }))
}

pub fn append_agent_message(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
    response_json: Option<String>,
) -> Result<(), String> {
    super::session::append_agent_message(
        conn,
        workspace_id,
        conversation_id,
        role,
        content,
        response_json,
    )
}

pub fn save_summary(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    summary: &str,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    super::session::save_summary_for_conversation(
        conn,
        workspace_id,
        conversation_id,
        summary,
        covered_message_id,
    )
}
