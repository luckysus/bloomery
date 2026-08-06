use crate::agent::desktop::{
    append_agent_message, assistant_content_for_stream_result, build_agent_response_json,
    prepare_chat, prepare_summary, save_summary, LocalAgentChatRequest, LocalAgentState,
};
use crate::app::desktop_stream::stream_llm_answer;
use crate::db::{current_workspace_id, with_conn_mut, DbState};
use serde_json::Value;

#[tauri::command]
pub async fn desktop_agent_chat(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: LocalAgentChatRequest,
) -> Result<Value, String> {
    let workspace_id = current_workspace_id();
    let preparation = with_conn_mut(&db, |conn| prepare_chat(conn, workspace_id, request))?;
    let run_id = preparation.run_id.to_string();
    let conversation_id = preparation.conversation_id.to_string();

    if let Some(response) = preparation.unavailable_response {
        let answer = response["answer"]
            .as_str()
            .unwrap_or("Local capability unavailable.");
        with_conn_mut(&db, |conn| {
            append_agent_message(
                conn,
                workspace_id,
                &conversation_id,
                "agent",
                answer,
                Some(response.to_string()),
            )
        })?;
        return Ok(response);
    }

    let streamed = stream_llm_answer(
        &app,
        &agent_state,
        "desktop-agent-delta",
        &run_id,
        &preparation.config,
        &preparation.prompt,
        &preparation.message,
    )
    .await;
    let streamed = match streamed {
        Ok(answer) => answer,
        Err(error) => {
            agent_state.clear_cancelled(&run_id);
            return Err(error);
        }
    };
    let answer = assistant_content_for_stream_result(&streamed);
    let mut response = build_agent_response_json(
        &run_id,
        &conversation_id,
        &answer,
        &preparation.config.provider,
        &preparation.config.model_name,
        !preparation.config.api_key.trim().is_empty(),
        &preparation.route,
        preparation.evidence_pack.as_ref(),
        &preparation.skills.rendered.enabled_versions,
    );
    if streamed.stopped {
        response["status"] = Value::String("cancelled".to_string());
        response["workflow"]["state"] = Value::String("cancelled".to_string());
    }
    with_conn_mut(&db, |conn| {
        append_agent_message(
            conn,
            workspace_id,
            &conversation_id,
            "agent",
            &answer,
            Some(response.to_string()),
        )
    })?;

    if !streamed.stopped {
        let summary = with_conn_mut(&db, |conn| {
            prepare_summary(conn, workspace_id, &conversation_id, None)
        });
        if let Ok(Ok(summary)) = summary {
            if let Ok(summary_answer) = stream_llm_answer(
                &app,
                &agent_state,
                "desktop-ask-delta",
                &run_id,
                &summary.config,
                &summary.prompt,
                "summarize",
            )
            .await
            {
                if !summary_answer.stopped && !summary_answer.text.trim().is_empty() {
                    let _ = with_conn_mut(&db, |conn| {
                        save_summary(
                            conn,
                            workspace_id,
                            &conversation_id,
                            summary_answer.text.trim(),
                            Some(summary.plan.covered_message_id.clone()),
                        )
                    });
                }
            }
        }
    }
    agent_state.clear_cancelled(&run_id);
    Ok(response)
}
