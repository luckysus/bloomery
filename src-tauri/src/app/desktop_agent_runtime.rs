use crate::agent::desktop::{LocalAgentState, StreamedLlmAnswer};
use crate::agent::protocol::{AgentEventData, RunOutcome};
use crate::agent::runtime::{
    AgentLoop, CompositeToolExecutor, DomainToolExecutor, ProviderModelAdapter,
    SqliteAgentEventSink,
};
use crate::app::mcp_agent_runtime::load_enabled_tools_for_query;
use crate::db::database_path;
use crate::permissions::{ParameterScope, RuleEffect};
use crate::providers::capabilities::ChatProvider;
use crate::providers::configured_chat_provider;
use crate::steel::SteelToolExecutor;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
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
    let optimization_gateway = std::sync::Arc::new(
        crate::app::compute_commands::gateway::DesktopOptimizationGateway::new(database.clone()),
    );
    let steel_agent_gateway = std::sync::Arc::new(
        crate::app::steel_agent_gateway::DesktopSteelAgentGateway::new(
            database.clone(),
            workspace_id,
        ),
    );
    let steel_tools = SteelToolExecutor::with_agent_gateways(
        optimization_gateway,
        steel_agent_gateway,
        tool_calls_enabled,
    );
    let mcp_configs = crate::storage::repositories::mcp::list(&connection, workspace_id)
        .map_err(|error| format!("load MCP configurations failed: {error}"))?;
    let mcp_tools = load_enabled_tools_for_query(app, mcp_configs, &preparation.message).await?;
    let combined_tools = CompositeToolExecutor::try_new(vec![&steel_tools, &mcp_tools])
        .map_err(|error| format!("combine Agent tools failed: {error}"))?;
    let domain_tools =
        DomainToolExecutor::new_for_domains(&combined_tools, &preparation.active_domains);
    let permissions = agent_state.permission_resolver();
    let assistant_message_id = Uuid::new_v4();
    let request = crate::agent::desktop::build_agent_loop_request_with_attachments(
        assistant_message_id,
        &preparation.prompt,
        &preparation.message,
        preparation.evidence_pack.as_ref(),
        &preparation.attachments,
    );
    let run_id = preparation.run_id;
    let run_id_text = run_id.to_string();
    let app_for_events = app.clone();
    let tool_call_audit = Arc::new(Mutex::new(Vec::new()));
    let tool_call_audit_for_events = Arc::clone(&tool_call_audit);
    let mut publisher = move |event: &crate::agent::protocol::AgentEventEnvelope| {
        if let Ok(mut tool_calls) = tool_call_audit_for_events.lock() {
            capture_tool_call_audit(&mut tool_calls, &event.data);
        }
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
    let tool_calls = tool_call_audit
        .lock()
        .map(|calls| calls.clone())
        .unwrap_or_default();
    Ok(StreamedLlmAnswer {
        text: result.answer,
        stopped: result.outcome == RunOutcome::Cancelled,
        tool_calls,
    })
}

fn capture_tool_call_audit(tool_calls: &mut Vec<Value>, data: &AgentEventData) {
    match data {
        AgentEventData::ToolRequested(event) => tool_calls.push(json!({
            "id": event.tool_call_id,
            "tool_id": event.tool_id,
            "name": event.tool_name,
            "status": "requested",
        })),
        AgentEventData::ToolStarted(event) => {
            update_tool_call(tool_calls, event.tool_call_id, |value| {
                value["status"] = json!("running");
            })
        }
        AgentEventData::ToolCompleted(event) => {
            let status = match event.outcome {
                crate::agent::protocol::ToolOutcome::Succeeded => "succeeded",
                crate::agent::protocol::ToolOutcome::Failed => "failed",
                crate::agent::protocol::ToolOutcome::Cancelled => "cancelled",
            };
            update_tool_call(tool_calls, event.tool_call_id, |value| {
                value["status"] = json!(status);
                if let Some(error) = &event.error {
                    value["error_code"] = json!(error.code);
                    value["error_message"] = json!(error.message);
                }
            });
        }
        _ => {}
    }
}

fn update_tool_call(
    tool_calls: &mut Vec<Value>,
    tool_call_id: Uuid,
    update: impl FnOnce(&mut Value),
) {
    if let Some(value) = tool_calls
        .iter_mut()
        .find(|value| value["id"] == tool_call_id.to_string())
    {
        update(value);
        return;
    }
    let mut value = json!({"id": tool_call_id, "status": "unknown"});
    update(&mut value);
    tool_calls.push(value);
}

#[cfg(test)]
mod tests {
    use crate::agent::protocol::{AgentEventData, ToolCompleted, ToolOutcome, ToolRequested};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn captures_tool_call_audit_from_agent_events() {
        let call_id = Uuid::new_v4();
        let mut tool_calls = Vec::new();

        super::capture_tool_call_audit(
            &mut tool_calls,
            &AgentEventData::ToolRequested(ToolRequested {
                tool_call_id: call_id,
                tool_id: "steel.search_literature".to_string(),
                tool_name: "search_literature".to_string(),
                arguments: json!({"query": "Q355B"}),
            }),
        );
        super::capture_tool_call_audit(
            &mut tool_calls,
            &AgentEventData::ToolCompleted(ToolCompleted {
                tool_call_id: call_id,
                outcome: ToolOutcome::Succeeded,
                output: Some(json!({"items": 1})),
                error: None,
            }),
        );

        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], call_id.to_string());
        assert_eq!(tool_calls[0]["name"], "search_literature");
        assert_eq!(tool_calls[0]["status"], "succeeded");
        assert!(tool_calls[0].get("output").is_none());
    }
}
