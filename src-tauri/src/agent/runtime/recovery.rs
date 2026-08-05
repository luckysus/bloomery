use crate::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentRunState, PermissionRisk, RunOutcome, RunStateChanged,
};
use crate::storage::repositories::{events, runs};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingPermission {
    pub permission_id: Uuid,
    pub tool_call_id: Uuid,
    pub risk: PermissionRisk,
    pub reason: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCheckpoint {
    pub tool_call_id: Uuid,
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum RecoveryAction {
    Regenerate,
    AwaitPermissions(Vec<PendingPermission>),
    ResumeTools(Vec<ToolCheckpoint>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveredRun {
    pub run: runs::AgentRunRecord,
    pub action: RecoveryAction,
    pub events: Vec<AgentEventEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunCommandResult {
    pub run: runs::AgentRunRecord,
    pub events: Vec<AgentEventEnvelope>,
    pub replay_only: bool,
}

pub struct AgentRecoveryService<'a> {
    connection: &'a mut Connection,
    workspace_id: &'a str,
}

impl<'a> AgentRecoveryService<'a> {
    pub fn new(connection: &'a mut Connection, workspace_id: &'a str) -> Result<Self, String> {
        if workspace_id.trim().is_empty() || workspace_id.trim() != workspace_id {
            return Err("workspace_id is invalid".to_string());
        }
        Ok(Self {
            connection,
            workspace_id,
        })
    }

    pub fn recover_active(
        &mut self,
        idempotent_tool_ids: &HashSet<String>,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<RecoveredRun>, String> {
        let runs = runs::list_nonterminal(self.connection, self.workspace_id, None)
            .map_err(|error| error.to_string())?;
        self.recover(runs, idempotent_tool_ids, timestamp)
    }

    pub fn recover_stale(
        &mut self,
        idempotent_tool_ids: &HashSet<String>,
        cutoff: DateTime<Utc>,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<RecoveredRun>, String> {
        let runs = runs::list_nonterminal(self.connection, self.workspace_id, Some(cutoff))
            .map_err(|error| error.to_string())?;
        self.recover(runs, idempotent_tool_ids, timestamp)
    }

    pub fn replay(
        &self,
        run_id: Uuid,
        after_sequence: u64,
    ) -> Result<Vec<AgentEventEnvelope>, String> {
        events::replay(self.connection, self.workspace_id, run_id, after_sequence)
            .map_err(|error| error.to_string())
    }

    pub fn cancel(
        &mut self,
        run_id: Uuid,
        assistant_message_id: Option<Uuid>,
        timestamp: DateTime<Utc>,
    ) -> Result<RunCommandResult, String> {
        let run = self.require_run(run_id)?;
        if is_terminal(run.state) {
            return Ok(RunCommandResult {
                run,
                events: self.replay(run_id, 0)?,
                replay_only: true,
            });
        }
        let events = runs::finish(
            self.connection,
            self.workspace_id,
            run_id,
            RunStateChanged {
                previous: run.state,
                current: AgentRunState::Cancelled,
                reason: Some("user_cancelled".to_string()),
            },
            RunOutcome::Cancelled,
            assistant_message_id,
            timestamp,
        )
        .map_err(|error| error.to_string())?;
        Ok(RunCommandResult {
            run: self.require_run(run_id)?,
            events,
            replay_only: false,
        })
    }

    pub fn retry(
        &mut self,
        source_run_id: Uuid,
        run_id: Uuid,
        event_id: Uuid,
        timestamp: DateTime<Utc>,
    ) -> Result<runs::RunWithEvent, String> {
        let source = self.require_run(source_run_id)?;
        if !is_terminal(source.state) {
            return Err("agent run must be terminal before retry".to_string());
        }
        runs::create(
            self.connection,
            runs::NewAgentRun {
                id: run_id,
                workspace_id: self.workspace_id.to_string(),
                conversation_id: source.conversation_id,
                user_message_id: source.user_message_id,
                event_id,
                timestamp,
            },
        )
        .map_err(|error| error.to_string())
    }

    fn recover(
        &mut self,
        active_runs: Vec<runs::AgentRunRecord>,
        idempotent_tool_ids: &HashSet<String>,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<RecoveredRun>, String> {
        let mut recovered = Vec::with_capacity(active_runs.len());
        for run in active_runs {
            let replay = self.replay(run.id, 0)?;
            let action = recovery_action(&run, &replay, idempotent_tool_ids);
            if !matches!(action, RecoveryAction::Regenerate) {
                recovered.push(RecoveredRun {
                    run,
                    action,
                    events: Vec::new(),
                });
                continue;
            }
            let events = runs::finish(
                self.connection,
                self.workspace_id,
                run.id,
                RunStateChanged {
                    previous: run.state,
                    current: AgentRunState::Interrupted,
                    reason: Some("app_restart".to_string()),
                },
                RunOutcome::Interrupted,
                None,
                timestamp,
            )
            .map_err(|error| error.to_string())?;
            recovered.push(RecoveredRun {
                run: self.require_run(run.id)?,
                action,
                events,
            });
        }
        Ok(recovered)
    }

    fn require_run(&self, run_id: Uuid) -> Result<runs::AgentRunRecord, String> {
        runs::get(self.connection, self.workspace_id, run_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "agent run not found".to_string())
    }
}

fn recovery_action(
    run: &runs::AgentRunRecord,
    replay: &[AgentEventEnvelope],
    idempotent_tool_ids: &HashSet<String>,
) -> RecoveryAction {
    match run.state {
        AgentRunState::AwaitingPermission => {
            let permissions = pending_permissions(replay);
            if permissions.is_empty() {
                RecoveryAction::Regenerate
            } else {
                RecoveryAction::AwaitPermissions(permissions)
            }
        }
        AgentRunState::ExecutingTools => {
            let tools = pending_tools(replay);
            if !tools.is_empty()
                && tools
                    .iter()
                    .all(|checkpoint| idempotent_tool_ids.contains(&checkpoint.tool_id))
            {
                RecoveryAction::ResumeTools(tools)
            } else {
                RecoveryAction::Regenerate
            }
        }
        _ => RecoveryAction::Regenerate,
    }
}

fn pending_permissions(events: &[AgentEventEnvelope]) -> Vec<PendingPermission> {
    let mut pending = Vec::new();
    for event in events {
        match &event.data {
            AgentEventData::PermissionRequested(permission) => pending.push(PendingPermission {
                permission_id: permission.permission_id,
                tool_call_id: permission.tool_call_id,
                risk: permission.risk,
                reason: permission.reason.clone(),
                summary: permission.summary.clone(),
            }),
            AgentEventData::PermissionResolved(resolved) => {
                pending.retain(|permission| permission.permission_id != resolved.permission_id);
            }
            _ => {}
        }
    }
    pending
}

fn pending_tools(events: &[AgentEventEnvelope]) -> Vec<ToolCheckpoint> {
    let mut pending = Vec::new();
    for event in events {
        match &event.data {
            AgentEventData::ToolRequested(tool) => pending.push(ToolCheckpoint {
                tool_call_id: tool.tool_call_id,
                tool_id: tool.tool_id.clone(),
                tool_name: tool.tool_name.clone(),
                arguments: tool.arguments.clone(),
            }),
            AgentEventData::ToolCompleted(tool) => {
                pending.retain(|checkpoint| checkpoint.tool_call_id != tool.tool_call_id);
            }
            _ => {}
        }
    }
    pending
}

fn is_terminal(state: AgentRunState) -> bool {
    matches!(
        state,
        AgentRunState::Completed
            | AgentRunState::Cancelled
            | AgentRunState::Failed
            | AgentRunState::Interrupted
    )
}
