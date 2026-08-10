import json
from pathlib import Path

import pytest

from bloomery_worker.optimization import optimize_constrained
from bloomery_worker.training import predict_linear_regression, train_linear_regression

SUITE_PATH = (
    Path(__file__).resolve().parents[2]
    / "domain-packs"
    / "steel"
    / "evaluations"
    / "steel-evaluations-v1.json"
)


@pytest.fixture(scope="module")
def suite() -> dict:
    with SUITE_PATH.open("r", encoding="utf-8") as stream:
        loaded = json.load(stream)
    assert loaded["schema_version"] == "1.0.0"
    return loaded


def threshold(suite: dict, category: str) -> float:
    return float(suite["categories"][category]["threshold"])


def score(passed: int, total: int) -> float:
    return passed / total if total else 0.0


def test_worker_inference_cases_match_reference_vectors(suite: dict) -> None:
    cases = suite["cases"]["inference"]
    passed = 0
    failures = []
    for case in cases:
        prediction = predict_linear_regression(case["artifact"], case["features"])
        expected = case["expected_predictions"]
        if len(prediction["predictions"]) == len(expected) and all(
            abs(actual - want) <= 1e-9
            for actual, want in zip(prediction["predictions"], expected)
        ):
            passed += 1
        else:
            failures.append(f"{case['id']}: {prediction['predictions']} != {expected}")
    assert score(passed, len(cases)) >= threshold(suite, "inference"), failures


def test_worker_training_is_reproducible_and_matches_expected_metrics(suite: dict) -> None:
    cases = suite["cases"]["training_reproducibility"]
    passed = 0
    failures = []
    for case in cases:
        first = train_linear_regression(case["payload"])
        second = train_linear_regression(case["payload"])
        if first != second:
            failures.append(f"{case['id']}: artifacts differ between identical runs")
            continue
        train_mae = first["metrics"]["train"]["mae"]
        validation_mae = first["metrics"]["validation"]["mae"]
        if (
            abs(train_mae - case["expected_train_mae"]) <= 1e-9
            and abs(validation_mae - case["expected_validation_mae"]) <= 1e-9
        ):
            passed += 1
        else:
            failures.append(
                f"{case['id']}: mae train={train_mae} validation={validation_mae}"
            )
    assert score(passed, len(cases)) >= threshold(suite, "training_reproducibility"), failures


def test_worker_optimization_recommendations_stay_feasible(suite: dict) -> None:
    cases = suite["cases"]["optimization_feasibility"]
    passed = 0
    failures = []
    for case in cases:
        result = optimize_constrained(case["payload"])
        recommendations = result["recommendations"]
        feature = case["payload"]["objectives"][0]
        minimum = case["expected_minimum_value"]
        ok = bool(recommendations) and all(
            recommendation["feasible"] is case["expected_feasible"]
            and recommendation["values"][feature] >= minimum - 1e-9
            for recommendation in recommendations
        )
        # Deterministic seeds must reproduce the same recommendation set.
        repeated = optimize_constrained(case["payload"])
        if ok and repeated["recommendations"] == recommendations:
            passed += 1
        else:
            failures.append(f"{case['id']}: feasibility or determinism violated")
    assert score(passed, len(cases)) >= threshold(suite, "optimization_feasibility"), failures


def test_deferred_provider_categories_keep_recording_fields(suite: dict) -> None:
    for category in ["retrieval", "citation", "terminology_qa"]:
        spec = suite["categories"][category]
        assert spec["runner"] == "provider"
        assert "provider" in spec and "model" in spec and "run_at" in spec
