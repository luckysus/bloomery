use bloomery::steel::{parse_suite, run_rust_categories};
use serde_json::json;
use std::path::PathBuf;

fn steel_package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../domain-packs/steel")
}

fn official_suite() -> serde_json::Value {
    let text =
        std::fs::read_to_string(steel_package_root().join("evaluations/steel-evaluations-v1.json"))
            .expect("read official evaluation suite");
    parse_suite(&text).expect("official suite must parse")
}

#[test]
fn official_suite_declares_versioned_categories_and_thresholds() {
    let suite = official_suite();
    assert_eq!(suite["evaluation_version"], "1.0.0");
    let categories = suite["categories"].as_object().expect("categories object");
    for required in [
        "calculators",
        "dataset_mapping",
        "dataset_profiling",
        "terminology",
        "inference",
        "training_reproducibility",
        "optimization_feasibility",
        "retrieval",
        "citation",
        "terminology_qa",
    ] {
        let category = &categories[required];
        assert!(
            !category.is_null(),
            "official suite must declare category {required}"
        );
        assert!(
            category["threshold"].as_f64().is_some(),
            "category {required} must declare a threshold"
        );
    }
    for provider_category in ["retrieval", "citation", "terminology_qa"] {
        let category = &categories[provider_category];
        assert_eq!(category["runner"], "provider");
        assert!(
            category.as_object().unwrap().contains_key("provider")
                && category.as_object().unwrap().contains_key("model")
                && category.as_object().unwrap().contains_key("run_at"),
            "provider category {provider_category} must reserve provider/model/run_at recording fields"
        );
    }
    for case in suite["cases"]["calculators"]
        .as_array()
        .expect("calculator cases")
    {
        assert!(
            case["formula_version"].as_str().is_some() && case["source"].as_str().is_some(),
            "calculator vector must record formula version and source"
        );
    }
}

#[test]
fn rust_evaluation_categories_meet_their_thresholds() {
    let suite = official_suite();
    let report =
        run_rust_categories(&suite, &steel_package_root()).expect("official suite must run");

    assert_eq!(report.evaluation_version, "1.0.0");
    let rust_reports: Vec<_> = report
        .categories
        .iter()
        .filter(|category| category.runner == "rust")
        .collect();
    assert_eq!(rust_reports.len(), 4, "four rust-run categories expected");
    for category in &rust_reports {
        assert!(
            category.meets_threshold(),
            "category {} scored {:?} below threshold {} with failures {:?}",
            category.category,
            category.score,
            category.threshold,
            category.failures
        );
        assert!(category.failures.is_empty());
        assert!(category.total > 0);
    }
    let deferred: Vec<_> = report
        .categories
        .iter()
        .filter(|category| category.runner != "rust")
        .collect();
    assert_eq!(
        deferred.len(),
        6,
        "six worker/provider categories stay tracked"
    );
    for category in &deferred {
        assert!(
            category.score.is_none(),
            "deferred category {} must not claim a score",
            category.category
        );
    }
}

#[test]
fn regression_is_recorded_instead_of_hidden() {
    let mut suite = official_suite();
    suite["categories"] = json!({
        "calculators": {"runner": "rust", "threshold": 1.0}
    });
    let cases = suite["cases"]["calculators"]
        .as_array()
        .expect("calculator cases");
    let mut broken = cases[0].clone();
    broken["expected"] = json!(999.0);
    suite["cases"]["calculators"] = json!([broken]);

    let report =
        run_rust_categories(&suite, &steel_package_root()).expect("broken suite still runs");
    let category = &report.categories[0];
    assert_eq!(category.passed, 0);
    assert_eq!(category.total, 1);
    assert!(!category.meets_threshold());
    assert!(
        category.failures[0].contains("does not match expected"),
        "failure detail must be recorded, got {:?}",
        category.failures
    );
}

#[test]
fn unknown_schema_version_is_rejected() {
    let error = parse_suite("{\"schema_version\":\"9.9.9\",\"categories\":{}}")
        .expect_err("unsupported schema must be rejected");
    assert_eq!(error, "unsupported evaluation schema version");
}
