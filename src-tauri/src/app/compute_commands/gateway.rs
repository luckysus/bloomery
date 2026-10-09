use super::logic::{self, OptimizeSteelProcessRequest};
use crate::steel::OptimizationGateway;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Agent-facing gateway bound to the workspace database file. Each call opens
/// a short-lived connection so tool executions never hold locks across await
/// points.
pub struct DesktopOptimizationGateway {
    database: PathBuf,
    run_id: Option<uuid::Uuid>,
}

impl DesktopOptimizationGateway {
    pub fn new(database: PathBuf) -> Self {
        Self {
            database,
            run_id: None,
        }
    }

    pub fn with_run_id(mut self, run_id: uuid::Uuid) -> Self {
        self.run_id = Some(run_id);
        self
    }

    fn open(&self) -> Result<rusqlite::Connection, String> {
        let (connection, _) = crate::storage::database::open(&self.database)
            .map_err(|error| format!("open optimization database failed: {error}"))?;
        Ok(connection)
    }

    fn submit_task(
        &self,
        arguments: Value,
        tool_call_id: Option<uuid::Uuid>,
    ) -> Result<Value, String> {
        let request: OptimizeSteelProcessRequest = serde_json::from_value(arguments)
            .map_err(|error| format!("invalid optimization request: {error}"))?;
        let training_task_id = uuid::Uuid::parse_str(&request.training_task_id)
            .map_err(|error| format!("invalid training task ID: {error}"))?;
        let mut connection = self.open()?;
        let task = match (self.run_id, tool_call_id) {
            (Some(run_id), Some(tool_call_id)) => crate::tasks::sources::submit_with(
                &mut connection,
                crate::tasks::sources::AgentTaskSource {
                    workspace_id: crate::db::current_workspace_id().to_string(),
                    run_id,
                    tool_call_id,
                },
                |connection| {
                    logic::submit_optimization_on_connection(connection, &request, training_task_id)
                },
            )?,
            _ => logic::submit_optimization_on_connection(&connection, &request, training_task_id)?,
        };
        Ok(json!(
            crate::app::task_commands::tasks::background_task_response(task)
        ))
    }
}

impl OptimizationGateway for DesktopOptimizationGateway {
    fn submit(&self, arguments: Value) -> Result<Value, String> {
        self.submit_task(arguments, None)
    }

    fn submit_for_tool_call(
        &self,
        arguments: Value,
        tool_call_id: uuid::Uuid,
    ) -> Result<Value, String> {
        self.submit_task(arguments, Some(tool_call_id))
    }

    fn status(&self, task_id: &str) -> Result<Value, String> {
        let id =
            uuid::Uuid::parse_str(task_id).map_err(|error| format!("invalid task ID: {error}"))?;
        let connection = self.open()?;
        logic::optimization_task_status_on_connection(&connection, id)
    }
}
