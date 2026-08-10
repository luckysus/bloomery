use super::{
    calculate_carbon_equivalent, CarbonEquivalentFormula, CompositionInput, CompositionUnit,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const VALUE_TOLERANCE: f64 = 1e-9;

/// Outcome of one evaluation category. `score` stays `None` for categories
/// executed outside this runner (worker or provider); their thresholds are
/// still carried so reports never silently drop them.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CategoryReport {
    pub category: String,
    pub runner: String,
    pub threshold: f64,
    pub score: Option<f64>,
    pub passed: usize,
    pub total: usize,
    pub failures: Vec<String>,
}

impl CategoryReport {
    pub fn meets_threshold(&self) -> bool {
        self.score.is_some_and(|score| score >= self.threshold)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EvaluationReport {
    pub evaluation_version: String,
    pub categories: Vec<CategoryReport>,
}

pub fn parse_suite(text: &str) -> Result<Value, String> {
    let suite: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if suite["schema_version"] != "1.0.0" {
        return Err("unsupported evaluation schema version".to_string());
    }
    if suite["categories"].get("calculators").is_none() {
        return Err("evaluation suite must declare categories".to_string());
    }
    Ok(suite)
}

/// Execute every category owned by this runner and carry deferred categories
/// with their thresholds intact. Failures are recorded verbatim; the report
/// never rounds a failing score up to a passing one.
pub fn run_rust_categories(suite: &Value, package_root: &Path) -> Result<EvaluationReport, String> {
    let categories = suite["categories"]
        .as_object()
        .ok_or_else(|| "evaluation categories must be an object".to_string())?;
    let cases = suite["cases"]
        .as_object()
        .ok_or_else(|| "evaluation cases must be an object".to_string())?;

    let mut reports = Vec::new();
    for (name, spec) in categories {
        let runner = spec["runner"].as_str().unwrap_or("unknown").to_string();
        let threshold = spec["threshold"]
            .as_f64()
            .ok_or_else(|| format!("category {name} has no threshold"))?;
        let category_cases = cases
            .get(name)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut report = match runner.as_str() {
            "rust" => run_category(name, &category_cases, package_root)?,
            "worker" | "provider" => CategoryReport {
                category: name.clone(),
                runner: runner.clone(),
                threshold,
                score: None,
                passed: 0,
                total: category_cases.len(),
                failures: Vec::new(),
            },
            other => return Err(format!("category {name} has unknown runner {other}")),
        };
        report.threshold = threshold;
        reports.push(report);
    }
    Ok(EvaluationReport {
        evaluation_version: suite["evaluation_version"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        categories: reports,
    })
}

fn run_category(
    name: &str,
    cases: &[Value],
    package_root: &Path,
) -> Result<CategoryReport, String> {
    let mut passed = 0;
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap_or("unnamed").to_string();
        match match name {
            "calculators" => evaluate_calculator_case(case),
            "dataset_mapping" => evaluate_mapping_case(case, package_root),
            "dataset_profiling" => evaluate_profiling_case(case),
            "terminology" => evaluate_terminology_case(case, package_root),
            other => Err(format!("no rust evaluator for category {other}")),
        } {
            Ok(()) => passed += 1,
            Err(reason) => failures.push(format!("{id}: {reason}")),
        }
    }
    let total = cases.len();
    let score = if total == 0 {
        0.0
    } else {
        passed as f64 / total as f64
    };
    Ok(CategoryReport {
        category: name.to_string(),
        runner: "rust".to_string(),
        threshold: 1.0,
        score: Some(score),
        passed,
        total,
        failures,
    })
}

fn evaluate_calculator_case(case: &Value) -> Result<(), String> {
    let formula = match case["formula"].as_str() {
        Some("iiw") => CarbonEquivalentFormula::Iiw,
        Some("pcm") => CarbonEquivalentFormula::Pcm,
        other => return Err(format!("unknown formula {other:?}")),
    };
    let unit = match case["unit"].as_str() {
        Some("percent_mass") => CompositionUnit::PercentMass,
        Some("mass_fraction") => CompositionUnit::MassFraction,
        other => return Err(format!("unknown unit {other:?}")),
    };
    let composition = case["composition"]
        .as_object()
        .ok_or_else(|| "composition must be an object".to_string())?;
    let mut values = BTreeMap::new();
    for (element, value) in composition {
        values.insert(
            element.clone(),
            value
                .as_f64()
                .ok_or_else(|| format!("composition {element} is not a number"))?,
        );
    }
    let expected = case["expected"]
        .as_f64()
        .ok_or_else(|| "expected value is required".to_string())?;
    let result = calculate_carbon_equivalent(&CompositionInput { values, unit }, formula)
        .map_err(|error| error.to_string())?;
    let expected_version = case["formula_version"]
        .as_str()
        .ok_or_else(|| "formula_version is required".to_string())?;
    if result.formula_id != expected_version {
        return Err(format!(
            "formula version {} does not match {}",
            result.formula_id, expected_version
        ));
    }
    if (result.value - expected).abs() > VALUE_TOLERANCE {
        return Err(format!(
            "value {} does not match expected {expected}",
            result.value
        ));
    }
    Ok(())
}

fn read_package_asset(package_root: &Path, relative: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(package_root.join(relative))
        .map_err(|error| format!("{relative}: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("{relative}: {error}"))
}

fn evaluate_mapping_case(case: &Value, package_root: &Path) -> Result<(), String> {
    let mappings = read_package_asset(package_root, "assets/data-mappings.json")?;
    let dataset_id = case["dataset"]
        .as_str()
        .ok_or_else(|| "dataset is required".to_string())?;
    let field = case["field"]
        .as_str()
        .ok_or_else(|| "field is required".to_string())?;
    let datasets = mappings["datasets"]
        .as_array()
        .ok_or_else(|| "data mappings asset is invalid".to_string())?;
    let dataset = datasets
        .iter()
        .find(|entry| entry["id"] == dataset_id)
        .ok_or_else(|| format!("dataset {dataset_id} is missing from presets"))?;
    let entry = &dataset["fields"][field];
    if entry.is_null() {
        return Err(format!("field {field} is missing from preset {dataset_id}"));
    }
    let expected_canonical = case["expected_canonical"]
        .as_str()
        .ok_or_else(|| "expected_canonical is required".to_string())?;
    if entry["canonical"] != expected_canonical {
        return Err(format!(
            "canonical {:?} does not match {expected_canonical}",
            entry["canonical"]
        ));
    }
    if let Some(expected_type) = case["expected_type"].as_str() {
        if entry["type"] != expected_type {
            return Err(format!(
                "type {:?} does not match {expected_type}",
                entry["type"]
            ));
        }
    }
    if let Some(expected_unit) = case["expected_unit"].as_str() {
        if entry["unit"] != expected_unit {
            return Err(format!(
                "unit {:?} does not match {expected_unit}",
                entry["unit"]
            ));
        }
    }
    if let Some(expected_required) = case["expected_required"].as_bool() {
        if entry["required"] != expected_required {
            return Err(format!(
                "required flag {:?} does not match {expected_required}",
                entry["required"]
            ));
        }
    }
    Ok(())
}

fn evaluate_profiling_case(case: &Value) -> Result<(), String> {
    let csv = case["csv"]
        .as_str()
        .ok_or_else(|| "csv content is required".to_string())?;
    let path = std::env::temp_dir().join(format!(
        "bloomery-eval-profile-{}.csv",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&path, csv).map_err(|error| error.to_string())?;
    let table = super::read_dataset_table(&super::DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: None,
    });
    let _ = std::fs::remove_file(&path);
    let table = table?;
    let expected_headers = case["expected_headers"]
        .as_array()
        .ok_or_else(|| "expected_headers is required".to_string())?;
    let expected: Vec<String> = expected_headers
        .iter()
        .map(|header| {
            header
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "header must be a string".to_string())
        })
        .collect::<Result<_, _>>()?;
    if table.headers != expected {
        return Err(format!(
            "headers {:?} do not match expected {expected:?}",
            table.headers
        ));
    }
    let expected_rows = case["expected_row_count"]
        .as_u64()
        .ok_or_else(|| "expected_row_count is required".to_string())?;
    if table.rows.len() as u64 != expected_rows {
        return Err(format!(
            "row count {} does not match expected {expected_rows}",
            table.rows.len()
        ));
    }
    Ok(())
}

fn evaluate_terminology_case(case: &Value, package_root: &Path) -> Result<(), String> {
    let terminology = read_package_asset(package_root, "assets/terminology.json")?;
    let term_id = case["term"]
        .as_str()
        .ok_or_else(|| "term is required".to_string())?;
    let terms = terminology["terms"]
        .as_array()
        .ok_or_else(|| "terminology asset is invalid".to_string())?;
    let term = terms
        .iter()
        .find(|entry| entry["id"] == term_id)
        .ok_or_else(|| format!("term {term_id} is missing"))?;
    let expected_category = case["expected_category"]
        .as_str()
        .ok_or_else(|| "expected_category is required".to_string())?;
    if term["category"] != expected_category {
        return Err(format!(
            "category {:?} does not match {expected_category}",
            term["category"]
        ));
    }
    if let Some(expected_stage) = case["expected_stage"].as_str() {
        if term["stage"] != expected_stage {
            return Err(format!(
                "stage {:?} does not match {expected_stage}",
                term["stage"]
            ));
        }
    }
    if let Some(expected_unit) = case["expected_unit"].as_str() {
        let units = term["units"]
            .as_array()
            .ok_or_else(|| "term has no units".to_string())?;
        if !units.iter().any(|unit| unit == expected_unit) {
            return Err(format!("units {units:?} do not include {expected_unit}"));
        }
    }
    Ok(())
}
