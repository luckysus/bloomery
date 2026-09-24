use crate::agent::runtime::RuntimeHost;
use crate::db::{current_workspace_id, with_conn_mut, DbState};
use crate::storage::repositories::child_turns::{self, ChildTurnCommandResult, ChildTurnRecord};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayAgentChildTurnRequest {
    pub child_turn_id: String,
    pub after_sequence: Option<u64>,
}

#[tauri::command]
pub fn list_agent_child_turns(
    db: tauri::State<DbState>,
    parent_turn_id: Option<String>,
) -> Result<Vec<ChildTurnRecord>, String> {
    let parent_turn_id = parent_turn_id
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            Uuid::parse_str(value.trim()).map_err(|_| "parent_turn_id must be a UUID".to_string())
        })
        .transpose()?;
    with_conn_mut(&db, |connection| {
        child_turns::list(connection, current_workspace_id(), parent_turn_id)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn replay_agent_child_turn(
    db: tauri::State<DbState>,
    request: ReplayAgentChildTurnRequest,
) -> Result<Vec<crate::agent::protocol::AgentEventEnvelope>, String> {
    let child_turn_id = Uuid::parse_str(request.child_turn_id.trim())
        .map_err(|_| "child_turn_id must be a UUID".to_string())?;
    with_conn_mut(&db, |connection| {
        child_turns::replay(
            connection,
            current_workspace_id(),
            child_turn_id,
            request.after_sequence.unwrap_or(0),
        )
        .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn cancel_agent_child_turn(
    db: tauri::State<DbState>,
    state: tauri::State<RuntimeHost>,
    child_turn_id: String,
) -> Result<ChildTurnCommandResult, String> {
    let child_turn_id = Uuid::parse_str(child_turn_id.trim())
        .map_err(|_| "child_turn_id must be a UUID".to_string())?;
    let child = with_conn_mut(&db, |connection| {
        child_turns::get(connection, current_workspace_id(), child_turn_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "child turn not found".to_string())
    })?;
    let terminal = matches!(
        child.state,
        crate::agent::protocol::AgentRunState::Completed
            | crate::agent::protocol::AgentRunState::Cancelled
            | crate::agent::protocol::AgentRunState::Failed
            | crate::agent::protocol::AgentRunState::Interrupted
    );
    if !terminal && state.snapshot(child_turn_id).is_ok() {
        state.cancel_turn(child_turn_id)?;
        let events = with_conn_mut(&db, |connection| {
            child_turns::replay(connection, current_workspace_id(), child_turn_id, 0)
                .map_err(|error| error.to_string())
        })?;
        return Ok(ChildTurnCommandResult {
            child,
            events,
            replay_only: false,
        });
    }
    with_conn_mut(&db, |connection| {
        child_turns::cancel(
            connection,
            current_workspace_id(),
            child_turn_id,
            Utc::now(),
        )
        .map_err(|error| error.to_string())
    })
}
