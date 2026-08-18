use super::optimization_tool::{
    optimization_status_tool, optimize_constrained_tool, OptimizationGateway,
};
use super::{
    agent_tools::{agent_gateway_tools, SteelAgentGateway},
    calculate_carbon_equivalent, CarbonEquivalentFormula, CompositionInput, CompositionUnit,
};
use crate::agent::protocol::PermissionRisk;
use crate::agent::runtime::{
    CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation,
    ToolRegistration,
};
use crate::agent::tool_repair::ToolSpec;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct CarbonEquivalentRequest {
    formula: CarbonEquivalentFormula,
    unit: CompositionUnit,
    composition: BTreeMap<String, f64>,
}

struct CarbonEquivalentTool;

impl ToolHandler for CarbonEquivalentTool {
    fn execute(&self, arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
        Box::pin(async move {
            let request =
                serde_json::from_value::<CarbonEquivalentRequest>(arguments).map_err(|error| {
                    ToolExecutionError::new("steel_calculation_invalid", error.to_string())
                })?;
            let result = calculate_carbon_equivalent(
                &CompositionInput {
                    values: request.composition,
                    unit: request.unit,
                },
                request.formula,
            )
            .map_err(|error| {
                ToolExecutionError::new("steel_calculation_invalid", error.to_string())
            })?;
            serde_json::to_value(result).map_err(|error| {
                ToolExecutionError::new("steel_calculation_failed", error.to_string())
            })
        })
    }
}

pub fn carbon_equivalent_tool() -> ToolRegistration {
    ToolRegistration::new(
        ToolSpec {
            id: "steel.carbon_equivalent".to_string(),
            name: "carbon_equivalent".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "formula": {"type": "string", "enum": ["iiw", "pcm"]},
                    "unit": {"type": "string", "enum": ["percent_mass", "mass_fraction"]},
                    "composition": {"type": "object", "additionalProperties": {"type": "number"}}
                },
                "required": ["formula", "unit", "composition"],
                "additionalProperties": false
            }),
            risk: PermissionRisk::Automatic,
        },
        true,
        Arc::new(CarbonEquivalentTool),
    )
}

pub struct SteelToolExecutor {
    registrations: Vec<ToolRegistration>,
}

impl SteelToolExecutor {
    pub fn new(tool_calls_enabled: bool) -> Self {
        Self {
            registrations: tool_calls_enabled
                .then(|| carbon_equivalent_tool())
                .into_iter()
                .collect(),
        }
    }

    pub fn with_optimization_gateway(
        gateway: Arc<dyn OptimizationGateway>,
        tool_calls_enabled: bool,
    ) -> Self {
        let mut registrations: Vec<ToolRegistration> = tool_calls_enabled
            .then(|| carbon_equivalent_tool())
            .into_iter()
            .collect();
        if tool_calls_enabled {
            registrations.push(optimize_constrained_tool(gateway.clone()));
            registrations.push(optimization_status_tool(gateway));
        }
        Self { registrations }
    }

    pub fn with_agent_gateways(
        optimization_gateway: Arc<dyn OptimizationGateway>,
        agent_gateway: Arc<dyn SteelAgentGateway>,
        tool_calls_enabled: bool,
    ) -> Self {
        let mut registrations =
            Self::with_optimization_gateway(optimization_gateway, tool_calls_enabled).registrations;
        if tool_calls_enabled {
            registrations.extend(agent_gateway_tools(agent_gateway));
        }
        Self { registrations }
    }
}

impl ToolExecutor for SteelToolExecutor {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        let Some(registration) = self.registrations.iter().find(|registration| {
            registration.spec.id == invocation.tool_id
                && registration.spec.name == invocation.tool_name
        }) else {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "steel tool is not registered",
                ))
            });
        };
        registration
            .handler
            .execute(invocation.arguments, cancellation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tool_repair::repair_tool_call;
    use crate::steel::SteelAgentGatewayFuture;
    use serde_json::{json, Value};

    struct FakeOptimizationGateway;

    impl OptimizationGateway for FakeOptimizationGateway {
        fn submit(&self, _arguments: Value) -> Result<Value, String> {
            Ok(json!({"id": "optimization-task-1"}))
        }

        fn status(&self, _task_id: &str) -> Result<Value, String> {
            Ok(json!({"state": "queued"}))
        }
    }

    struct FakeAgentGateway;

    impl SteelAgentGateway for FakeAgentGateway {
        fn execute(
            &self,
            tool_name: &'static str,
            _arguments: Value,
            _cancellation: CancellationToken,
        ) -> SteelAgentGatewayFuture {
            Box::pin(async move { Ok(json!({"tool": tool_name, "success": true})) })
        }
    }

    #[test]
    fn steel_agent_tools_expose_web_parity_names() {
        let tools = SteelToolExecutor::with_agent_gateways(
            Arc::new(FakeOptimizationGateway),
            Arc::new(FakeAgentGateway),
            true,
        );
        let names = tools
            .registrations()
            .iter()
            .map(|registration| registration.spec.name.as_str())
            .collect::<Vec<_>>();

        for expected in [
            "search_literature",
            "read_literature_section",
            "query_production_data",
            "query_composition_standard",
            "query_process_standard",
            "ask_llm_with_context",
            "get_model_status",
            "predict_performance",
            "optimize_process",
            "match_coil",
            "start_training",
            "process_literature",
            "export_data",
            "remember_memory",
            "read_memory",
            "search_memory",
            "list_memory",
            "forget_memory",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
    }

    #[test]
    fn steel_agent_tools_keep_desktop_safety_boundaries() {
        let tools = SteelToolExecutor::with_agent_gateways(
            Arc::new(FakeOptimizationGateway),
            Arc::new(FakeAgentGateway),
            true,
        );
        let registration = |name: &str| {
            tools
                .registrations()
                .iter()
                .find(|registration| registration.spec.name == name)
                .unwrap_or_else(|| panic!("missing tool {name}"))
        };

        assert_eq!(
            registration("process_literature").spec.risk,
            PermissionRisk::ConfirmationRequired
        );
        assert_eq!(
            registration("export_data").spec.risk,
            PermissionRisk::ConfirmationRequired
        );
        assert!(registration("ask_llm_with_context").read_only);
        assert!(!registration("process_literature").read_only);
        assert!(!registration("export_data").read_only);
    }

    #[test]
    fn steel_compute_tools_accept_web_style_agent_arguments() {
        let tools = SteelToolExecutor::with_agent_gateways(
            Arc::new(FakeOptimizationGateway),
            Arc::new(FakeAgentGateway),
            true,
        );
        let specs = tools
            .registrations()
            .iter()
            .map(|registration| registration.spec.clone())
            .collect::<Vec<_>>();

        let prediction = repair_tool_call(
            r#"{"name":"predict_performance","arguments":{"query":"预测 Q355B","params":{"C":0.12,"Mn":1.4}}}"#,
            &specs,
        )
        .expect("web prediction arguments pass schema");
        assert_eq!(prediction.tool_name, "predict_performance");

        let optimization = repair_tool_call(
            r#"{"name":"optimize_process","arguments":{"query":"优化工艺","filters":{"target_yield":460},"context":{"composition":{"C":0.12}}}}"#,
            &specs,
        )
        .expect("web optimization arguments pass schema");
        assert_eq!(optimization.tool_name, "optimize_process");

        let training = repair_tool_call(
            r#"{"name":"start_training","arguments":{"query":"重新训练","model_version":"agent_v1"}}"#,
            &specs,
        )
        .expect("web training arguments pass schema");
        assert_eq!(training.tool_name, "start_training");
    }

    #[test]
    fn steel_agent_tools_accept_web_style_query_arguments() {
        let tools = SteelToolExecutor::with_agent_gateways(
            Arc::new(FakeOptimizationGateway),
            Arc::new(FakeAgentGateway),
            true,
        );
        let specs = tools
            .registrations()
            .iter()
            .map(|registration| registration.spec.clone())
            .collect::<Vec<_>>();

        for (raw, expected) in [
            (
                r#"{"name":"search_literature","arguments":{"query":"Q355B 析出强化","top_k":8}}"#,
                "search_literature",
            ),
            (
                r#"{"name":"read_literature_section","arguments":{"query":"读取第二章","mode":"section","chapter_number":2,"part":1,"language":"zh","reader_version":"v1","max_chars":12000,"context":{"session_id":"s1"}}}"#,
                "read_literature_section",
            ),
            (
                r#"{"name":"query_production_data","arguments":{"query":"查 Q355B","steel_mark":"Q355B","steel_grade":"Q355B","yield_rp02_min":355,"yield_rp02_max":500,"limit":20}}"#,
                "query_production_data",
            ),
            (
                r#"{"name":"query_composition_standard","arguments":{"query":"Q355B 成分","steel_mark":"Q355B","steel_grade":"Q355B","records":[{"steel_mark":"Q355B"}]}}"#,
                "query_composition_standard",
            ),
            (
                r#"{"name":"match_coil","arguments":{"query":"找相近钢卷","target_yield":460,"target_tensile":550,"target_elongation":21}}"#,
                "match_coil",
            ),
            (
                r#"{"name":"search_memory","arguments":{"query":"用户偏好","filters":{"memory_type":"preference"},"session_id":"s1"}}"#,
                "search_memory",
            ),
        ] {
            let call = repair_tool_call(raw, &specs)
                .unwrap_or_else(|error| panic!("{expected} rejected web arguments: {error}"));
            assert_eq!(call.tool_name, expected);
        }
    }
}
