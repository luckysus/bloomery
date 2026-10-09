use super::model::{NewTask, TaskError, TaskRecord};
use super::repository;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskSource {
    pub workspace_id: String,
    pub run_id: Uuid,
    pub tool_call_id: Uuid,
}

pub fn create(
    connection: &mut Connection,
    task: NewTask,
    source: AgentTaskSource,
) -> Result<TaskRecord, TaskError> {
    task.validate()?;
    if task.workspace_id != source.workspace_id {
        return Err(TaskError::new(
            "invalid_task_source",
            "task and source must belong to the same workspace",
        ));
    }
    if source.run_id.is_nil() || source.tool_call_id.is_nil() {
        return Err(TaskError::new(
            "invalid_task_source",
            "run_id and tool_call_id must not be nil",
        ));
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    let run_state = transaction
        .query_row(
            "SELECT state FROM agent_runs WHERE workspace_id = ?1 AND id = ?2",
            params![source.workspace_id, source.run_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage_error)?;
    if run_state.as_deref() != Some("executing_tools") {
        return Err(TaskError::new(
            "invalid_task_source",
            "source run must be executing_tools",
        ));
    }

    let task_record = repository::create(&transaction, task).map_err(|error| error)?;
    transaction
        .execute(
            "INSERT INTO agent_task_sources
                (workspace_id, run_id, tool_call_id, task_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                source.workspace_id,
                source.run_id.to_string(),
                source.tool_call_id.to_string(),
                task_record.id.to_string(),
            ],
        )
        .map_err(|error| {
            if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                TaskError::new("duplicate_task_submission", "tool call already submitted")
            } else {
                storage_error(error)
            }
        })?;
    transaction.commit().map_err(storage_error)?;
    Ok(task_record)
}

fn storage_error(error: rusqlite::Error) -> TaskError {
    TaskError::new("storage_error", error.to_string())
}

/// Submission and its source share a transaction. Replaying an interrupted
/// tool call returns its existing task instead of repeating the computation.
pub fn submit_with(
    connection: &mut Connection,
    source: AgentTaskSource,
    submit: impl FnOnce(&Connection) -> Result<TaskRecord, String>,
) -> Result<TaskRecord, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    if let Some(task) = for_tool_call(&transaction, &source)? {
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(task);
    }
    validate_source(&transaction, &source)?;
    let task = submit(&transaction)?;
    attach_existing(&transaction, source, task.id)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(task)
}

pub fn for_tool_call(
    connection: &Connection,
    source: &AgentTaskSource,
) -> Result<Option<TaskRecord>, String> {
    let id = connection
        .query_row(
            "SELECT task_id FROM agent_task_sources
             WHERE workspace_id = ?1 AND run_id = ?2 AND tool_call_id = ?3",
            params![
                source.workspace_id,
                source.run_id.to_string(),
                source.tool_call_id.to_string()
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    id.map(|id| {
        let id = Uuid::parse_str(&id).map_err(|error| error.to_string())?;
        repository::get(connection, &source.workspace_id, id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "linked background task was not found".to_string())
    })
    .transpose()
}

pub fn attach_existing(
    connection: &Connection,
    source: AgentTaskSource,
    task_id: Uuid,
) -> Result<(), String> {
    if let Some(existing) = for_tool_call(connection, &source)? {
        return if existing.id == task_id {
            Ok(())
        } else {
            Err("tool call already belongs to another background task".to_string())
        };
    }
    validate_source(connection, &source)?;
    repository::get(connection, &source.workspace_id, task_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "background task was not found in the source workspace".to_string())?;
    connection
        .execute(
            "INSERT INTO agent_task_sources (workspace_id, run_id, tool_call_id, task_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                source.workspace_id,
                source.run_id.to_string(),
                source.tool_call_id.to_string(),
                task_id.to_string()
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn validate_source_for_submission(
    connection: &Connection,
    source: &AgentTaskSource,
) -> Result<(), String> {
    super::model::validate_identifier("workspace_id", &source.workspace_id)
        .map_err(|error| error.to_string())?;
    if source.run_id.is_nil() || source.tool_call_id.is_nil() {
        return Err("run_id and tool_call_id must not be nil".to_string());
    }
    let state = connection
        .query_row(
            "SELECT state FROM agent_runs WHERE workspace_id = ?1 AND id = ?2",
            params![source.workspace_id, source.run_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if state.as_deref() != Some("executing_tools") {
        return Err("source run must be executing_tools".to_string());
    }
    Ok(())
}

fn validate_source(connection: &Connection, source: &AgentTaskSource) -> Result<(), String> {
    validate_source_for_submission(connection, source)
}
