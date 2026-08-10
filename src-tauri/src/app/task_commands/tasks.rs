// Background task command implementations.
use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::tasks::{repository, TaskRecord, TaskState};
use rusqlite::Connection;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct BackgroundTaskResponse {
    pub id: Uuid,
    pub kind: String,
    pub state: TaskState,
    pub progress: u8,
    pub attempt: u32,
    pub error_code: Option<String>,
    pub cancel_requested: bool,
    pub can_cancel: bool,
    pub can_retry: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) fn background_task_response(task: TaskRecord) -> BackgroundTaskResponse {
    let can_cancel = !task.cancel_requested
        && matches!(
            task.state,
            TaskState::Queued
                | TaskState::Running
                | TaskState::WaitingExternal
                | TaskState::Paused
                | TaskState::Interrupted
        );
    let can_retry = matches!(
        task.state,
        TaskState::Failed | TaskState::Cancelled | TaskState::Interrupted | TaskState::Paused
    );
    BackgroundTaskResponse {
        id: task.id,
        kind: task.kind,
        state: task.state,
        progress: task.progress,
        attempt: task.attempt,
        error_code: task.error_code,
        cancel_requested: task.cancel_requested,
        can_cancel,
        can_retry,
        created_at: task.created_at,
        updated_at: task.updated_at,
    }
}

fn cancel_task(
    connection: &mut Connection,
    workspace_id: &str,
    id: Uuid,
) -> Result<BackgroundTaskResponse, String> {
    let task = repository::get(connection, workspace_id, id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "task_not_found: task not found".to_string())?;
    let updated = match task.state {
        TaskState::Running => {
            repository::request_running_cancellation(connection, workspace_id, id, task.attempt)
        }
        TaskState::Queued
        | TaskState::WaitingExternal
        | TaskState::Paused
        | TaskState::Interrupted => repository::transition(
            connection,
            workspace_id,
            id,
            task.attempt,
            task.state,
            TaskState::Cancelled,
            None,
        ),
        _ => return Err("task_not_cancellable: task cannot be cancelled".to_string()),
    }
    .map_err(|error| error.to_string())?;
    Ok(background_task_response(updated))
}

fn retry_task(
    connection: &mut Connection,
    workspace_id: &str,
    id: Uuid,
) -> Result<BackgroundTaskResponse, String> {
    let task = repository::get(connection, workspace_id, id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "task_not_found: task not found".to_string())?;
    if !matches!(
        task.state,
        TaskState::Failed | TaskState::Cancelled | TaskState::Interrupted | TaskState::Paused
    ) {
        return Err("task_not_retryable: task cannot be retried".to_string());
    }
    repository::retry_manually(connection, workspace_id, id, task.attempt, task.state)
        .map(background_task_response)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_background_tasks(
    db: tauri::State<DbState>,
) -> Result<Vec<BackgroundTaskResponse>, String> {
    with_conn(&db, |connection| {
        repository::list(connection, current_workspace_id())
            .map(|tasks| tasks.into_iter().map(background_task_response).collect())
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn cancel_background_task(
    db: tauri::State<DbState>,
    id: String,
) -> Result<BackgroundTaskResponse, String> {
    let id = Uuid::parse_str(&id).map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn_mut(&db, |connection| {
        cancel_task(connection, current_workspace_id(), id)
    })
}

#[tauri::command]
pub fn retry_background_task(
    db: tauri::State<DbState>,
    id: String,
) -> Result<BackgroundTaskResponse, String> {
    let id = Uuid::parse_str(&id).map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn_mut(&db, |connection| {
        retry_task(connection, current_workspace_id(), id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::migrations::migrate;
    use crate::tasks::{repository, NewTask, TaskState};
    use rusqlite::Connection;

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open database");
        migrate(&mut connection).expect("migrate database");
        connection
    }

    fn create_task(connection: &mut Connection) -> crate::tasks::TaskRecord {
        repository::create(
            connection,
            NewTask {
                workspace_id: "workspace-a".to_string(),
                kind: "document_parse".to_string(),
                payload_json: r#"{"secret":"payload-value"}"#.to_string(),
                checkpoint_json: Some(r#"{"token":"checkpoint-value"}"#.to_string()),
                next_run_at: None,
                progress: 17,
            },
        )
        .expect("create task")
    }

    #[test]
    fn task_response_exposes_status_without_payload_or_checkpoint() {
        let mut connection = database();
        let response = background_task_response(create_task(&mut connection));

        assert!(response.can_cancel);
        assert!(!response.can_retry);
        let json = serde_json::to_string(&response).expect("serialize task response");
        for forbidden in [
            "payload_json",
            "checkpoint_json",
            "payload-value",
            "checkpoint-value",
        ] {
            assert!(!json.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn cancelling_a_queued_task_is_immediate_and_durable() {
        let mut connection = database();
        let task = create_task(&mut connection);

        let response =
            cancel_task(&mut connection, "workspace-a", task.id).expect("cancel queued task");

        assert_eq!(response.state, TaskState::Cancelled);
        assert!(!response.can_cancel);
        assert!(response.can_retry);
        assert_eq!(
            repository::get(&connection, "workspace-a", task.id)
                .expect("read task")
                .expect("task exists")
                .state,
            TaskState::Cancelled
        );
    }

    #[test]
    fn retrying_a_failed_task_clears_the_failure_and_cancel_flag() {
        let mut connection = database();
        let task = create_task(&mut connection);
        let running =
            repository::claim_next(&mut connection, "workspace-a", "2099-01-01T00:00:00Z")
                .expect("claim task")
                .expect("claimed task");
        repository::checkpoint(
            &mut connection,
            "workspace-a",
            task.id,
            running.attempt,
            None,
            running.progress,
            Some("2099-12-31T00:00:00Z"),
        )
        .expect("schedule future run");
        let failed = repository::transition(
            &mut connection,
            "workspace-a",
            task.id,
            running.attempt,
            TaskState::Running,
            TaskState::Failed,
            Some("provider_timeout"),
        )
        .expect("fail task");

        let response =
            retry_task(&mut connection, "workspace-a", failed.id).expect("retry failed task");

        assert_eq!(response.state, TaskState::Queued);
        assert_eq!(response.error_code, None);
        assert!(!response.cancel_requested);
        assert!(!response.can_retry);
        assert_eq!(
            repository::get(&connection, "workspace-a", task.id)
                .expect("read task")
                .expect("task exists")
                .next_run_at,
            None
        );
    }

    #[test]
    fn cancelling_a_running_task_requests_cooperative_cancellation() {
        let mut connection = database();
        let task = create_task(&mut connection);
        repository::claim_next(&mut connection, "workspace-a", "2099-01-01T00:00:00Z")
            .expect("claim task")
            .expect("claimed task");

        let response =
            cancel_task(&mut connection, "workspace-a", task.id).expect("request cancellation");

        assert_eq!(response.state, TaskState::Running);
        assert!(response.cancel_requested);
        assert!(!response.can_cancel);
    }

    #[test]
    fn running_cancellation_is_fenced_by_attempt() {
        let mut connection = database();
        let task = create_task(&mut connection);
        let running =
            repository::claim_next(&mut connection, "workspace-a", "2099-01-01T00:00:00Z")
                .expect("claim task")
                .expect("claimed task");

        let error = repository::request_running_cancellation(
            &mut connection,
            "workspace-a",
            task.id,
            running.attempt + 1,
        )
        .expect_err("stale cancellation must fail");

        assert_eq!(error.code(), "stale_claim");
        assert!(
            !repository::get(&connection, "workspace-a", task.id)
                .expect("read task")
                .expect("task exists")
                .cancel_requested
        );
    }
}
