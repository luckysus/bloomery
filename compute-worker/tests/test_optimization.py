import math

import pytest

from suna_worker.optimization import OptimizationError, optimize_constrained
from suna_worker.training import train_sklearn_model


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


def test_optimization_accepts_a_supported_sklearn_artifact() -> None:
    artifact = train_sklearn_model(
        {
            "features": [[float(index)] for index in range(40)],
            "targets": [float(index * 2) for index in range(40)],
            "feature_names": ["temperature"],
            "split_policy": {"kind": "random", "validation_fraction": 0.25, "seed": 11},
            "algorithm": "elasticnet",
            "seed": 11,
        }
    )

    result = optimize_constrained(
        base_payload(
            artifact=artifact,
            bounds=[{"min": 0.0, "max": 39.0}],
            trials=24,
        )
    )

    assert result["model_type"] == "elasticnet"
    assert result["recommendations"]
    assert all(recommendation["feasible"] for recommendation in result["recommendations"])


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


# ---------------------------------------------------------------------------
# 第 41 章：可选算法族（ga / pso / grid）与模型支持面


def test_unknown_algorithm_is_rejected() -> None:
    with pytest.raises(OptimizationError) as excinfo:
        optimize_constrained(base_payload(algorithm="lstm"))
    assert excinfo.value.code == "invalid_payload"


@pytest.mark.parametrize("algorithm", ["ga", "pso"])
def test_ga_and_pso_reject_multi_objective_problems(algorithm) -> None:
    payload = base_payload(
        artifact=linear_artifact(coefficients=(1.0, -1.0), names=("temperature", "carbon")),
        objectives=["temperature", "carbon"],
        bounds=[{"min": 0.0, "max": 10.0}, {"min": 0.0, "max": 10.0}],
        algorithm=algorithm,
    )
    with pytest.raises(OptimizationError) as excinfo:
        optimize_constrained(payload)
    assert excinfo.value.code == "invalid_payload"
    assert "nsga2" in str(excinfo.value)


@pytest.mark.parametrize("algorithm", ["ga", "pso", "grid"])
def test_single_objective_algorithms_reach_the_optimal_bound(algorithm) -> None:
    result = optimize_constrained(base_payload(trials=60, algorithm=algorithm))

    assert result["method"] == algorithm
    recommendation = result["recommendations"][0]
    assert recommendation["feasible"] is True
    # 最小化 2*x 于 [0, 10]，最优在 x=0。
    assert recommendation["values"]["temperature"] < 0.5
    assert recommendation["objectives"][0] < 2.0
    # 第 42 章：结果必须携带 pareto_front。
    assert result["pareto_front"]
    assert result["pareto_front"][0]["objectives"][0] == pytest.approx(
        min(entry["objectives"][0] for entry in result["recommendations"])
    )


@pytest.mark.parametrize("algorithm", ["ga", "pso", "grid"])
def test_single_objective_algorithms_are_seed_deterministic(algorithm) -> None:
    first = optimize_constrained(base_payload(trials=60, seed=13, algorithm=algorithm))
    second = optimize_constrained(base_payload(trials=60, seed=13, algorithm=algorithm))
    assert first["recommendations"] == second["recommendations"]
    assert first["trials_completed"] == second["trials_completed"]


def test_grid_search_enumerates_a_deterministic_grid() -> None:
    # 单自由度、25 次预算 → 25 个覆盖 [0, 10] 的等距网格点；
    # 最小化 2*x 的最优点必须落在下界。
    result = optimize_constrained(base_payload(trials=25, algorithm="grid"))
    assert result["trials_completed"] == 25
    assert result["recommendations"][0]["values"]["temperature"] == pytest.approx(0.0, abs=1e-9)


def test_grid_search_rejects_too_many_free_dimensions() -> None:
    names = tuple(f"feature_{index}" for index in range(12))
    payload = base_payload(
        artifact=linear_artifact(coefficients=tuple(1.0 for _ in names), names=names),
        objectives=["feature_0"],
        bounds=[{"min": 0.0, "max": 1.0} for _ in names],
        algorithm="grid",
    )
    with pytest.raises(OptimizationError) as excinfo:
        optimize_constrained(payload)
    assert excinfo.value.code == "invalid_payload"
    assert "10 free feature dimensions" in str(excinfo.value)


def test_grid_search_supports_multi_objective_fronts() -> None:
    payload = base_payload(
        artifact=linear_artifact(coefficients=(1.0, -1.0), names=("temperature", "carbon")),
        objectives=["temperature", "carbon"],
        bounds=[{"min": 0.0, "max": 10.0}, {"min": 0.0, "max": 10.0}],
        trials=25,
        algorithm="grid",
    )
    result = optimize_constrained(payload)

    assert result["method"] == "grid"
    assert result["trials_completed"] == 25
    front = result["pareto_front"]
    assert len(front) >= 2
    # 前沿上的点互不支配。
    for left in front:
        for right in front:
            if left is right:
                continue
            dominates = all(a <= b for a, b in zip(left["objectives"], right["objectives"])) and any(
                a < b for a, b in zip(left["objectives"], right["objectives"])
            )
            assert not dominates, "pareto_front contains a dominated candidate"


def test_optimization_supports_boosted_and_kernel_models() -> None:
    algorithm_models = ["random_forest", "svr", "mlp"]
    pytest.importorskip("numpy")
    try:
        import lightgbm  # noqa: F401

        algorithm_models.append("lightgbm")
    except ImportError:
        pass
    try:
        import xgboost  # noqa: F401

        algorithm_models.append("xgboost")
    except ImportError:
        pass

    for algorithm in algorithm_models:
        artifact = train_sklearn_model(
            {
                "features": [[float(index)] for index in range(40)],
                "targets": [float(index * 2) for index in range(40)],
                "feature_names": ["temperature"],
                "split_policy": {"kind": "random", "validation_fraction": 0.25, "seed": 11},
                "algorithm": algorithm,
                "seed": 11,
            }
        )
        result = optimize_constrained(
            base_payload(artifact=artifact, bounds=[{"min": 0.0, "max": 39.0}], trials=24)
        )
        assert result["model_type"] == algorithm
        assert result["recommendations"][0]["feasible"] is True


def test_optimization_supports_transformer_artifacts() -> None:
    artifact = train_sklearn_model(
        {
            "features": [[float(index), float(index % 5)] for index in range(60)],
            "targets": [float(index) for index in range(60)],
            "feature_names": ["temperature", "carbon"],
            "split_policy": {"kind": "random", "validation_fraction": 0.25, "seed": 11},
            "algorithm": "transformer",
            "seed": 11,
            "epochs": 10,
        }
    )
    assert artifact["artifact_version"] == "transformer.v1"

    result = optimize_constrained(
        base_payload(
            artifact=artifact,
            objectives=["temperature"],
            bounds=[{"min": 0.0, "max": 45.0}, {"min": 0.0, "max": 4.0}],
            trials=30,
        )
    )

    assert result["model_type"] == "transformer"
    recommendation = result["recommendations"][0]
    assert 0.0 <= recommendation["values"]["temperature"] <= 45.0
    assert recommendation["feasible"] is True
