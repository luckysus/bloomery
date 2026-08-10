import math

import pytest

from bloomery_worker.optimization import OptimizationError, optimize_constrained


def linear_artifact(
    coefficients=(2.0,),
    intercept=0.0,
    names=("temperature",),
) -> dict:
    count = len(coefficients)
    return {
        "artifact_version": "linear-regression.v1",
        "model_type": "linear_regression",
        "model_id": "model-opt",
        "feature_names": list(names),
        "preprocessing": {
            "means": [0.0] * count,
            "scales": [1.0] * count,
        },
        "coefficients": list(coefficients),
        "intercept": intercept,
        "applicability_range": [{"min": 0.0, "max": 10.0} for _ in range(count)],
    }


def base_payload(**overrides) -> dict:
    payload = {
        "artifact": linear_artifact(),
        "direction": "minimize",
        "objectives": ["temperature"],
        "bounds": [{"min": 0.0, "max": 10.0}],
        "trials": 24,
        "seed": 7,
    }
    payload.update(overrides)
    return payload


def test_optimization_respects_bounds_and_re_evaluates_recommendation() -> None:
    result = optimize_constrained(base_payload())

    assert result["method"] == "tpe"
    assert result["direction"] == "minimize"
    assert result["trials_completed"] > 0
    assert result["deterministic_seed"] == 7
    recommendation = result["recommendations"][0]
    value = recommendation["values"]["temperature"]
    assert 0.0 <= value <= 10.0
    assert recommendation["feasible"] is True
    # The model re-evaluation must match the analytic objective 2*x.
    assert recommendation["prediction"] == pytest.approx(recommendation["objectives"][0])
    assert recommendation["prediction"] == pytest.approx(2.0 * value)
    # Minimizing 2*x over [0, 10] lands close to the lower bound.
    assert recommendation["objectives"][0] < 2.0


def test_inequality_constraint_is_enforced_on_recommendations() -> None:
    payload = base_payload(
        trials=48,
        constraints=[
            {"kind": "inequality", "coefficients": {"temperature": 1.0}, "value": 4.0, "tolerance": 0.0}
        ],
    )
    result = optimize_constrained(payload)

    recommendation = result["recommendations"][0]
    assert recommendation["feasible"] is True
    assert recommendation["values"]["temperature"] >= 4.0 - 1e-9
    assert recommendation["objectives"][0] < 12.0


def test_equality_constraint_and_fixed_values_are_honored() -> None:
    payload = base_payload(
        artifact=linear_artifact(coefficients=(1.0, 3.0), names=("temperature", "carbon")),
        objectives=["temperature", "carbon"],
        bounds=[{"min": 0.0, "max": 10.0}, {"min": 0.0, "max": 10.0}],
        fixed_values={"carbon": 2.0},
        trials=48,
        constraints=[
            {"kind": "equality", "coefficients": {"temperature": 1.0}, "value": 3.0, "tolerance": 0.1}
        ],
    )
    result = optimize_constrained(payload)

    recommendation = result["recommendations"][0]
    assert recommendation["values"]["carbon"] == pytest.approx(2.0)
    assert recommendation["values"]["temperature"] == pytest.approx(3.0, abs=0.15)
    assert recommendation["feasible"] is True


def test_infeasible_problem_is_rejected_instead_of_hidden() -> None:
    payload = base_payload(
        trials=24,
        constraints=[
            {"kind": "equality", "coefficients": {"temperature": 1.0}, "value": 2.0},
            {"kind": "inequality", "coefficients": {"temperature": 1.0}, "value": 5.0},
        ],
    )
    with pytest.raises(OptimizationError) as excinfo:
        optimize_constrained(payload)
    assert excinfo.value.code == "optimization_infeasible"


def test_multi_objective_run_returns_a_pareto_front() -> None:
    payload = base_payload(
        artifact=linear_artifact(coefficients=(1.0, -1.0), names=("temperature", "carbon")),
        objectives=["temperature", "carbon"],
        bounds=[{"min": 0.0, "max": 10.0}, {"min": 0.0, "max": 10.0}],
        trials=160,
    )
    result = optimize_constrained(payload)

    assert result["method"] == "nsga2"
    assert len(result["recommendations"]) >= 1
    # Objective 0 is the model prediction (temp - carbon), objective 1 is the
    # carbon setpoint itself; the front must reach the analytic corner
    # (temp=0, carbon=10) giving prediction -10 and carbon 10.
    corner = [
        recommendation
        for recommendation in result["recommendations"]
        if recommendation["objectives"][0] < -8.0 and recommendation["objectives"][1] > 8.0
    ]
    assert corner, "Pareto front must approach the analytic corner (0, 10)"


def test_same_seed_is_deterministic_and_different_seed_diverges() -> None:
    first = optimize_constrained(base_payload(seed=5))
    second = optimize_constrained(base_payload(seed=5))
    assert first["recommendations"] == second["recommendations"]
    assert first["trials_completed"] == second["trials_completed"]


def test_cancellation_stops_the_search_after_the_requested_trial() -> None:
    calls = {"count": 0}

    def cancel_after_two() -> bool:
        calls["count"] += 1
        return calls["count"] > 3

    with pytest.raises(OptimizationError) as excinfo:
        optimize_constrained(base_payload(trials=200), is_cancelled=cancel_after_two)
    assert excinfo.value.code == "optimization_cancelled"


def test_progress_reports_search_and_validation_stages() -> None:
    stages: list[tuple[str, int]] = []
    optimize_constrained(base_payload(), report=lambda stage, progress: stages.append((stage, progress)))

    names = [stage for stage, _ in stages]
    assert "searching" in names
    assert names[-1] == "validated"
    values = [progress for _, progress in stages]
    assert values == sorted(values)
    assert values[-1] <= 99


@pytest.mark.parametrize(
    "overrides,code",
    [
        ({"artifact": {"artifact_version": "bogus"}}, "invalid_artifact"),
        ({"bounds": []}, "invalid_payload"),
        ({"bounds": [{"min": 5.0, "max": 1.0}]}, "invalid_bounds"),
        ({"bounds": [{"min": 0.0, "max": float("inf")}]}, "invalid_bounds"),
        ({"objectives": ["missing"]}, "invalid_objective"),
        ({"direction": "sideways"}, "invalid_payload"),
        ({"trials": 0}, "invalid_payload"),
        ({"trials": 10_000}, "invalid_payload"),
        ({"fixed_values": {"missing": 1.0}}, "invalid_fixed_value"),
        ({"constraints": [{"kind": "sideways", "coefficients": {}, "value": 1.0}]}, "invalid_constraint"),
        ({"constraints": [{"kind": "equality", "coefficients": {"missing": 1.0}, "value": 1.0}]}, "invalid_constraint"),
    ],
)
def test_invalid_payloads_are_rejected_with_typed_codes(overrides, code) -> None:
    with pytest.raises(OptimizationError) as excinfo:
        optimize_constrained(base_payload(**overrides))
    assert excinfo.value.code == code


def test_recommendation_violations_are_reported_not_hidden() -> None:
    payload = base_payload(
        trials=24,
        constraints=[
            {"kind": "inequality", "coefficients": {"temperature": 1.0}, "value": 15.0, "tolerance": 0.0}
        ],
    )
    with pytest.raises(OptimizationError) as excinfo:
        optimize_constrained(payload)
    assert excinfo.value.code == "optimization_infeasible"
    assert excinfo.value.details is not None
    assert excinfo.value.details["violations"][0]["kind"] == "inequality"


def test_recommendation_values_are_finite() -> None:
    result = optimize_constrained(base_payload())
    for recommendation in result["recommendations"]:
        for value in recommendation["values"].values():
            assert math.isfinite(value)
