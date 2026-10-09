use crate::agent::desktop::LocalAgentChatRequest;
use crate::agent::runtime::RuntimeHost;
use crate::db::DbState;
use crate::storage::secrets::SecretState;
use crate::tasks::cron_repository::{self, CronEvent};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};
use uuid::Uuid;

#[derive(Default)]
pub struct AgentAutomationState {
    loop_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    generation: Arc<AtomicU64>,
    in_flight: Arc<Mutex<HashSet<Uuid>>>,
}

pub(crate) fn content_root_for(database: &PathBuf) -> Result<PathBuf, String> {
    database
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "resolve RAG content root failed".to_string())
}

impl AgentAutomationState {
    pub fn start(
        &self,
        app: tauri::AppHandle,
        database: PathBuf,
        workspace_id: String,
    ) -> Result<(), String> {
        let mut task = self
            .loop_task
            .lock()
            .map_err(|_| "automation state poisoned")?;
        if task.is_some() {
            return Ok(());
        }
        let expected_generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = self.generation.clone();
        let in_flight = self.in_flight.clone();
        *task = Some(tauri::async_runtime::spawn(async move {
            while generation.load(Ordering::SeqCst) == expected_generation {
                if let Err(error) = pump(
                    &app,
                    &database,
                    &workspace_id,
                    &in_flight,
                    &generation,
                    expected_generation,
                ) {
                    eprintln!("Agent schedule pump failed: {error}");
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }));
        Ok(())
    }

    pub fn request_shutdown(&self) {
        // Serialize shutdown against the synchronous reservation phase. Runs
        // already started keep their checkpoints for startup recovery.
        let _dispatch = self
            .in_flight
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.stop_loop();
    }

    pub fn pause_for_restore(&self) -> Result<(), String> {
        let dispatch = self
            .in_flight
            .lock()
            .map_err(|_| "automation state poisoned")?;
        if !dispatch.is_empty() {
            return Err("定时 Agent 仍在运行，请先等待完成或取消运行，再恢复备份。".to_string());
        }
        self.stop_loop();
        Ok(())
    }

    fn stop_loop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut task) = self.loop_task.lock() {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
    }
}

pub fn start_for_database(app: &tauri::AppHandle, database: PathBuf) -> Result<(), String> {
    app.state::<AgentAutomationState>().start(
        app.clone(),
        database,
        crate::db::current_workspace_id().to_string(),
    )
}

pub fn prepare_restore(
    app: &tauri::AppHandle,
    agent_state: &crate::agent::runtime::RuntimeHost,
) -> Result<(), String> {
    if !agent_state.active_turns()?.is_empty() {
        return Err("Agent 仍在运行，请先等待完成或取消运行，再恢复备份。".to_string());
    }
    app.state::<AgentAutomationState>().pause_for_restore()
}

fn open(database: &Path) -> Result<Connection, String> {
    crate::storage::database::open(database)
        .map(|(connection, _)| connection)
        .map_err(|error| error.to_string())
}

fn pump(
    app: &tauri::AppHandle,
    database: &Path,
    workspace_id: &str,
    in_flight: &Arc<Mutex<HashSet<Uuid>>>,
    generation: &AtomicU64,
    expected_generation: u64,
) -> Result<(), String> {
    let mut dispatch = in_flight.lock().map_err(|_| "automation state poisoned")?;
    if generation.load(Ordering::SeqCst) != expected_generation {
        return Ok(());
    }
    let mut connection = open(database)?;
    cron_repository::tick(&mut connection, workspace_id, chrono::Utc::now(), 32)?;
    let events = cron_repository::pending(&connection, workspace_id)?;
    let launching = dispatch.clone();
    let mut busy_sessions = app
        .state::<RuntimeHost>()
        .active_turns()?
        .into_iter()
        .map(|turn| turn.session_id)
        .collect::<HashSet<_>>();
    busy_sessions.extend(
        events
            .iter()
            .filter(|event| launching.contains(&event.event_id))
            .filter_map(|event| event.conversation_id),
    );
    for mut event in events {
        if launching.contains(&event.event_id) {
            continue;
        }
        let run_id = event.run_id.unwrap_or(event.event_id);
        if let Some(state) = run_state(&connection, workspace_id, run_id)? {
            if matches!(
                state.as_str(),
                "completed" | "cancelled" | "failed" | "interrupted"
            ) {
                if state == "completed" {
                    cron_repository::acknowledge(&connection, workspace_id, event.event_id)?;
                } else {
                    cron_repository::record_error(
                        &connection,
                        workspace_id,
                        event.event_id,
                        &format!("Agent run {state}"),
                    )?;
                }
            }
            // Startup recovery owns recorded turns. Never create a second turn
            // for a recorded outbox run, even after cancellation.
            continue;
        }
        let Some(conversation_id) = event.conversation_id else {
            cron_repository::record_error(
                &connection,
                workspace_id,
                event.event_id,
                "schedule has no conversation",
            )?;
            continue;
        };
        if conversation_busy(&connection, workspace_id, conversation_id)?
            || busy_sessions.contains(&conversation_id)
        {
            continue;
        }
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE workspace_id = ?1 AND id = ?2 AND archived = 0)",
            params![workspace_id, conversation_id.to_string()], |row| row.get(0),
        ).map_err(|error| error.to_string())?;
        if !exists {
            cron_repository::record_error(
                &connection,
                workspace_id,
                event.event_id,
                "conversation was deleted or archived",
            )?;
            continue;
        }
        event.run_id = Some(cron_repository::reserve_run(
            &connection,
            workspace_id,
            event.event_id,
        )?);
        dispatch.insert(event.event_id);
        busy_sessions.insert(conversation_id);
        let app_for_run = app.clone();
        let database = database.to_path_buf();
        let in_flight = in_flight.clone();
        tauri::async_runtime::spawn(async move {
            let event_id = event.event_id;
            let _guard = DispatchGuard {
                in_flight,
                event_id,
            };
            if let Err(error) = dispatch_event(&app_for_run, &database, event.clone()).await {
                let error = crate::diagnostics::observability::redact_line(&error);
                if let Ok(connection) = open(&database) {
                    let _ = cron_repository::record_error(
                        &connection,
                        &event.workspace_id,
                        event_id,
                        &error,
                    );
                }
                let _ = app_for_run.emit("agent-schedule-error", serde_json::json!({
                    "jobId": event.job_id, "eventId": event_id, "conversationId": event.conversation_id,
                    "runId": event.run_id, "error": error,
                }));
            }
        });
    }
    Ok(())
}

pub(crate) async fn dispatch_event(
    app: &tauri::AppHandle,
    database: &Path,
    event: CronEvent,
) -> Result<(), String> {
    let conversation_id = event
        .conversation_id
        .ok_or("schedule conversation is missing")?;
    let run_id = event.run_id.ok_or("schedule run reservation is missing")?;
    let result = crate::app::desktop_chat_commands::desktop_agent_chat(
        app.clone(),
        app.state::<DbState>(),
        app.state::<SecretState>(),
        app.state::<RuntimeHost>(),
        LocalAgentChatRequest {
            session_id: Some(conversation_id.to_string()),
            message: event.prompt,
            run_id: Some(run_id.to_string()),
            agent_id: Some(event.agent_id),
            evidence_pack_id: None,
            smart_search_enabled: false,
            attachments: Vec::new(),
        },
    )
    .await;
    let mut connection = open(database)?;
    let final_result = result.and_then(|_| {
        let state = run_state(&connection, &event.workspace_id, run_id)?;
        completed_run(state.as_deref())
    });
    if let Err(error) = final_result {
        // Provider validation can fail after prepare_chat records a run.
        // Finish that run so it does not block this conversation forever.
        if run_state(&connection, &event.workspace_id, run_id)?.is_some() {
            if let Ok(mut recovery) = crate::agent::runtime::AgentRecoveryService::new(
                &mut connection,
                &event.workspace_id,
            ) {
                let _ = recovery.fail(run_id, error.clone(), chrono::Utc::now());
            }
        }
        return Err(error);
    }
    cron_repository::acknowledge(&connection, &event.workspace_id, event.event_id)?;
    let _ = app.emit("agent-schedule-completed", serde_json::json!({
        "jobId": event.job_id, "eventId": event.event_id, "conversationId": conversation_id, "runId": run_id,
    }));
    Ok(())
}

struct DispatchGuard {
    in_flight: Arc<Mutex<HashSet<Uuid>>>,
    event_id: Uuid,
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.in_flight.lock() {
            pending.remove(&self.event_id);
        }
    }
}

fn completed_run(state: Option<&str>) -> Result<(), String> {
    match state {
        Some("completed") => Ok(()),
        Some(state) => Err(format!("Agent schedule run ended in state: {state}")),
        None => Err("Agent schedule run was not persisted".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_requires_idle_dispatch_and_old_generation_stays_stopped() {
        let state = AgentAutomationState::default();
        state.in_flight.lock().unwrap().insert(Uuid::new_v4());
        assert!(state.pause_for_restore().is_err());
        assert_eq!(state.generation.load(Ordering::SeqCst), 0);
        state.in_flight.lock().unwrap().clear();
        state.pause_for_restore().unwrap();
        assert_eq!(state.generation.load(Ordering::SeqCst), 1);
        state.request_shutdown();
        assert_eq!(state.generation.load(Ordering::SeqCst), 2);
        assert!(completed_run(Some("completed")).is_ok());
        for state in [
            Some("cancelled"),
            Some("failed"),
            Some("interrupted"),
            Some("created"),
            None,
        ] {
            assert!(completed_run(state).is_err());
        }
    }
}

fn run_state(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT state FROM agent_runs WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, run_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn conversation_busy(
    connection: &Connection,
    workspace_id: &str,
    conversation_id: Uuid,
) -> Result<bool, String> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE workspace_id = ?1 AND conversation_id = ?2
         AND state NOT IN ('completed','cancelled','failed','interrupted'))",
        params![workspace_id, conversation_id.to_string()], |row| row.get(0),
    ).map_err(|error| error.to_string())
}
