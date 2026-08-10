use crate::agent::desktop::{LocalAgentState, StreamedLlmAnswer};
use crate::agent::protocol::{AgentEventData, RunOutcome};
use crate::agent::runtime::{
    AgentLoop, CompositeToolExecutor, DomainToolExecutor, ProviderModelAdapter,
    SqliteAgentEventSink,
};
use crate::app::mcp_agent_runtime::load_enabled_tools;
use crate::db::database_path;
use crate::permissions::{ParameterScope, RuleEffect};
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
    let persistent_permission_keys =
        crate::storage::repositories::permissions::list(&connection, workspace_id)
            .map_err(|error| format!("load permission rules failed: {error}"))?
            .into_iter()
            .filter_map(|rule| {
                if rule.effect != RuleEffect::Allow {
                    return None;
                }
                match rule.scope {
                    ParameterScope::Exact(arguments) => {
                        Some(crate::agent::desktop::permission_key_for(
                            rule.tool_id.as_str(),
                            &arguments,
                        ))
                    }
                    ParameterScope::Any | ParameterScope::Fields(_) => None,
                }
            });
    agent_state.load_always_permission_keys(persistent_permission_keys);
    let (profile, credential) =
        crate::agent::desktop::provider_profile_from_config(&preparation.config)?;
    let provider = configured_chat_provider(profile, credential)
        .map_err(|error| format!("configure local chat provider failed: {error}"))?;
    let tool_calls_enabled = provider.capabilities().tool_calls;
    let model = ProviderModelAdapter::new(provider);
    let steel_tools = SteelToolExecutor::new(tool_calls_enabled);
    let mcp_configs = crate::storage::repositories::mcp::list(&connection, workspace_id)
        .map_err(|error| format!("load MCP configurations failed: {error}"))?;
    let mcp_tools = load_enabled_tools(app, mcp_configs).await?;
    let combined_tools = CompositeToolExecutor::try_new(vec![&steel_tools, &mcp_tools])
        .map_err(|error| format!("combine Agent tools failed: {error}"))?;
    let domain_tools = DomainToolExecutor::new(&combined_tools, preparation.active_domain.as_ref());
    let permissions = agent_state.permission_resolver();
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
