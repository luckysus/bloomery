use crate::agent::desktop::{
    append_agent_message, assistant_content_for_stream_result, build_agent_response_json,
    prepare_chat, prepare_summary, save_summary, LocalAgentChatRequest, LocalAgentState,
    StreamedLlmAnswer,
};
use crate::agent::protocol::{AgentEventData, RunOutcome};
use crate::agent::runtime::{
    AgentLoop, DenyPermissions, DomainToolExecutor, ProviderModelAdapter, SqliteAgentEventSink,
};
use crate::app::desktop_stream::stream_llm_answer;
use crate::db::{current_workspace_id, database_path, with_conn_mut, DbState};
use crate::providers::capabilities::ChatProvider;
use crate::providers::configured_chat_provider;
use crate::steel::SteelToolExecutor;
use serde_json::Value;
use tauri::Emitter;
use uuid::Uuid;

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

    let streamed = run_standard_agent(&app, &agent_state, &preparation, workspace_id).await;
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

async fn run_standard_agent(
    app: &tauri::AppHandle,
    agent_state: &LocalAgentState,
    preparation: &crate::agent::desktop::ChatPreparation,
    workspace_id: &str,
) -> Result<StreamedLlmAnswer, String> {
    let database = database_path(app)?;
    let (mut connection, _) = crate::storage::database::open(&database)
        .map_err(|error| format!("open agent runtime database failed: {error}"))?;
    let (profile, credential) =
        crate::agent::desktop::provider_profile_from_config(&preparation.config)?;
    let provider = configured_chat_provider(profile, credential)
        .map_err(|error| format!("configure local chat provider failed: {error}"))?;
    let tool_calls_enabled = provider.capabilities().tool_calls;
    let model = ProviderModelAdapter::new(provider);
    let steel_tools = SteelToolExecutor::new(tool_calls_enabled);
    let domain_tools = DomainToolExecutor::new(&steel_tools, preparation.active_domain.as_ref());
    let permissions = DenyPermissions;
    let assistant_message_id = Uuid::new_v4();
    let request = crate::agent::desktop::build_agent_loop_request(
        assistant_message_id,
        &preparation.prompt,
        &preparation.message,
        preparation.evidence_pack.as_ref(),
    );
    let run_id = preparation.run_id;
    let run_id_text = run_id.to_string();
    let app_for_events = app.clone();
    let mut publisher = move |event: &crate::agent::protocol::AgentEventEnvelope| {
        let _ = app_for_events.emit("agent-event", event);
        if let AgentEventData::MessageDelta(delta) = &event.data {
            let _ = app_for_events.emit(
                "desktop-agent-delta",
                crate::agent::desktop::LocalAgentDelta {
                    run_id: run_id_text.clone(),
                    delta: delta.delta.clone(),
                },
            );
        }
        Ok(())
    };
    let mut sink = SqliteAgentEventSink::new(&mut connection, workspace_id, run_id, &mut publisher);
    let result = AgentLoop::new(&model, &domain_tools, &permissions)
        .run(
            request,
            &mut sink,
            agent_state.cancellation_token(&run_id.to_string()),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(StreamedLlmAnswer {
        text: result.answer,
        stopped: result.outcome == RunOutcome::Cancelled,
    })
}
