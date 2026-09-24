use crate::agent::runtime::{AgentRecoveryService, RunCommandResult, RuntimeHost};
use crate::db::{current_workspace_id, with_conn_mut, DbState};
use chrono::Utc;
use std::time::{Duration, Instant};

#[tauri::command]
pub fn cancel_agent_run(
    db: tauri::State<DbState>,
    state: tauri::State<RuntimeHost>,
    run_id: String,
    assistant_message_id: Option<String>,
) -> Result<RunCommandResult, String> {
    let run_id =
        uuid::Uuid::parse_str(run_id.trim()).map_err(|_| "run_id must be a UUID".to_string())?;
    let assistant_message_id = assistant_message_id
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            uuid::Uuid::parse_str(value.trim())
                .map_err(|_| "assistant_message_id must be a UUID".to_string())
        })
        .transpose()?;
    let existing = with_conn_mut(&db, |connection| {
        crate::storage::repositories::runs::get(connection, current_workspace_id(), run_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "agent run not found".to_string())
    })?;
    let terminal = matches!(
        existing.state,
        crate::agent::protocol::AgentRunState::Completed
            | crate::agent::protocol::AgentRunState::Cancelled
            | crate::agent::protocol::AgentRunState::Failed
            | crate::agent::protocol::AgentRunState::Interrupted
    );

    // Signal both active loops and restored permission waiters. Active loops
    // own their terminal event; inactive restored runs are finished below.
    if !terminal {
        let active = state.snapshot(run_id).is_ok();
        state.cancel_turn(run_id)?;
        if active {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let current = with_conn_mut(&db, |connection| {
                    crate::storage::repositories::runs::get(
                        connection,
                        current_workspace_id(),
                        run_id,
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "agent run not found".to_string())
                })?;
                let events = with_conn_mut(&db, |connection| {
                    let service = AgentRecoveryService::new(connection, current_workspace_id())?;
                    service.replay(run_id, 0)
                })?;
                let terminal = matches!(
                    current.state,
                    crate::agent::protocol::AgentRunState::Completed
                        | crate::agent::protocol::AgentRunState::Cancelled
                        | crate::agent::protocol::AgentRunState::Failed
                        | crate::agent::protocol::AgentRunState::Interrupted
                );
                if terminal || Instant::now() >= deadline {
                    return Ok(RunCommandResult {
                        run: current,
                        events,
                        replay_only: false,
                    });
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    with_conn_mut(&db, |connection| {
        let mut service = AgentRecoveryService::new(connection, current_workspace_id())?;
        service.cancel(run_id, assistant_message_id, Utc::now())
    })
}
