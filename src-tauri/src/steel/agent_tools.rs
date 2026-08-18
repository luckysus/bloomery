use crate::agent::protocol::PermissionRisk;
use crate::agent::runtime::{
    CancellationToken, ToolExecutionError, ToolFuture, ToolHandler, ToolRegistration,
};
use crate::agent::tool_repair::ToolSpec;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type SteelAgentGatewayFuture =
    Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'static>>;

pub trait SteelAgentGateway: Send + Sync {
    fn execute(
        &self,
        tool_name: &'static str,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> SteelAgentGatewayFuture;
}

struct GatewayTool {
    name: &'static str,
    gateway: Arc<dyn SteelAgentGateway>,
}

impl ToolHandler for GatewayTool {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        let name = self.name;
        let gateway = self.gateway.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::cancelled());
            }
            gateway
                .execute(name, arguments, cancellation)
                .await
                .map_err(|error| ToolExecutionError::new(format!("steel_{name}_failed"), error))
        })
    }
}

fn gateway_tool(
    id: &'static str,
    name: &'static str,
    input_schema: Value,
    risk: PermissionRisk,
    read_only: bool,
    gateway: Arc<dyn SteelAgentGateway>,
) -> ToolRegistration {
    ToolRegistration::new(
        ToolSpec {
            id: id.to_string(),
            name: name.to_string(),
            input_schema,
            risk,
        },
        read_only,
        Arc::new(GatewayTool { name, gateway }),
    )
}

fn query_schema(required: bool) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "query": {"type": "string"},
            "limit": {"type": "integer", "minimum": 1, "maximum": 50},
            "top_k": {"type": "integer", "minimum": 1, "maximum": 50},
            "steel_mark": {"type": "string"},
            "steel_grade": {"type": "string"},
            "records": {"type": "array"},
            "slab_width_min": {"type": "number"},
            "slab_width_max": {"type": "number"},
            "slab_thickness_min": {"type": "number"},
            "slab_thickness_max": {"type": "number"},
            "yield_rp02_min": {"type": "number"},
            "yield_rp02_max": {"type": "number"},
            "yield_strength_min": {"type": "number"},
            "yield_strength_max": {"type": "number"},
            "tensile_strength_min": {"type": "number"},
            "tensile_strength_max": {"type": "number"},
            "elongation_min": {"type": "number"},
            "elongation_max": {"type": "number"}
        },
        "additionalProperties": false
    });
    if required {
        schema["required"] = json!(["query"]);
    }
    schema
}

pub(crate) fn agent_gateway_tools(gateway: Arc<dyn SteelAgentGateway>) -> Vec<ToolRegistration> {
    vec![
        gateway_tool(
            "steel.search_literature",
            "search_literature",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "knowledge_base_ids": {"type": "array", "items": {"type": "string"}},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50},
                    "top_k": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.read_literature_section",
            "read_literature_section",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "mode": {"type": "string", "enum": ["toc", "abstract", "references", "section"]},
                    "chapter_number": {"type": "integer"},
                    "document_hint": {"type": "string"},
                    "folder_hint": {"type": "string"},
                    "part": {"type": "integer", "minimum": 1},
                    "language": {"type": "string"},
                    "reader_version": {"type": "string"},
                    "max_chars": {"type": "integer", "minimum": 1},
                    "context": {"type": "object"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50},
                    "top_k": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "required": ["query", "mode"],
                "additionalProperties": false
            }),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.query_production_data",
            "query_production_data",
            query_schema(false),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.query_composition_standard",
            "query_composition_standard",
            query_schema(false),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.query_process_standard",
            "query_process_standard",
            query_schema(false),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.ask_llm_with_context",
            "ask_llm_with_context",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "evidence": {"type": "array"},
                    "context": {"type": "object"},
                    "direct_answer": {"type": "boolean"}
                },
                "additionalProperties": true
            }),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.match_coil",
            "match_coil",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "yield_strength": {"type": "number"},
                    "tensile_strength": {"type": "number"},
                    "elongation": {"type": "number"},
                    "target_yield": {"type": "number"},
                    "target_tensile": {"type": "number"},
                    "target_elongation": {"type": "number"},
                    "yield_rp02_min": {"type": "number"},
                    "tensile_strength_min": {"type": "number"},
                    "elongation_min": {"type": "number"},
                    "tolerance": {"type": "number"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "additionalProperties": false
            }),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.get_model_status",
            "get_model_status",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "lineageId": {"type": "string"}
                },
                "additionalProperties": false
            }),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.predict_performance",
            "predict_performance",
            json!({
                "type": "object",
                "properties": {
                    "datasetId": {"type": "string"},
                    "trainingTaskId": {"type": "string"},
                    "featureValues": {"type": "array", "items": {"type": "number"}},
                    "dataset_id": {"type": "string"},
                    "training_task_id": {"type": "string"},
                    "feature_values": {"type": "array", "items": {"type": "number"}},
                    "params": {"type": "object"},
                    "query": {"type": "string"}
                },
                "additionalProperties": false
            }),
            PermissionRisk::Automatic,
            false,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.optimize_process",
            "optimize_process",
            json!({
                "type": "object",
                "properties": {
                    "datasetId": {"type": "string"},
                    "trainingTaskId": {"type": "string"},
                    "direction": {"type": "string", "enum": ["minimize", "maximize"]},
                    "objectiveColumns": {"type": "array", "items": {"type": "integer"}, "minItems": 1, "maxItems": 4},
                    "dataset_id": {"type": "string"},
                    "training_task_id": {"type": "string"},
                    "objective_columns": {"type": "array", "items": {"type": "integer"}, "minItems": 1, "maxItems": 4},
                    "bounds": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {"min": {"type": "number"}, "max": {"type": "number"}},
                            "required": ["min", "max"],
                            "additionalProperties": false
                        }
                    },
                    "fixedValues": {"type": "array", "items": {"type": ["number", "null"]}},
                    "fixed_values": {"type": "array", "items": {"type": ["number", "null"]}},
                    "constraints": {"type": "array"},
                    "trials": {"type": "integer", "minimum": 1, "maximum": 500},
                    "seed": {"type": "integer"},
                    "filters": {"type": "object"},
                    "context": {"type": "object"},
                    "query": {"type": "string"}
                },
                "additionalProperties": false
            }),
            PermissionRisk::ConfirmationRequired,
            false,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.start_training",
            "start_training",
            json!({
                "type": "object",
                "properties": {
                    "datasetId": {"type": "string"},
                    "targetColumn": {"type": "integer"},
                    "featureColumns": {"type": "array", "items": {"type": "integer"}, "minItems": 1},
                    "dataset_id": {"type": "string"},
                    "target_column": {"type": "integer"},
                    "feature_columns": {"type": "array", "items": {"type": "integer"}, "minItems": 1},
                    "splitPolicy": {"type": "object"},
                    "split_policy": {"type": "object"},
                    "algorithm": {"type": "string", "enum": ["linear_regression", "elasticnet", "random_forest", "hist_gradient_boosting"]},
                    "model_version": {"type": "string"},
                    "query": {"type": "string"}
                },
                "additionalProperties": false
            }),
            PermissionRisk::Dangerous,
            false,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.process_literature",
            "process_literature",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "file_path": {"type": "string"},
                    "knowledge_base_id": {"type": "string"},
                    "knowledge_base_name": {"type": "string"},
                    "embedding_profile_id": {"type": "string"},
                    "embedding_dimension": {"type": "integer", "minimum": 1},
                    "mineru_profile_id": {"type": "string"}
                },
                "additionalProperties": true
            }),
            PermissionRisk::ConfirmationRequired,
            false,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.export_data",
            "export_data",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "format": {"type": "string", "enum": ["xlsx", "csv", "json"]}
                },
                "additionalProperties": true
            }),
            PermissionRisk::ConfirmationRequired,
            false,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.remember_memory",
            "remember_memory",
            json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "summary": {"type": "string"},
                    "query": {"type": "string"},
                    "memory_type": {"type": "string"},
                    "keywords": {"type": "array", "items": {"type": "string"}},
                    "payload": {"type": "object"},
                    "session_id": {"type": "string"},
                    "run_id": {"type": "string"},
                    "source": {"type": "string"}
                },
                "required": ["summary"],
                "additionalProperties": true
            }),
            PermissionRisk::Automatic,
            false,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.read_memory",
            "read_memory",
            json!({
                "type": "object",
                "properties": {"memory_id": {"type": "string"}, "query": {"type": "string"}},
                "required": ["memory_id"],
                "additionalProperties": true
            }),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.search_memory",
            "search_memory",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "memory_type": {"type": "string"},
                    "filters": {"type": "object"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "required": ["query"],
                "additionalProperties": true
            }),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.list_memory",
            "list_memory",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "memory_type": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "additionalProperties": true
            }),
            PermissionRisk::Automatic,
            true,
            gateway.clone(),
        ),
        gateway_tool(
            "steel.forget_memory",
            "forget_memory",
            json!({
                "type": "object",
                "properties": {"memory_id": {"type": "string"}, "query": {"type": "string"}},
                "required": ["memory_id"],
                "additionalProperties": true
            }),
            PermissionRisk::ConfirmationRequired,
            false,
            gateway,
        ),
    ]
}
