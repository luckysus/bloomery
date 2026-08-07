use crate::steel::{
    calculate_carbon_equivalent, preview_dataset, CarbonEquivalentFormula, CarbonEquivalentResult,
    CompositionInput, CompositionUnit, DatasetPreview, DatasetPreviewRequest,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarbonEquivalentRequest {
    pub formula: CarbonEquivalentFormula,
    pub unit: CompositionUnit,
    pub composition: BTreeMap<String, f64>,
}

#[tauri::command]
pub fn calculate_steel_carbon_equivalent(
    request: CarbonEquivalentRequest,
) -> Result<CarbonEquivalentResult, String> {
    calculate_carbon_equivalent(
        &CompositionInput {
            values: request.composition,
            unit: request.unit,
        },
        request.formula,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_steel_dataset(request: DatasetPreviewRequest) -> Result<DatasetPreview, String> {
    preview_dataset(&request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_returns_a_versioned_carbon_equivalent() {
        let result = calculate_steel_carbon_equivalent(CarbonEquivalentRequest {
            formula: CarbonEquivalentFormula::Iiw,
            unit: CompositionUnit::PercentMass,
            composition: BTreeMap::from([
                ("C".to_string(), 0.2),
                ("Mn".to_string(), 1.0),
                ("Cr".to_string(), 0.25),
                ("Mo".to_string(), 0.05),
                ("V".to_string(), 0.02),
                ("Ni".to_string(), 0.2),
                ("Cu".to_string(), 0.3),
            ]),
        })
        .expect("carbon equivalent command");

        assert_eq!(result.formula_id, "carbon-equivalent.iiw.v1");
        assert!((result.value - 0.464).abs() < 1e-9);
    }
}
