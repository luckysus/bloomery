use crate::agent::protocol::PermissionDecision;
use crate::agent::runtime::{AgentInputKind, AgentRecoveryService, RecoveredRun, RuntimeHost};
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
pub fn steer_agent_run(
    state: tauri::State<RuntimeHost>,
    run_id: String,
    message: String,
) -> Result<(), String> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    state.enqueue_input(run_id, AgentInputKind::Steering, message)
}

#[tauri::command]
pub fn follow_up_agent_run(
    state: tauri::State<RuntimeHost>,
    run_id: String,
    message: String,
) -> Result<(), String> {
    let run_id = parse_uuid(&run_id, "run_id")?;
    state.enqueue_input(run_id, AgentInputKind::FollowUp, message)
}

#[tauri::command]
pub fn recover_agent_runs(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    state: tauri::State<RuntimeHost>,
) -> Result<Vec<RecoveredRun>, String> {
    let recovered = with_conn_mut(&db, |connection| {
        let mut service = AgentRecoveryService::new(connection, current_workspace_id())?;
        service.recover_active(&HashSet::new(), Utc::now())
    })?;
    for candidate in recovered.iter().filter(|candidate| {
        matches!(
            &candidate.action,
            crate::agent::runtime::RecoveryAction::ResumeFromCheckpoint(_)
                | crate::agent::runtime::RecoveryAction::ResumeTools(_)
                | crate::agent::runtime::RecoveryAction::AwaitPermissions(_)
        ) && state.snapshot(candidate.run.id).is_err()
    }) {
        let app_for_run = app.clone();
        let runtime = state.inner().clone();
        let candidate = candidate.clone();
        let waits = match crate::app::desktop_agent_runtime::restore_recovered_permissions(
            &runtime, &candidate,
        ) {
            Ok(waits) => waits,
            Err(error) => {
                eprintln!("restore agent permissions from the desktop control failed: {error}");
                None
            }
        };
        if matches!(
            &candidate.action,
            crate::agent::runtime::RecoveryAction::AwaitPermissions(_)
        ) && waits.is_none()
        {
            continue;
        }
        tauri::async_runtime::spawn(async move {
            let result = if let Some(waits) = waits {
                crate::app::desktop_agent_runtime::resume_recovered_agent_with_permissions(
                    &app_for_run,
                    &runtime,
                    current_workspace_id(),
                    candidate,
                    waits,
                )
                .await
            } else {
                crate::app::desktop_agent_runtime::resume_recovered_agent(
                    &app_for_run,
                    &runtime,
                    current_workspace_id(),
                    candidate,
                )
                .await
            };
            if let Err(error) = result {
                eprintln!("resume agent run from the desktop control failed: {error}");
            }
        });
    }
    Ok(recovered)
}

#[tauri::command]
pub fn resolve_agent_permission(
    db: tauri::State<DbState>,
    state: tauri::State<RuntimeHost>,
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
