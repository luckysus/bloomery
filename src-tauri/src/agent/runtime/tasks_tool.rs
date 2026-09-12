use super::{
    CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation,
    ToolRegistration,
};
use crate::agent::protocol::PermissionRisk;
use crate::agent::tool_repair::ToolSpec;
use crate::tasks::{repository, TaskRecord};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const TASKS_TOOL_ID: &str = "agent.list_background_tasks";
const TASKS_TOOL_NAME: &str = "list_background_tasks";

pub struct BackgroundTasksTool {
    registration: ToolRegistration,
}

impl BackgroundTasksTool {
    pub fn from_connection(connection: &Connection, workspace_id: &str) -> Result<Self, String> {
        crate::tasks::model::validate_identifier("workspace_id", workspace_id)
            .map_err(|error| error.to_string())?;
        let path = connection
            .path()
            .filter(|path| !path.is_empty())
            .ok_or("background task queries require a persistent database")?;
        let database = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
        Ok(Self {
            registration: ToolRegistration::new(
                ToolSpec {
                    id: TASKS_TOOL_ID.to_string(),
                    name: TASKS_TOOL_NAME.to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {},
                        "required": [],
                        "additionalProperties": false
                    }),
                    risk: PermissionRisk::Automatic,
                },
                true,
                Arc::new(BackgroundTasksHandler {
                    database,
                    workspace_id: workspace_id.to_string(),
                }),
            ),
        })
    }
}

impl ToolExecutor for BackgroundTasksTool {
    fn registrations(&self) -> &[ToolRegistration] {
        std::slice::from_ref(&self.registration)
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        if invocation.tool_id != TASKS_TOOL_ID || invocation.tool_name != TASKS_TOOL_NAME {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "background task tool is not registered",
                ))
            });
        }
        self.registration
            .handler
            .execute(invocation.arguments, cancellation)
    }
}

struct BackgroundTasksHandler {
    database: PathBuf,
    workspace_id: String,
}

impl ToolHandler for BackgroundTasksHandler {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        let database = self.database.clone();
        let workspace_id = self.workspace_id.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::cancelled());
            }
            if !arguments.is_object()
                || arguments.as_object().is_some_and(|value| !value.is_empty())
            {
                return Err(ToolExecutionError::new(
                    "invalid_task_query",
                    "list_background_tasks does not accept arguments",
                ));
            }
            let connection =
                Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(
                    |error| ToolExecutionError::new("task_query_failed", error.to_string()),
                )?;
            connection
                .busy_timeout(Duration::from_secs(5))
                .map_err(|error| ToolExecutionError::new("task_query_failed", error.to_string()))?;
            let tasks = repository::list(&connection, &workspace_id)
                .map_err(|error| ToolExecutionError::new("task_query_failed", error.to_string()))?;
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::cancelled());
            }
            Ok(json!({
                "tasks": tasks.iter().map(task_payload).collect::<Vec<_>>(),
                "source": "local_sqlite_scheduler",
            }))
        })
    }
}

fn task_payload(task: &TaskRecord) -> Value {
    json!({
        "id": task.id,
        "kind": task.kind,
        "state": task.state,
        "progress": task.progress,
        "attempt": task.attempt,
        "error_code": task.error_code,
        "cancel_requested": task.cancel_requested,
        "created_at": task.created_at,
        "updated_at": task.updated_at,
        "started_at": task.started_at,
        "finished_at": task.finished_at,
    })
}
