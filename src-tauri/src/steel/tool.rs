use super::{
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
