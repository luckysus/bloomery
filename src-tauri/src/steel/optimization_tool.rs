use crate::agent::protocol::PermissionRisk;
use crate::agent::runtime::{
    CancellationToken, ToolExecutionError, ToolFuture, ToolHandler, ToolRegistration,
};
use crate::agent::tool_repair::ToolSpec;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Storage-backed surface the Agent uses to queue and inspect optimization
/// tasks. The desktop runtime binds it to the workspace database; tests bind
/// fakes so tool behavior stays verifiable without a desktop runtime.
/// Arguments travel as JSON values so the trait stays public while the typed
/// request DTO remains inside the app layer.
pub trait OptimizationGateway: Send + Sync {
    fn submit(&self, arguments: Value) -> Result<Value, String>;
    fn status(&self, task_id: &str) -> Result<Value, String>;
}

struct OptimizeConstrainedTool {
    gateway: Arc<dyn OptimizationGateway>,
}

impl ToolHandler for OptimizeConstrainedTool {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        let gateway = self.gateway.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::new(
                    "steel_optimization_cancelled",
                    "optimization was cancelled before submission",
                ));
            }
            validate_optimization_arguments(&arguments)?;
            gateway.submit(arguments).map_err(|error| {
                ToolExecutionError::new("steel_optimization_submit_failed", error)
            })
        })
    }
}

fn validate_optimization_arguments(arguments: &Value) -> Result<(), ToolExecutionError> {
    #[derive(Debug, Deserialize)]
    struct Shape {
        #[serde(rename = "datasetId")]
        dataset_id: String,
        #[serde(rename = "trainingTaskId")]
        training_task_id: String,
        direction: String,
        #[serde(rename = "objectiveColumns")]
        objective_columns: Vec<usize>,
        bounds: Vec<Value>,
        trials: u32,
    }
    let shape: Shape = serde_json::from_value(arguments.clone()).map_err(|error| {
        ToolExecutionError::new("steel_optimization_invalid", error.to_string())
    })?;
    if shape.dataset_id.trim().is_empty() || shape.training_task_id.trim().is_empty() {
        return Err(ToolExecutionError::new(
            "steel_optimization_invalid",
            "datasetId and trainingTaskId are required",
        ));
    }
    if !matches!(shape.direction.as_str(), "minimize" | "maximize") {
        return Err(ToolExecutionError::new(
            "steel_optimization_invalid",
            "direction must be minimize or maximize",
        ));
    }
    if shape.objective_columns.is_empty() || shape.objective_columns.len() > 4 {
        return Err(ToolExecutionError::new(
            "steel_optimization_invalid",
            "objectiveColumns must contain 1 to 4 entries",
        ));
    }
    if shape.bounds.is_empty() {
        return Err(ToolExecutionError::new(
            "steel_optimization_invalid",
            "bounds must cover every model feature",
        ));
    }
    if !(1..=500).contains(&shape.trials) {
        return Err(ToolExecutionError::new(
            "steel_optimization_invalid",
            "trials must be between 1 and 500",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct OptimizationStatusRequest {
    task_id: String,
}

struct OptimizationStatusTool {
    gateway: Arc<dyn OptimizationGateway>,
}

impl ToolHandler for OptimizationStatusTool {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        let gateway = self.gateway.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::new(
                    "steel_optimization_cancelled",
                    "optimization status lookup was cancelled",
                ));
            }
            let request = serde_json::from_value::<OptimizationStatusRequest>(arguments)
                .map_err(|error| {
                    ToolExecutionError::new("steel_optimization_invalid", error.to_string())
                })?;
            gateway.status(&request.task_id).map_err(|error| {
                ToolExecutionError::new("steel_optimization_status_failed", error)
            })
        })
    }
}

pub fn optimize_constrained_tool(gateway: Arc<dyn OptimizationGateway>) -> ToolRegistration {
    ToolRegistration::new(
        ToolSpec {
            id: "steel.optimize_constrained".to_string(),
            name: "optimize_constrained".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "datasetId": {"type": "string"},
                    "trainingTaskId": {"type": "string"},
                    "direction": {"type": "string", "enum": ["minimize", "maximize"]},
                    "objectiveColumns": {"type": "array", "items": {"type": "integer"}, "minItems": 1, "maxItems": 4},
                    "bounds": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {"min": {"type": "number"}, "max": {"type": "number"}},
                            "required": ["min", "max"],
                            "additionalProperties": false
                        }
                    },
                    "fixedValues": {
                        "type": "array",
                        "items": {"type": ["number", "null"]}
                    },
                    "constraints": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": {"type": "string", "enum": ["equality", "inequality"]},
                                "coefficients": {"type": "array", "items": {"type": "number"}},
                                "value": {"type": "number"},
                                "tolerance": {"type": "number"}
                            },
                            "required": ["kind", "coefficients", "value"],
                            "additionalProperties": false
                        }
                    },
                    "trials": {"type": "integer", "minimum": 1, "maximum": 500},
                    "seed": {"type": "integer"}
                },
                "required": ["datasetId", "trainingTaskId", "direction", "objectiveColumns", "bounds", "trials"],
                "additionalProperties": false
            }),
            risk: PermissionRisk::ConfirmationRequired,
        },
        true,
        Arc::new(OptimizeConstrainedTool { gateway }),
    )
}

pub fn optimization_status_tool(gateway: Arc<dyn OptimizationGateway>) -> ToolRegistration {
    ToolRegistration::new(
        ToolSpec {
            id: "steel.optimization_status".to_string(),
            name: "optimization_status".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_id": {"type": "string"}
                },
                "required": ["task_id"],
                "additionalProperties": false
            }),
            risk: PermissionRisk::Automatic,
        },
        true,
        Arc::new(OptimizationStatusTool { gateway }),
    )
}
