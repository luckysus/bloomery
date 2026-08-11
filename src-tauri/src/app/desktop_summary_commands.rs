use crate::agent::desktop::{
    assistant_content_for_stream_result, prepare_summary, save_summary, LocalAgentState,
    SummarizeConversationRequest, SummarizeConversationResponse,
};
use crate::app::desktop_stream::stream_llm_answer;
use crate::db::{current_workspace_id, with_conn_mut, DbState};
use crate::storage::secrets::SecretState;

#[tauri::command]
pub async fn desktop_summarize_conversation(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: SummarizeConversationRequest,
) -> Result<SummarizeConversationResponse, String> {
    let workspace_id = current_workspace_id();
    let run_id = request
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let conversation_id = request.conversation_id.trim().to_string();
    let prepared = with_conn_mut(&db, |conn| {
        prepare_summary(
            conn,
            workspace_id,
            &conversation_id,
            request.covered_message_id.as_deref(),
            secrets.store(),
        )
    })?;
    let Ok(prepared) = prepared else {
        return Ok(prepared.err().expect("summary result is an error"));
    };
    let answer = stream_llm_answer(
        &app,
        &agent_state,
        "desktop-ask-delta",
        &run_id,
        &prepared.config,
        &prepared.prompt,
        "summarize",
    )
    .await;
    agent_state.clear_cancelled(&run_id);
    let answer = answer?;
    if answer.stopped {
        return Ok(SummarizeConversationResponse {
            summarized: false,
            summary: None,
            covered_message_id: None,
            total_tokens: prepared.plan.total_tokens,
            folded_tokens: prepared.plan.folded_tokens,
        });
    }
    let summary = assistant_content_for_stream_result(&answer)
        .trim()
        .to_string();
    if summary.is_empty() {
        return Ok(SummarizeConversationResponse {
            summarized: false,
            summary: None,
            covered_message_id: Some(prepared.plan.covered_message_id),
            total_tokens: prepared.plan.total_tokens,
            folded_tokens: prepared.plan.folded_tokens,
        });
    }
    with_conn_mut(&db, |conn| {
        save_summary(
            conn,
            workspace_id,
            &conversation_id,
            &summary,
            Some(prepared.plan.covered_message_id.clone()),
        )
    })?;
    Ok(SummarizeConversationResponse {
        summarized: true,
        summary: Some(summary),
        covered_message_id: Some(prepared.plan.covered_message_id),
        total_tokens: prepared.plan.total_tokens,
        folded_tokens: prepared.plan.folded_tokens,
    })
}
