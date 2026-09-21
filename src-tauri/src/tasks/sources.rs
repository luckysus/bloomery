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
