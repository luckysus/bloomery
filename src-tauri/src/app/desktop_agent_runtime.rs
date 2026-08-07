use crate::agent::desktop::{LocalAgentState, StreamedLlmAnswer};
use crate::agent::protocol::{AgentEventData, RunOutcome};
use crate::agent::runtime::{
    AgentLoop, DenyPermissions, DomainToolExecutor, ProviderModelAdapter, SqliteAgentEventSink,
};
use crate::db::database_path;
use crate::providers::capabilities::ChatProvider;
use crate::providers::configured_chat_provider;
use crate::steel::SteelToolExecutor;
use tauri::Emitter;
use uuid::Uuid;

pub(crate) async fn run_standard_agent(
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
