use bloomery::agent::runtime::{CancellationToken, ToolExecutor};
use bloomery::steel::{
    calculate_carbon_equivalent, carbon_equivalent_tool, CarbonEquivalentFormula, CompositionInput,
    CompositionUnit, SteelCalculationError, SteelToolExecutor,
};
use std::collections::BTreeMap;

fn composition(values: &[(&str, f64)], unit: CompositionUnit) -> CompositionInput {
    CompositionInput {
        values: values
            .iter()
            .map(|(element, value)| ((*element).to_string(), *value))
            .collect::<BTreeMap<_, _>>(),
        unit,
    }
}

#[test]
fn iiw_carbon_equivalent_returns_formula_and_normalized_inputs() {
    let result = calculate_carbon_equivalent(
        &composition(
            &[
                ("C", 0.20),
                ("Mn", 1.00),
                ("Cr", 0.25),
                ("Mo", 0.05),
                ("V", 0.02),
                ("Ni", 0.20),
                ("Cu", 0.30),
            ],
            CompositionUnit::PercentMass,
        ),
        CarbonEquivalentFormula::Iiw,
    )
    .expect("IIW calculation");

    assert_eq!(result.formula_id, "carbon-equivalent.iiw.v1");
    assert_eq!(result.normalized_inputs["C"], 0.20);
    assert!((result.value - 0.464).abs() < 1e-9);
    assert!(result.expression.contains("Mn/6"));
    assert!(!result.applicability_note.is_empty());
}

#[test]
fn pcm_formula_normalizes_fraction_input() {
    let result = calculate_carbon_equivalent(
        &composition(
            &[
                ("C", 0.0018),
                ("Si", 0.0025),
                ("Mn", 0.012),
                ("Cu", 0.001),
                ("Cr", 0.002),
                ("Ni", 0.001),
                ("Mo", 0.0005),
                ("V", 0.0002),
                ("B", 0.00001),
            ],
            CompositionUnit::MassFraction,
        ),
        CarbonEquivalentFormula::Pcm,
    )
    .expect("Pcm calculation");

    assert_eq!(result.formula_id, "carbon-equivalent.pcm.v1");
    assert_eq!(result.normalized_inputs["C"], 0.18);
    assert!((result.value - 0.2753333333333333).abs() < 1e-9);
}

#[test]
fn calculator_rejects_missing_and_unknown_elements() {
    let missing = calculate_carbon_equivalent(
        &composition(&[("C", 0.2)], CompositionUnit::PercentMass),
        CarbonEquivalentFormula::Iiw,
    )
    .expect_err("missing formula elements must be rejected");
    assert!(matches!(
        missing,
        SteelCalculationError::MissingElement { .. }
    ));

    let unknown = calculate_carbon_equivalent(
        &composition(&[("C", 0.2), ("Xx", 99.8)], CompositionUnit::PercentMass),
        CarbonEquivalentFormula::Iiw,
    )
    .expect_err("unknown element names must be rejected");
    assert!(matches!(
        unknown,
        SteelCalculationError::UnknownElement { .. }
    ));
}

#[test]
fn calculator_accepts_common_steel_elements_not_used_by_the_formula() {
    let result = calculate_carbon_equivalent(
        &composition(
            &[
                ("C", 0.20),
                ("Mn", 1.00),
                ("Cr", 0.25),
                ("Mo", 0.05),
                ("V", 0.02),
                ("Ni", 0.20),
                ("Cu", 0.30),
                ("Fe", 97.0),
                ("P", 0.01),
                ("S", 0.01),
            ],
            CompositionUnit::PercentMass,
        ),
        CarbonEquivalentFormula::Iiw,
    )
    .expect("common steel elements should be accepted");

    assert_eq!(result.normalized_inputs["Fe"], 97.0);
    assert!((result.value - 0.464).abs() < 1e-9);
}

#[test]
fn calculator_rejects_invalid_values_without_silent_coercion() {
    let error = calculate_carbon_equivalent(
        &composition(&[("C", 1.2)], CompositionUnit::MassFraction),
        CarbonEquivalentFormula::Iiw,
    )
    .expect_err("mass fractions above one must be rejected");

    assert!(matches!(
        error,
        SteelCalculationError::ValueOutOfRange { .. }
    ));
}

#[tokio::test]
async fn carbon_equivalent_tool_returns_an_audited_result() {
    let registration = carbon_equivalent_tool();
    assert_eq!(registration.spec.id, "steel.carbon_equivalent");
    assert_eq!(
        registration.spec.risk,
        bloomery::agent::protocol::PermissionRisk::Automatic
    );
    assert_eq!(
        registration.spec.input_schema["required"],
        serde_json::json!(["formula", "unit", "composition"])
    );

    let output = registration
        .handler
        .execute(
            serde_json::json!({
                "formula": "iiw",
                "unit": "percent_mass",
                "composition": {
                    "C": 0.2,
                    "Mn": 1.0,
                    "Cr": 0.25,
                    "Mo": 0.05,
                    "V": 0.02,
                    "Ni": 0.2,
                    "Cu": 0.3
                }
            }),
            CancellationToken::new(|| false),
        )
        .await
        .expect("tool calculation");

    assert_eq!(output["formula_id"], "carbon-equivalent.iiw.v1");
    assert_eq!(output["unit"], "percent_mass");
    assert!((output["value"].as_f64().expect("numeric result") - 0.464).abs() < 1e-9);
    assert!(output["applicability_note"]
        .as_str()
        .is_some_and(|note| !note.is_empty()));
}

#[test]
fn steel_tool_executor_can_disable_tool_calls_for_limited_models() {
    assert_eq!(SteelToolExecutor::new(true).registrations().len(), 1);
    assert!(SteelToolExecutor::new(false).registrations().is_empty());
}
