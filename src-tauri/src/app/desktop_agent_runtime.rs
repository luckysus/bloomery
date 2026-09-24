use crate::agent::desktop::StreamedLlmAnswer;
use crate::agent::protocol::{AgentEventData, AgentRunState, RunOutcome};
use crate::agent::runtime::{
    AgentContextCheckpoint, AgentLoop, AgentLoopResume, CompositeToolExecutor, DomainToolExecutor,
    ModelAdapter, PermissionFuture, ProviderModelAdapter, ResumableToolCall, RuntimeHost,
    SkillTool, SnapshotToolExecutor, SqliteAgentEventSink, SqliteChildTurnStore, SubagentTool,
    TodoTracker, TurnSnapshot,
};
use crate::app::mcp_agent_runtime::load_enabled_tools_for_query;
use crate::db::database_path;
use crate::permissions::{ParameterScope, RuleEffect};
use crate::providers::capabilities::ChatProvider;
use crate::providers::configured_chat_provider;
use crate::steel::SteelToolExecutor;
use crate::tasks::mailbox::MailboxStore;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tauri::Manager;
use uuid::Uuid;

fn should_load_agent_tools(smart_search_enabled: bool, has_evidence_pack: bool) -> bool {
    smart_search_enabled && has_evidence_pack
}

pub(crate) async fn run_standard_agent(
    app: &tauri::AppHandle,
    agent_state: &RuntimeHost,
    preparation: &crate::agent::desktop::ChatPreparation,
    workspace_id: &str,
) -> Result<StreamedLlmAnswer, String> {
    run_standard_agent_inner(
        app,
        agent_state,
        preparation,
        workspace_id,
        None,
        None,
        None,
    )
    .await
}

pub(crate) async fn resume_standard_agent(
    app: &tauri::AppHandle,
    agent_state: &RuntimeHost,
    preparation: &crate::agent::desktop::ChatPreparation,
    workspace_id: &str,
    snapshot: crate::storage::repositories::turn_snapshots::AgentTurnSnapshot,
    checkpoint: AgentContextCheckpoint,
    state: AgentRunState,
    assistant_result_recorded: bool,
    assistant_message_id: Uuid,
) -> Result<StreamedLlmAnswer, String> {
    resume_standard_agent_with_tools(
        app,
        agent_state,
        preparation,
        workspace_id,
        snapshot,
        checkpoint,
        state,
        assistant_result_recorded,
        assistant_message_id,
        Vec::new(),
    )
    .await
}

pub(crate) async fn resume_standard_agent_with_tools(
    app: &tauri::AppHandle,
    agent_state: &RuntimeHost,
    preparation: &crate::agent::desktop::ChatPreparation,
    workspace_id: &str,
    snapshot: crate::storage::repositories::turn_snapshots::AgentTurnSnapshot,
    checkpoint: AgentContextCheckpoint,
    state: AgentRunState,
    assistant_result_recorded: bool,
    assistant_message_id: Uuid,
    pending_tools: Vec<ResumableToolCall>,
) -> Result<StreamedLlmAnswer, String> {
    let resume = AgentLoopResume {
        checkpoint,
        state,
        assistant_result_recorded,
        pending_tools,
    };
    run_standard_agent_inner(
        app,
        agent_state,
        preparation,
        workspace_id,
        Some(snapshot),
        Some(resume),
        Some(assistant_message_id),
    )
    .await
}

async fn run_standard_agent_inner(
    app: &tauri::AppHandle,
    agent_state: &RuntimeHost,
    preparation: &crate::agent::desktop::ChatPreparation,
    workspace_id: &str,
    persisted_snapshot: Option<crate::storage::repositories::turn_snapshots::AgentTurnSnapshot>,
    resume: Option<AgentLoopResume>,
    assistant_message_id: Option<Uuid>,
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
    let model = Arc::new(ProviderModelAdapter::new(provider));
    let retrieval_tools_enabled = should_load_agent_tools(
        preparation.smart_search_enabled,
        preparation.evidence_pack.is_some(),
    );
    let steel_tools = if retrieval_tools_enabled {
        let optimization_gateway = std::sync::Arc::new(
            crate::app::compute_commands::gateway::DesktopOptimizationGateway::new(
                database.clone(),
            ),
        );
        let mut steel_agent_gateway =
            crate::app::steel_agent_gateway::DesktopSteelAgentGateway::new(
                database.clone(),
                workspace_id,
            );
        if let Ok(pool) = crate::knowledge_db::pool_for_query(
            app.state::<crate::knowledge_db::KnowledgeDatabaseState>()
                .inner(),
        ) {
            steel_agent_gateway = steel_agent_gateway.with_postgres_pool(app.clone(), pool);
        }
        let steel_agent_gateway = std::sync::Arc::new(steel_agent_gateway);
        SteelToolExecutor::with_agent_gateways(
            optimization_gateway,
            steel_agent_gateway,
            tool_calls_enabled,
        )
    } else {
        SteelToolExecutor::new(false)
    };
    let todo_tracker = Arc::new(TodoTracker::default());
    let skill_tool = SkillTool::default();
    let mcp_configs = crate::storage::repositories::mcp::list(&connection, workspace_id)
        .map_err(|error| format!("load MCP configurations failed: {error}"))?;
    let mcp_tools = if tool_calls_enabled {
        load_enabled_tools_for_query(app, mcp_configs, &preparation.message).await?
    } else {
        crate::mcp::McpToolExecutor::from_bindings(Vec::new())
            .map_err(|error| format!("create empty MCP tool set failed: {error}"))?
    };
    let background_tasks = if tool_calls_enabled {
        Some(
            crate::agent::runtime::BackgroundTasksTool::from_connection(&connection, workspace_id)
                .map_err(|error| format!("configure background task query failed: {error}"))?,
        )
    } else {
        None
    };
    let mut tool_sources: Vec<&dyn crate::agent::runtime::ToolExecutor> =
        vec![&steel_tools, &mcp_tools, todo_tracker.as_ref(), &skill_tool];
    if let Some(tasks) = &background_tasks {
        tool_sources.push(tasks);
    }
    let combined_tools = CompositeToolExecutor::try_new(tool_sources)
        .map_err(|error| format!("combine Agent tools failed: {error}"))?;
    let domain_tools =
        DomainToolExecutor::new_for_domains(&combined_tools, &preparation.active_domains);
    let child_tools: Arc<dyn crate::agent::runtime::ToolExecutor> =
        Arc::new(SnapshotToolExecutor::from(&domain_tools));
    let permissions: Arc<dyn crate::agent::runtime::PermissionResolver> =
        Arc::new(agent_state.permission_resolver());
    let subagent_hooks: Arc<dyn crate::agent::runtime::AgentHooks> = todo_tracker.clone();
    let child_store = Arc::new(SqliteChildTurnStore::new(
        database.clone(),
        workspace_id.to_string(),
    ));
    let subagent = SubagentTool::new_for_parent_with_store(
        model.clone(),
        child_tools,
        permissions.clone(),
        subagent_hooks,
        agent_state.clone(),
        preparation.run_id,
        child_store,
    );
    let parent_tools = CompositeToolExecutor::try_new(vec![&domain_tools, &subagent])
        .map_err(|error| format!("combine subagent tools failed: {error}"))?;
    let assistant_message_id = assistant_message_id.unwrap_or_else(Uuid::new_v4);
    let mailbox = MailboxStore::new(
        database
            .parent()
            .map(|parent| parent.join(".agent").join("mailboxes"))
            .ok_or_else(|| "agent mailbox root is unavailable".to_string())?,
    )
    .map_err(|error| format!("create agent mailbox failed: {error}"))?;
    let mailbox_message = if resume.is_none() {
        mailbox
            .claim("lead")
            .map_err(|error| format!("claim lead mailbox failed: {error}"))?
    } else {
        None
    };
    let mut request = crate::agent::desktop::build_agent_loop_request_with_attachments(
        assistant_message_id,
        &preparation.prompt,
        &preparation.message,
        preparation.evidence_pack.as_ref(),
        &preparation.attachments,
    );
    if let Some(message) = mailbox_message.as_ref() {
        crate::agent::desktop::add_mailbox_context(&mut request, message);
    }
    let run_id = preparation.run_id;
    if let Some(resume) = &resume {
        request.output_reservation = persisted_snapshot
            .as_ref()
            .map(|snapshot| snapshot.turn.output_reservation)
            .unwrap_or(request.output_reservation);
        request.limits = persisted_snapshot
            .as_ref()
            .map(|snapshot| snapshot.turn.limits.clone())
            .unwrap_or_else(|| request.limits.clone());
        request.resume = Some(resume.clone());
    }
    let initial_snapshot = persisted_snapshot
        .as_ref()
        .map(|snapshot| snapshot.turn.clone())
        .unwrap_or_else(|| TurnSnapshot {
            turn_id: run_id,
            session_id: preparation.conversation_id,
            parent_turn_id: None,
            child_turn_limit: 4,
            provider: preparation.config.provider.clone(),
            model: preparation.config.model_name.clone(),
            model_context_window: model.capabilities().context_window,
            output_reservation: request.output_reservation,
            limits: request.limits.clone(),
            tool_ids: Vec::new(),
            tool_snapshot: Vec::new(),
        });
    if initial_snapshot.turn_id != run_id {
        return Err("agent turn snapshot does not match the requested run".to_string());
    }
    if let (Some(expected), Some(actual)) = (
        initial_snapshot.model_context_window,
        model.capabilities().context_window,
    ) {
        if expected != actual {
            return Err("agent model context window changed since the turn started".to_string());
        }
    }
    let turn = agent_state.begin_turn(initial_snapshot)?;
    request.input_queue = turn.input_queue;
    let updated_snapshot = match agent_state.set_tool_snapshot(run_id, &parent_tools) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            agent_state.finish_turn(run_id);
            if let Some(message) = mailbox_message.as_ref() {
                let _ = mailbox.release(message);
            }
            return Err(error);
        }
    };
    if let Some(expected) = persisted_snapshot.as_ref() {
        let tools_match = if expected.turn.tool_snapshot.is_empty() {
            expected.turn.tool_ids == updated_snapshot.tool_ids
        } else {
            expected.turn.tool_snapshot == updated_snapshot.tool_snapshot
        };
        if !tools_match {
            agent_state.finish_turn(run_id);
            return Err("agent tool snapshot changed since the turn started".to_string());
        }
    }
    crate::storage::repositories::turn_snapshots::save(
        &connection,
        workspace_id,
        run_id,
        &crate::storage::repositories::turn_snapshots::AgentTurnSnapshot {
            turn: updated_snapshot,
            assistant_message_id,
            provider_base_url: preparation.config.base_url.clone(),
            smart_search_enabled: preparation.smart_search_enabled,
            evidence_pack_id: preparation.evidence_pack.as_ref().map(|pack| pack.id),
        },
        chrono::Utc::now(),
    )
    .map_err(|error| {
        agent_state.finish_turn(run_id);
        error.to_string()
    })?;
    let app_for_events = app.clone();
    let tool_call_audit = Arc::new(Mutex::new(Vec::new()));
    let tool_call_audit_for_events = Arc::clone(&tool_call_audit);
    let mut publisher = move |event: &crate::agent::protocol::AgentEventEnvelope| {
        if let Ok(mut tool_calls) = tool_call_audit_for_events.lock() {
            capture_tool_call_audit(&mut tool_calls, &event.data);
        }
        let _ = app_for_events.emit("agent-event", event);
        Ok(())
    };
    let mut sink = SqliteAgentEventSink::new(&mut connection, workspace_id, run_id, &mut publisher);
    let artifact_root = database
        .parent()
        .map(|parent| parent.join(".agent").join("artifacts"))
        .ok_or_else(|| "agent artifact workspace is unavailable".to_string())?;
    let artifact_store = crate::tools::FileArtifactStore::new(PathBuf::from(artifact_root))
        .map_err(|error| format!("create agent artifact store failed: {error}"))?;
    let result = AgentLoop::new_with_hooks_and_artifact_store(
        model.as_ref(),
        &parent_tools,
        permissions.as_ref(),
        todo_tracker.as_ref(),
        &artifact_store,
    )
    .run(request, &mut sink, agent_state.cancellation_token(run_id))
    .await;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            agent_state.finish_turn(run_id);
            if let Some(message) = mailbox_message.as_ref() {
                let _ = mailbox.release(message);
            }
            return Err(error.to_string());
        }
    };
    agent_state.finish_turn(run_id);
    if let Some(message) = mailbox_message.as_ref() {
        mailbox
            .ack(message)
            .map_err(|error| format!("ack lead mailbox failed: {error}"))?;
    }
    let tool_calls = tool_call_audit
        .lock()
        .map(|calls| calls.clone())
        .unwrap_or_default();
    Ok(StreamedLlmAnswer {
        text: result.answer,
        reasoning: result.reasoning,
        reasoning_ms: result.reasoning_ms,
        stopped: result.outcome == RunOutcome::Cancelled,
        tool_calls,
    })
}

/// Rebuild a desktop preparation from durable run metadata and continue the
/// same Runtime-owned Turn after an application restart.
pub(crate) async fn resume_recovered_agent(
    app: &tauri::AppHandle,
    agent_state: &RuntimeHost,
    workspace_id: &str,
    recovered: crate::agent::runtime::RecoveredRun,
) -> Result<(), String> {
    resume_recovered_agent_inner(app, agent_state, workspace_id, recovered, Vec::new()).await
}

pub(crate) async fn resume_recovered_agent_with_permissions(
    app: &tauri::AppHandle,
    agent_state: &RuntimeHost,
    workspace_id: &str,
    recovered: crate::agent::runtime::RecoveredRun,
    waits: Vec<PermissionFuture>,
) -> Result<(), String> {
    resume_recovered_agent_inner(app, agent_state, workspace_id, recovered, waits).await
}

pub(crate) fn restore_recovered_permissions(
    agent_state: &RuntimeHost,
    recovered: &crate::agent::runtime::RecoveredRun,
) -> Result<Option<Vec<PermissionFuture>>, String> {
    let crate::agent::runtime::RecoveryAction::AwaitPermissions(permissions) = &recovered.action
    else {
        return Ok(None);
    };
    if permissions
        .iter()
        .any(|permission| agent_state.has_pending_permission(permission.permission_id))
    {
        return Ok(None);
    }
    let cancellation = agent_state.cancellation_token(recovered.run.id);
    permissions
        .iter()
        .map(|permission| {
            agent_state.restore_permission(
                crate::agent::runtime::PermissionRequest {
                    permission_id: permission.permission_id,
                    tool_call_id: permission.tool_call_id,
                    tool_id: permission.tool_id.clone(),
                    tool_name: permission.tool_name.clone(),
                    risk: permission.risk,
                    arguments: permission.arguments.clone(),
                },
                cancellation.clone(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

async fn resume_recovered_agent_inner(
    app: &tauri::AppHandle,
    agent_state: &RuntimeHost,
    workspace_id: &str,
    recovered: crate::agent::runtime::RecoveredRun,
    waits: Vec<PermissionFuture>,
) -> Result<(), String> {
    let action = recovered.action.clone();
    let permission_decisions = match &action {
        crate::agent::runtime::RecoveryAction::AwaitPermissions(permissions) => {
            if permissions.len() != waits.len() {
                return Err(
                    "recovered permission wait count does not match persisted requests".to_string(),
                );
            }
            let mut decisions = HashMap::new();
            for (permission, wait) in permissions.iter().zip(waits) {
                decisions.insert(permission.permission_id, wait.await);
            }
            Some(decisions)
        }
        _ if !waits.is_empty() => {
            return Err("permission waits were supplied for a non-permission recovery".to_string());
        }
        _ => None,
    };
    if agent_state.is_cancelled(&recovered.run.id.to_string())? {
        return Ok(());
    }
    if matches!(action, crate::agent::runtime::RecoveryAction::Regenerate) {
        return Ok(());
    }

    let run = recovered.run;
    let database = database_path(app)?;
    let (mut connection, _) = crate::storage::database::open(&database)
        .map_err(|error| format!("open recovery database failed: {error}"))?;
    let snapshot =
        crate::storage::repositories::turn_snapshots::get(&connection, workspace_id, run.id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "agent turn snapshot is missing".to_string())?;
    if snapshot.turn.session_id != run.conversation_id {
        return Err("agent turn snapshot conversation does not match the run".to_string());
    }
    let replay = crate::storage::repositories::events::replay(&connection, workspace_id, run.id, 0)
        .map_err(|error| error.to_string())?;
    let checkpoint = match &action {
        crate::agent::runtime::RecoveryAction::ResumeFromCheckpoint(checkpoint) => {
            checkpoint.clone()
        }
        crate::agent::runtime::RecoveryAction::AwaitPermissions(_)
        | crate::agent::runtime::RecoveryAction::ResumeTools(_) => {
            crate::storage::repositories::checkpoints::get(&connection, workspace_id, run.id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "recovered tool turn checkpoint is missing".to_string())?
        }
        crate::agent::runtime::RecoveryAction::Regenerate => return Ok(()),
    };
    let assistant_result_recorded = replay.iter().any(|event| {
        matches!(
            &event.data,
            crate::agent::protocol::AgentEventData::MessageCompleted(message)
                if message.message_id == snapshot.assistant_message_id
        )
    });
    let pending_tools = match &action {
        crate::agent::runtime::RecoveryAction::ResumeTools(tools) => tools.clone(),
        crate::agent::runtime::RecoveryAction::AwaitPermissions(_) => {
            pending_tool_checkpoints(&replay)
        }
        crate::agent::runtime::RecoveryAction::ResumeFromCheckpoint(_)
        | crate::agent::runtime::RecoveryAction::Regenerate => Vec::new(),
    };
    let permissions_by_tool = match &action {
        crate::agent::runtime::RecoveryAction::AwaitPermissions(permissions) => permissions
            .iter()
            .map(|permission| (permission.tool_call_id, permission))
            .collect::<HashMap<_, _>>(),
        _ => HashMap::new(),
    };
    let resumable_tools = pending_tools
        .into_iter()
        .map(|tool| {
            let permission = permissions_by_tool.get(&tool.tool_call_id);
            ResumableToolCall {
                tool_call_id: tool.tool_call_id,
                tool_id: tool.tool_id,
                tool_name: tool.tool_name,
                arguments: tool.arguments,
                permission_id: permission.map(|permission| permission.permission_id),
                decision: permission
                    .and_then(|permission| {
                        permission_decisions
                            .as_ref()?
                            .get(&permission.permission_id)
                    })
                    .copied(),
            }
        })
        .collect::<Vec<_>>();
    let message = connection
        .query_row(
            "SELECT content FROM messages
             WHERE workspace_id = ?1 AND conversation_id = ?2 AND id = ?3 AND role = 'user'",
            rusqlite::params![
                workspace_id,
                run.conversation_id.to_string(),
                run.user_message_id.to_string()
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| format!("load recovered user message failed: {error}"))?;
    let evidence_pack = snapshot
        .evidence_pack_id
        .map(|id| {
            crate::rag::citation::load_evidence_pack(&connection, workspace_id, id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "recovered evidence pack is missing".to_string())
        })
        .transpose()?;
    let mut config = crate::agent::desktop::load_local_llm_config(
        &connection,
        workspace_id,
        app.state::<crate::storage::secrets::SecretState>().store(),
    )?;
    config.provider = snapshot.turn.provider.clone();
    config.model_name = snapshot.turn.model.clone();
    if !snapshot.provider_base_url.trim().is_empty() {
        config.base_url = snapshot.provider_base_url.clone();
    }
    let active_domains =
        crate::storage::repositories::domains::active_manifests(&connection, workspace_id)?;
    let preparation = crate::agent::desktop::ChatPreparation {
        run_id: run.id,
        conversation_id: run.conversation_id,
        message,
        smart_search_enabled: snapshot.smart_search_enabled,
        route: crate::agent::desktop::DesktopRoute {
            intent: crate::agent::desktop::DesktopIntentKind::LocalQa,
            confidence: 0.0,
            reason: "recovered agent turn",
            unavailable_capability: None,
        },
        prompt: String::new(),
        config,
        evidence_pack,
        attachments: Vec::new(),
        skills: crate::skills::SkillContext::default(),
        active_domains,
        selected_memories: Vec::new(),
        unavailable_response: None,
    };
    let result = if resumable_tools.is_empty() {
        resume_standard_agent(
            app,
            agent_state,
            &preparation,
            workspace_id,
            snapshot.clone(),
            checkpoint,
            run.state,
            assistant_result_recorded,
            snapshot.assistant_message_id,
        )
        .await
    } else {
        resume_standard_agent_with_tools(
            app,
            agent_state,
            &preparation,
            workspace_id,
            snapshot.clone(),
            checkpoint,
            run.state,
            assistant_result_recorded,
            snapshot.assistant_message_id,
            resumable_tools,
        )
        .await
    };
    let answer = match result {
        Ok(answer) => answer,
        Err(error) => {
            let mut recovery =
                crate::agent::runtime::AgentRecoveryService::new(&mut connection, workspace_id)?;
            recovery.fail(
                run.id,
                format!("startup recovery failed: {error}"),
                chrono::Utc::now(),
            )?;
            return Err(error);
        }
    };
    let content = crate::agent::desktop::assistant_content_for_stream_result(&answer);
    let response = json!({
        "status": if answer.stopped { "cancelled" } else { "completed" },
        "recovered": true,
        "run_id": run.id,
    });
    crate::agent::desktop::append_agent_message(
        &mut connection,
        workspace_id,
        &run.conversation_id.to_string(),
        "agent",
        &content,
        Some(response.to_string()),
    )?;
    Ok(())
}

fn pending_tool_checkpoints(
    events: &[crate::agent::protocol::AgentEventEnvelope],
) -> Vec<crate::agent::runtime::ToolCheckpoint> {
    let mut pending = Vec::new();
    for event in events {
        match &event.data {
            AgentEventData::ToolRequested(tool) => {
                pending.push(crate::agent::runtime::ToolCheckpoint {
                    tool_call_id: tool.tool_call_id,
                    tool_id: tool.tool_id.clone(),
                    tool_name: tool.tool_name.clone(),
                    arguments: tool.arguments.clone(),
                })
            }
            AgentEventData::ToolCompleted(tool) => {
                pending.retain(|call| call.tool_call_id != tool.tool_call_id);
            }
            _ => {}
        }
    }
    pending
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
    fn local_agent_tools_require_explicit_search_and_evidence() {
        assert!(!super::should_load_agent_tools(false, false));
        assert!(!super::should_load_agent_tools(true, false));
        assert!(!super::should_load_agent_tools(false, true));
        assert!(super::should_load_agent_tools(true, true));
    }

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
