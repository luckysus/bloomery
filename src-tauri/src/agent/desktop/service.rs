use super::model::{
    DesktopRoute, LocalAgentChatRequest, LocalAskRequest, LocalLlmConfig,
    SummarizeConversationResponse, SummaryPreparation,
};
use crate::agent::context::{
    build_summary_prompt, estimate_summary_tokens, messages_after_covered_id, plan_summary,
};
use crate::agent::context::{ContextItem, ContextSource};
use crate::agent::runtime::{AgentLoopRequest, ContextEntry, EvidenceAttachment};
use crate::context::build_context_packet_for_connection;
use crate::rag::citation::{load_evidence_pack, EvidencePack};
use crate::skills::SkillContext;
use crate::storage::secrets::SecretStore;
use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;

pub struct ChatPreparation {
    pub run_id: Uuid,
    pub conversation_id: Uuid,
    pub message: String,
    pub route: DesktopRoute,
    pub prompt: String,
    pub config: LocalLlmConfig,
    pub evidence_pack: Option<EvidencePack>,
    pub skills: SkillContext,
    pub active_domain: Option<crate::domains::DomainManifest>,
    pub unavailable_response: Option<Value>,
}

pub struct LocalAskPreparation {
    pub run_id: String,
    pub query: String,
    pub prompt: String,
    pub config: LocalLlmConfig,
}

pub fn build_agent_loop_request(
    assistant_message_id: Uuid,
    prompt: &str,
    message: &str,
    evidence_pack: Option<&EvidencePack>,
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
        evidence: evidence_pack.map(|pack| EvidenceAttachment {
            evidence_pack_id: pack.id,
            citation_numbers: pack
                .evidence
                .iter()
                .map(|item| item.citation_number)
                .collect(),
        }),
    }
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
    let conversation_id = super::session::resolve_conversation(
        conn,
        workspace_id,
        request.session_id.as_deref(),
        &message,
    )?;
    let conversation_id_text = conversation_id.to_string();
    let route = super::routing::classify_desktop_intent(&message);
    let packet = serde_json::to_value(build_context_packet_for_connection(
        conn,
        workspace_id,
        &conversation_id_text,
        &message,
    )?)
    .map_err(|error| error.to_string())?;
    let mut packet = packet;
    packet["desktop_route"] = super::routing::route_to_json(&route);
    let evidence_pack = load_evidence_pack_reference(conn, workspace_id, request.evidence_pack_id)?;
    if let Some(pack) = &evidence_pack {
        packet["evidence_pack"] = serde_json::to_value(pack).map_err(|error| error.to_string())?;
    }
    let skills = crate::skills::load_context(conn, workspace_id, env!("CARGO_PKG_VERSION"))?;
    packet["skills"] = serde_json::json!({
        "enabled_versions": skills.rendered.enabled_versions,
        "prompt": skills.rendered.prompt,
    });
    super::session::start_agent_run(conn, workspace_id, conversation_id, run_id, &message)?;

    let unavailable_response = route.unavailable_capability.map(|_| {
        super::routing::build_capability_unavailable_response_json(
            &run_id.to_string(),
            &conversation_id_text,
            &route,
            &skills.rendered.enabled_versions,
        )
    });
    let (config, prompt, active_domain) = if unavailable_response.is_some() {
        (LocalLlmConfig::default(), String::new(), None)
    } else {
        let config = super::provider::load_local_llm_config(conn, workspace_id, secrets)?;
        super::provider::validate_local_llm_config(&config)?;
        let active_domain =
            crate::storage::repositories::domains::active_manifest(conn, workspace_id)?;
        let prompt = super::prompt::build_desktop_context_prompt(&packet, active_domain.as_ref());
        (config, prompt, active_domain)
    };
    Ok(ChatPreparation {
        run_id,
        conversation_id,
        message,
        route,
        prompt,
        config,
        evidence_pack,
        skills,
        active_domain,
        unavailable_response,
    })
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

pub fn prepare_local_ask(
    conn: &Connection,
    workspace_id: &str,
    request: LocalAskRequest,
    secrets: &dyn SecretStore,
) -> Result<LocalAskPreparation, String> {
    let query = request.query.trim().to_string();
    if query.is_empty() {
        return Err("query is required".to_string());
    }
    let config = super::provider::load_local_llm_config(conn, workspace_id, secrets)?;
    super::provider::validate_local_llm_config(&config)?;
    let mode = request.mode.unwrap_or_else(|| "literature".to_string());
    let run_id = request
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    Ok(LocalAskPreparation {
        run_id,
        prompt: super::prompt::build_local_ask_prompt(&query, &request.contexts, &mode),
        query,
        config,
    })
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
        prompt: super::prompt::build_local_ask_prompt(&query, &contexts, "summary"),
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
