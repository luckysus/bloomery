use super::model::{
    validate_identifier, validate_json, validate_progress, validate_timestamp, NewTask, TaskError,
    TaskRecord, TaskState,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::str::FromStr;
use uuid::Uuid;

pub fn create(conn: &Connection, task: NewTask) -> Result<TaskRecord, TaskError> {
    task.validate()?;
    let id = Uuid::new_v4();
    let timestamp = now_rfc3339();
    conn.execute(
        "INSERT INTO background_tasks
             (id, workspace_id, kind, state, payload_json, checkpoint_json, attempt,
              next_run_at, progress, error_code, cancel_requested, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'queued', ?4, ?5, 0, ?6, ?7, NULL, 0, ?8, ?8)",
        params![
            id.to_string(),
            task.workspace_id,
            task.kind,
            task.payload_json,
            task.checkpoint_json,
            task.next_run_at,
            task.progress,
            timestamp,
        ],
    )
    .map_err(storage_error)?;
    get(conn, &task.workspace_id, id)?
        .ok_or_else(|| TaskError::new("storage_error", "created task could not be read back"))
}

pub fn get(
    conn: &Connection,
    workspace_id: &str,
    id: Uuid,
) -> Result<Option<TaskRecord>, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    conn.query_row(
        SELECT_TASK,
        params![workspace_id, id.to_string()],
        row_to_task,
    )
    .optional()
    .map_err(storage_error)?
    .map(decode_task)
    .transpose()
}

pub fn list(conn: &Connection, workspace_id: &str) -> Result<Vec<TaskRecord>, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    let mut statement = conn
        .prepare(
            "SELECT id, workspace_id, kind, state, payload_json, checkpoint_json, attempt,
                        next_run_at, progress, error_code, cancel_requested, created_at, updated_at
                 FROM background_tasks
                 WHERE workspace_id = ?1
                 ORDER BY created_at ASC, id ASC",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map(params![workspace_id], row_to_task)
        .map_err(storage_error)?;
    rows.map(|row| row.map_err(storage_error).and_then(decode_task))
        .collect()
}

pub fn checkpoint(
    conn: &mut Connection,
    workspace_id: &str,
    id: Uuid,
    expected_attempt: u32,
    checkpoint_json: Option<&str>,
    progress: u8,
    next_run_at: Option<&str>,
) -> Result<TaskRecord, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    if let Some(checkpoint_json) = checkpoint_json {
        validate_json("checkpoint_json", checkpoint_json)?;
    }
    if let Some(next_run_at) = next_run_at {
        validate_timestamp("next_run_at", next_run_at)?;
    }
    validate_progress(progress)?;
    let updated = conn
        .execute(
            "UPDATE background_tasks
                 SET checkpoint_json = ?1, progress = ?2, next_run_at = ?3, updated_at = ?4
                 WHERE workspace_id = ?5 AND id = ?6 AND attempt = ?7
                   AND state IN ('running', 'waiting_external')",
            params![
                checkpoint_json,
                progress,
                next_run_at,
                now_rfc3339(),
                workspace_id,
                id.to_string(),
                expected_attempt
            ],
        )
        .map_err(storage_error)?;
    if updated == 0 {
        return match get(conn, workspace_id, id)? {
            Some(record) if record.attempt != expected_attempt => Err(TaskError::new(
                "stale_claim",
                "task attempt no longer belongs to this worker",
            )),
            Some(record) => {
                let code = if matches!(
                    record.state,
                    TaskState::Running | TaskState::WaitingExternal
                ) {
                    "storage_error"
                } else {
                    "stale_claim"
                };
                Err(TaskError::new(
                    code,
                    "task is not owned by an active worker",
                ))
            }
            None => Err(TaskError::new("task_not_found", "task not found")),
        };
    }
    get(conn, workspace_id, id)?
        .ok_or_else(|| TaskError::new("storage_error", "checkpointed task could not be read back"))
}

pub fn schedule_retry(
    conn: &mut Connection,
    workspace_id: &str,
    id: Uuid,
    expected_attempt: u32,
    checkpoint_json: Option<&str>,
    progress: u8,
    next_run_at: &str,
    now: &str,
) -> Result<TaskRecord, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    if let Some(checkpoint_json) = checkpoint_json {
        validate_json("checkpoint_json", checkpoint_json)?;
    }
    validate_progress(progress)?;
    validate_timestamp("next_run_at", next_run_at)?;
    validate_timestamp("now", now)?;
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let updated = transaction
        .execute(
            "UPDATE background_tasks
                 SET state = CASE WHEN cancel_requested = 1 THEN 'cancelled' ELSE 'queued' END,
                     checkpoint_json = ?1, progress = ?2,
                     next_run_at = ?3, error_code = NULL,
                     updated_at = ?4
                 WHERE workspace_id = ?5 AND id = ?6 AND attempt = ?7
                   AND state = 'running'",
            params![
                checkpoint_json,
                progress,
                next_run_at,
                now,
                workspace_id,
                id.to_string(),
                expected_attempt,
            ],
        )
        .map_err(storage_error)?;
    if updated == 0 {
        let current = transaction
            .query_row(
                SELECT_TASK,
                params![workspace_id, id.to_string()],
                row_to_task,
            )
            .optional()
            .map_err(storage_error)?
            .map(decode_task)
            .transpose()?;
        return match current {
            None => Err(TaskError::new("task_not_found", "task not found")),
            Some(task) if task.attempt != expected_attempt || task.state != TaskState::Running => {
                Err(TaskError::new(
                    "stale_claim",
                    "task attempt or state no longer belongs to this worker",
                ))
            }
            Some(_) => Err(TaskError::new(
                "storage_error",
                "retry update affected an unexpected number of rows",
            )),
        };
    }
    let record = transaction
        .query_row(
            SELECT_TASK,
            params![workspace_id, id.to_string()],
            row_to_task,
        )
        .map_err(storage_error)
        .and_then(decode_task)?;
    transaction.commit().map_err(storage_error)?;
    Ok(record)
}

pub fn transition(
    conn: &mut Connection,
    workspace_id: &str,
    id: Uuid,
    expected_attempt: u32,
    expected_state: TaskState,
    target: TaskState,
    error_code: Option<&str>,
) -> Result<TaskRecord, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    match (target, error_code) {
        (TaskState::Failed, Some(error_code)) => {
            validate_identifier("error_code", error_code)?;
        }
        (TaskState::Failed, None) => {
            return Err(TaskError::new(
                "invalid_task",
                "failed tasks require an error_code",
            ));
        }
        (_, Some(_)) => {
            return Err(TaskError::new(
                "invalid_task",
                "error_code is only valid for failed tasks",
            ));
        }
        (_, None) => {}
    }
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let current = transaction
        .query_row(
            SELECT_TASK,
            params![workspace_id, id.to_string()],
            row_to_task,
        )
        .optional()
        .map_err(storage_error)?
        .map(decode_task)
        .transpose()?
        .ok_or_else(|| TaskError::new("task_not_found", "task not found"))?;
    if current.attempt != expected_attempt {
        return Err(TaskError::new(
            "stale_claim",
            "task attempt no longer belongs to this worker",
        ));
    }
    if current.state != expected_state {
        return Err(TaskError::new(
            "stale_claim",
            "task state no longer belongs to this worker",
        ));
    }
    if matches!(
        expected_state,
        TaskState::Queued | TaskState::WaitingExternal
    ) && target == TaskState::Running
    {
        return Err(TaskError::new(
            "claim_required",
            "queued and waiting tasks must be started through the atomic claim operation",
        ));
    }
    if !current.state.can_transition_to(target) {
        return Err(TaskError::new(
            "illegal_transition",
            format!(
                "{} cannot transition to {}",
                current.state.as_str(),
                target.as_str()
            ),
        ));
    }
    let (progress, next_run_at, error_code) = if target == TaskState::Completed {
        (100, None, None)
    } else if target == TaskState::Failed {
        (current.progress, current.next_run_at.as_deref(), error_code)
    } else if target == TaskState::Queued {
        (current.progress, current.next_run_at.as_deref(), None)
    } else {
        (current.progress, current.next_run_at.as_deref(), None)
    };
    let cancel_requested = if target == TaskState::Queued {
        0
    } else {
        i64::from(current.cancel_requested)
    };
    transaction
        .execute(
            "UPDATE background_tasks
                 SET state = ?1, progress = ?2, next_run_at = ?3, error_code = ?4,
                     cancel_requested = ?5, updated_at = ?6
                 WHERE workspace_id = ?7 AND id = ?8 AND attempt = ?9 AND state = ?10",
            params![
                target.as_str(),
                progress,
                next_run_at,
                error_code,
                cancel_requested,
                now_rfc3339(),
                workspace_id,
                id.to_string(),
                expected_attempt,
                expected_state.as_str()
            ],
        )
        .map_err(storage_error)?;
    let record = transaction
        .query_row(
            SELECT_TASK,
            params![workspace_id, id.to_string()],
            row_to_task,
        )
        .map_err(storage_error)
        .and_then(decode_task)?;
    transaction.commit().map_err(storage_error)?;
    Ok(record)
}

pub fn request_cancellation(
    conn: &mut Connection,
    workspace_id: &str,
    id: Uuid,
) -> Result<TaskRecord, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    let changed = conn
        .execute(
            "UPDATE background_tasks SET cancel_requested = 1, updated_at = ?1
                 WHERE workspace_id = ?2 AND id = ?3",
            params![now_rfc3339(), workspace_id, id.to_string()],
        )
        .map_err(storage_error)?;
    if changed == 0 {
        return Err(TaskError::new("task_not_found", "task not found"));
    }
    get(conn, workspace_id, id)?
        .ok_or_else(|| TaskError::new("storage_error", "cancelled task could not be read back"))
}

pub fn request_running_cancellation(
    conn: &mut Connection,
    workspace_id: &str,
    id: Uuid,
    expected_attempt: u32,
) -> Result<TaskRecord, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let changed = transaction
        .execute(
            "UPDATE background_tasks SET cancel_requested = 1, updated_at = ?1
             WHERE workspace_id = ?2 AND id = ?3 AND attempt = ?4 AND state = 'running'",
            params![
                now_rfc3339(),
                workspace_id,
                id.to_string(),
                expected_attempt
            ],
        )
        .map_err(storage_error)?;
    if changed == 0 {
        let current = transaction
            .query_row(
                SELECT_TASK,
                params![workspace_id, id.to_string()],
                row_to_task,
            )
            .optional()
            .map_err(storage_error)?
            .map(decode_task)
            .transpose()?;
        return match current {
            None => Err(TaskError::new("task_not_found", "task not found")),
            Some(_) => Err(TaskError::new(
                "stale_claim",
                "task attempt or state no longer belongs to this worker",
            )),
        };
    }
    let record = transaction
        .query_row(
            SELECT_TASK,
            params![workspace_id, id.to_string()],
            row_to_task,
        )
        .map_err(storage_error)
        .and_then(decode_task)?;
    transaction.commit().map_err(storage_error)?;
    Ok(record)
}

pub fn retry_manually(
    conn: &mut Connection,
    workspace_id: &str,
    id: Uuid,
    expected_attempt: u32,
    expected_state: TaskState,
) -> Result<TaskRecord, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    if !matches!(
        expected_state,
        TaskState::Failed | TaskState::Cancelled | TaskState::Interrupted | TaskState::Paused
    ) {
        return Err(TaskError::new(
            "illegal_transition",
            "task cannot be retried",
        ));
    }
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let changed = transaction
        .execute(
            "UPDATE background_tasks
             SET state = 'queued', next_run_at = NULL, error_code = NULL,
                 cancel_requested = 0, updated_at = ?1
             WHERE workspace_id = ?2 AND id = ?3 AND attempt = ?4 AND state = ?5",
            params![
                now_rfc3339(),
                workspace_id,
                id.to_string(),
                expected_attempt,
                expected_state.as_str()
            ],
        )
        .map_err(storage_error)?;
    if changed == 0 {
        let exists = transaction
            .query_row(
                "SELECT 1 FROM background_tasks WHERE workspace_id = ?1 AND id = ?2",
                params![workspace_id, id.to_string()],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage_error)?
            .is_some();
        return Err(if exists {
            TaskError::new("stale_claim", "task attempt or state changed before retry")
        } else {
            TaskError::new("task_not_found", "task not found")
        });
    }
    let record = transaction
        .query_row(
            SELECT_TASK,
            params![workspace_id, id.to_string()],
            row_to_task,
        )
        .map_err(storage_error)
        .and_then(decode_task)?;
    transaction.commit().map_err(storage_error)?;
    Ok(record)
}

pub fn claim_next(
    conn: &mut Connection,
    workspace_id: &str,
    now: &str,
) -> Result<Option<TaskRecord>, TaskError> {
    validate_identifier("workspace_id", workspace_id)?;
    validate_timestamp("now", now)?;
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    {
        let mut statement = transaction
            .prepare(
                "SELECT next_run_at
                     FROM background_tasks
                     WHERE workspace_id = ?1 AND state IN ('queued', 'waiting_external')
                       AND cancel_requested = 0 AND next_run_at IS NOT NULL",
            )
            .map_err(storage_error)?;
        let schedules = statement
            .query_map(params![workspace_id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        for schedule in schedules {
            validate_timestamp("next_run_at", &schedule.map_err(storage_error)?)
                .map_err(storage_message)?;
        }
    }
    let candidate_id = transaction
        .query_row(
            "SELECT id
                 FROM background_tasks
                 WHERE workspace_id = ?1
                   AND state IN ('queued', 'waiting_external')
                   AND cancel_requested = 0
                   AND (next_run_at IS NULL OR julianday(next_run_at) <= julianday(?2))
                 ORDER BY next_run_at IS NOT NULL ASC,
                          julianday(next_run_at) ASC,
                          created_at ASC,
                          id ASC
                 LIMIT 1",
            params![workspace_id, now],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage_error)?;
    let Some(candidate_id) = candidate_id else {
        transaction.commit().map_err(storage_error)?;
        return Ok(None);
    };
    let updated = transaction
        .execute(
            "UPDATE background_tasks
                 SET state = 'running', attempt = attempt + 1, updated_at = ?1
                 WHERE workspace_id = ?2 AND id = ?3
                   AND state IN ('queued', 'waiting_external') AND cancel_requested = 0",
            params![now, workspace_id, candidate_id],
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(TaskError::new(
            "storage_error",
            "task claim update affected an unexpected number of rows",
        ));
    }
    let claimed = transaction
        .query_row(
            SELECT_TASK,
            params![workspace_id, candidate_id],
            row_to_task,
        )
        .map_err(storage_error)
        .and_then(decode_task)?;
    transaction.commit().map_err(storage_error)?;
    Ok(Some(claimed))
}

const SELECT_TASK: &str =
    "SELECT id, workspace_id, kind, state, payload_json, checkpoint_json, attempt,
            next_run_at, progress, error_code, cancel_requested, created_at, updated_at
     FROM background_tasks WHERE workspace_id = ?1 AND id = ?2";

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawTask> {
    Ok(RawTask {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        kind: row.get(2)?,
        state: row.get(3)?,
        payload_json: row.get(4)?,
        checkpoint_json: row.get(5)?,
        attempt: row.get(6)?,
        next_run_at: row.get(7)?,
        progress: row.get(8)?,
        error_code: row.get(9)?,
        cancel_requested: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

struct RawTask {
    id: String,
    workspace_id: String,
    kind: String,
    state: String,
    payload_json: String,
    checkpoint_json: Option<String>,
    attempt: i64,
    next_run_at: Option<String>,
    progress: i64,
    error_code: Option<String>,
    cancel_requested: i64,
    created_at: String,
    updated_at: String,
}

fn decode_task(raw: RawTask) -> Result<TaskRecord, TaskError> {
    let record = TaskRecord {
        id: Uuid::parse_str(&raw.id).map_err(|error| storage_message(error))?,
        workspace_id: raw.workspace_id,
        kind: raw.kind,
        state: TaskState::from_str(&raw.state).map_err(storage_message)?,
        payload_json: raw.payload_json,
        checkpoint_json: raw.checkpoint_json,
        attempt: u32::try_from(raw.attempt).map_err(storage_message)?,
        next_run_at: raw.next_run_at,
        progress: u8::try_from(raw.progress).map_err(storage_message)?,
        error_code: raw.error_code,
        cancel_requested: match raw.cancel_requested {
            0 => false,
            1 => true,
            value => return Err(storage_message(format!("invalid cancel flag {value}"))),
        },
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    };
    record
        .validate()
        .map_err(|error| TaskError::new("storage_error", error.to_string()))?;
    Ok(record)
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn storage_error(error: rusqlite::Error) -> TaskError {
    TaskError::new("storage_error", error.to_string())
}

fn storage_message(error: impl std::fmt::Display) -> TaskError {
    TaskError::new("storage_error", error.to_string())
}
