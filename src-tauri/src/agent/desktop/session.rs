use super::model::ExistingSummary;
use crate::agent::context::SummaryMessage;
use crate::agent::session::model::StartRunRequest;
use crate::agent::session::service::SessionService;
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

pub fn resolve_conversation(
    conn: &mut Connection,
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
    let conversation =
        session.create_conversation(&super::prompt::conversation_title(first_message))?;
    Uuid::parse_str(&conversation.id).map_err(|_| "created conversation id is invalid".to_string())
}

pub fn start_agent_run(
    conn: &mut Connection,
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

pub fn append_agent_message(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
    response_json: Option<String>,
) -> Result<(), String> {
    SessionService::new(conn, workspace_id)?
        .append_message(conversation_id, role, content, response_json)
        .map(|_| ())
}

pub fn load_summary_state(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<(Vec<SummaryMessage>, Option<ExistingSummary>), String> {
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
}

pub fn save_summary_for_conversation(
    conn: &mut Connection,
    workspace_id: &str,
    conversation_id: &str,
    summary: &str,
    covered_message_id: Option<String>,
) -> Result<(), String> {
    SessionService::new(conn, workspace_id)?.save_summary(
        conversation_id,
        summary,
        covered_message_id,
    )
}
