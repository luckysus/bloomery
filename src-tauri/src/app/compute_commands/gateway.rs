use super::logic::{self, OptimizeSteelProcessRequest};
use crate::steel::OptimizationGateway;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Agent-facing gateway bound to the workspace database file. Each call opens
/// a short-lived connection so tool executions never hold locks across await
/// points.
pub struct DesktopOptimizationGateway {
    database: PathBuf,
}

impl DesktopOptimizationGateway {
    pub fn new(database: PathBuf) -> Self {
        Self { database }
    }

    fn open(&self) -> Result<rusqlite::Connection, String> {
        let (connection, _) = crate::storage::database::open(&self.database)
            .map_err(|error| format!("open optimization database failed: {error}"))?;
        Ok(connection)
    }
}

impl OptimizationGateway for DesktopOptimizationGateway {
    fn submit(&self, arguments: Value) -> Result<Value, String> {
        let request: OptimizeSteelProcessRequest = serde_json::from_value(arguments)
            .map_err(|error| format!("invalid optimization request: {error}"))?;
        let training_task_id = uuid::Uuid::parse_str(&request.training_task_id)
            .map_err(|error| format!("invalid training task ID: {error}"))?;
        let mut connection = self.open()?;
        let task =
            logic::submit_optimization_on_connection(&mut connection, &request, training_task_id)?;
        Ok(json!(
            crate::app::task_commands::tasks::background_task_response(task)
        ))
    }

    fn status(&self, task_id: &str) -> Result<Value, String> {
        let id =
            uuid::Uuid::parse_str(task_id).map_err(|error| format!("invalid task ID: {error}"))?;
        let connection = self.open()?;
        logic::optimization_task_status_on_connection(&connection, id)
    }
}
