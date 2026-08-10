use crate::agent::desktop::LocalAgentState;
use crate::agent::protocol::PermissionDecision;
use crate::agent::runtime::{AgentRecoveryService, RecoveredRun, RunCommandResult};
use crate::db::{current_workspace_id, with_conn_mut, DbState};
use crate::permissions::{ParameterScope, PermissionAction, PermissionRule, RuleEffect};
use crate::tools::{ToolId, ToolSource, ToolVersion};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayAgentRunRequest {
    pub run_id: String,
    pub after_sequence: Option<u64>,
}

#[tauri::command]
pub fn replay_agent_run(
    db: tauri::State<DbState>,
    request: ReplayAgentRunRequest,
) -> Result<Vec<crate::agent::protocol::AgentEventEnvelope>, String> {
    let run_id = parse_uuid(&request.run_id, "run_id")?;
    with_conn_mut(&db, |connection| {
        let service = AgentRecoveryService::new(connection, current_workspace_id())?;
        service.replay(run_id, request.after_sequence.unwrap_or(0))
    })
}

#[tauri::command]
pub fn cancel_agent_run(
    db: tauri::State<DbState>,
    run_id: String,
    assistant_message_id: Option<String>,
) -> Result<RunCommandResult, String> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    let assistant_message_id = assistant_message_id
        .filter(|value| !value.trim().is_empty())
        .map(|value| parse_uuid(&value, "assistant_message_id"))
        .transpose()?;
    with_conn_mut(&db, |connection| {
        let mut service = AgentRecoveryService::new(connection, current_workspace_id())?;
        service.cancel(run_id, assistant_message_id, Utc::now())
    })
}

#[tauri::command]
pub fn retry_agent_run(
    db: tauri::State<DbState>,
    source_run_id: String,
    run_id: String,
    event_id: Option<String>,
) -> Result<crate::storage::repositories::runs::RunWithEvent, String> {
    let source_run_id = parse_uuid(&source_run_id, "source_run_id")?;
    let run_id = parse_uuid(&run_id, "run_id")?;
    let event_id = event_id
        .filter(|value| !value.trim().is_empty())
        .map(|value| parse_uuid(&value, "event_id"))
        .transpose()?
        .unwrap_or_else(Uuid::new_v4);
    with_conn_mut(&db, |connection| {
        let mut service = AgentRecoveryService::new(connection, current_workspace_id())?;
        service.retry(source_run_id, run_id, event_id, Utc::now())
    })
}

#[tauri::command]
pub fn recover_agent_runs(db: tauri::State<DbState>) -> Result<Vec<RecoveredRun>, String> {
    with_conn_mut(&db, |connection| {
        let mut service = AgentRecoveryService::new(connection, current_workspace_id())?;
        service.recover_active(&HashSet::new(), Utc::now())
    })
}

#[tauri::command]
pub fn resolve_agent_permission(
    db: tauri::State<DbState>,
    state: tauri::State<LocalAgentState>,
    permission_id: String,
    decision: PermissionDecision,
) -> Result<(), String> {
    let permission_id = parse_uuid(&permission_id, "permission_id")?;
    let request = state.pending_permission(permission_id)?;
    if decision == PermissionDecision::AllowAlways {
        let rule = PermissionRule {
            id: Uuid::new_v4(),
            tool_id: ToolId::new(request.tool_id.clone()).map_err(|error| error.to_string())?,
            tool_version: ToolVersion::parse("1.0.0").map_err(|error| error.to_string())?,
            source: ToolSource::Builtin,
            action: PermissionAction::Execute,
            scope: ParameterScope::Exact(request.arguments.clone()),
            effect: RuleEffect::Allow,
        };
        with_conn_mut(&db, |connection| {
            crate::storage::repositories::permissions::insert(
                connection,
                current_workspace_id(),
                &rule,
            )
        })?;
    }
    state
        .resolve_permission(permission_id, decision)
        .map(|_| ())
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value.trim()).map_err(|_| format!("{field} must be a UUID"))
}
