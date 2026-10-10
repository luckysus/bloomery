"""受约束的工艺优化引擎（第 38–43 章）。

在本地模型 artifact 上做搜索：目标 0 永远是模型预测值，附加目标取各自
特征值，从而在"性能 ↔ 工艺设定"之间形成真实的权衡面。

支持五种可选拨索算法（第 41 章）：

- ``nsga2``：多目标 NSGA-II（Optuna）；
- ``tpe``：贝叶斯优化 TPE（Optuna，单目标默认）；
- ``ga``：单目标遗传算法（纯 numpy，锦标赛 + BLX 交叉 + 高斯变异）；
- ``pso``：单目标粒子群（纯 numpy）；
- ``grid``：网格搜索（确定性枚举，单/多目标均可）。

所有算法产出的候选都会经过同一套**重新评估**：等式约束做最小范数投影、
硬约束逐条复核，不可行候选直接拒绝而非隐藏。返回值额外携带
``pareto_front``（第 42 章二维/三维可视化所用的非支配解集）。
"""

from __future__ import annotations

import itertools
import math
from collections.abc import Callable, Mapping, Sequence
from typing import Any

from .training import (
    _decode_model_pickle,
    _load_trusted_model_pickle,
    _validate_environment_lock,
)


MAX_TRIALS = 500
MIN_TRIALS = 1
# 网格枚举与前沿集合的安全上限。
MAX_GRID_POINTS = 2000
MAX_FRONT_POINTS = 200
RECOMMENDATION_LIMIT = 8

# 第 41 章算法族；ga/pso 仅支持单目标，多目标请用 nsga2。
SUPPORTED_ALGORITHMS = {"nsga2", "tpe", "ga", "pso", "grid"}
MULTI_OBJECTIVE_ALGORITHMS = {"nsga2", "grid"}


class OptimizationError(ValueError):
    def __init__(self, code: str, message: str, details: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.code = code
        self.details = details


def optimize_constrained(
    payload: Mapping[str, Any],
    report: Callable[[str, int], None] | None = None,
    is_cancelled: Callable[[], bool] | None = None,
) -> dict[str, Any]:
    """Search over a local model artifact and re-evaluate every candidate."""

    def _report(stage: str, progress: int) -> None:
        if report is not None:
            report(stage, progress)

    if not isinstance(payload, Mapping):
        raise OptimizationError("invalid_payload", "optimization payload must be an object")

    model = _model(payload.get("artifact"))
    feature_names = model["feature_names"]
    bounds = _bounds(payload.get("bounds"), len(feature_names))
    objectives = _objectives(payload.get("objectives"), feature_names)
    direction = _direction(payload.get("direction"))
    trials = _trials(payload.get("trials"))
    seed = _seed(payload.get("seed"))
    fixed = _fixed_values(payload.get("fixed_values"), feature_names, bounds)
    constraints = _constraints(payload.get("constraints"), feature_names)
    multi = len(objectives) > 1
    algorithm = _algorithm(payload.get("algorithm"), multi)
    _report("validated", 10)

    completed = _search(
        algorithm=algorithm,
        model=model,
        feature_names=feature_names,
        bounds=bounds,
        objectives=objectives,
        direction=direction,
        constraints=constraints,
        fixed=fixed,
        trials=trials,
        seed=seed,
        multi=multi,
        report=_report,
        is_cancelled=is_cancelled,
    )

    selected = _select_candidates(completed, objectives, direction, multi, constraints)
    if not selected:
        raise OptimizationError(
            "optimization_infeasible",
            "optimization completed but no candidate satisfied the constraints",
            details={"violations": _worst_violations(completed, constraints)},
        )

    recommendations: list[dict[str, Any]] = []
    for values in selected:
        projected, projection_failed = _project_equalities(values, constraints, fixed)
        if projection_failed or not _within_bounds(projected, feature_names, bounds):
            continue
        recomputed = _predict(model, feature_names, projected)
        recomputed_objectives = _objective_vector(
            projected, recomputed, objectives, direction
        )
        recheck = _re_evaluate(model, feature_names, projected, constraints)
        if not recheck["feasible"]:
            continue
        recommendations.append(
            {
                "values": projected,
                "objectives": recomputed_objectives,
                "prediction": recheck["prediction"],
                "feasible": True,
                "constraint_residuals": recheck["residuals"],
            }
        )
    if not recommendations:
        raise OptimizationError(
            "optimization_infeasible",
            "no re-evaluated candidate satisfied the hard constraints",
            details={"violations": _worst_violations(completed, constraints)},
        )
    _report("validated", 99)

    if multi:
        front = _non_dominated_indices(
            [recommendation["objectives"] for recommendation in recommendations]
        )
    else:
        front = sorted(
            range(len(recommendations)),
            key=lambda index: recommendations[index]["objectives"][0],
        )
    pareto_front = [recommendations[index] for index in front[:MAX_FRONT_POINTS]]

    return {
        "method": algorithm,
        "direction": direction,
        "objectives": objectives,
        "feature_names": feature_names,
        "model_id": model["model_id"],
        "model_type": model["model_type"],
        "trials_completed": len(completed),
        "deterministic_seed": seed,
        "recommendations": recommendations,
        "pareto_front": pareto_front,
    }


# ---------------------------------------------------------------------------
# 模型装载与预测


def _model(value: Any) -> dict[str, Any]:
    if not isinstance(value, Mapping):
        raise OptimizationError("invalid_artifact", "model artifact is required")
    artifact_version = value.get("artifact_version")
    model_type = value.get("model_type")
    names = value.get("feature_names")
    preprocessing = value.get("preprocessing")
    if (
        not isinstance(names, list)
        or not names
        or not all(isinstance(name, str) and name.strip() for name in names)
        or not isinstance(preprocessing, Mapping)
    ):
        raise OptimizationError("invalid_artifact", "model artifact schema is invalid")
    means = preprocessing.get("means")
    scales = preprocessing.get("scales")
    if (
        not isinstance(means, list)
        or not isinstance(scales, list)
        or len(means) != len(names)
        or len(scales) != len(names)
        or any(
            not isinstance(mean, (int, float))
            or not math.isfinite(float(mean))
            for mean in means
        )
        or any(
            not isinstance(scale, (int, float))
            or not math.isfinite(float(scale))
            or float(scale) <= 0
            for scale in scales
        )
    ):
        raise OptimizationError("invalid_artifact", "model preprocessing is invalid")
    model: dict[str, Any] = {
        "model_id": str(value.get("model_id", "")),
        "model_type": str(model_type),
        "artifact_version": str(artifact_version),
        "feature_names": [str(name) for name in names],
        "means": [float(item) for item in means],
        "scales": [float(item) for item in scales],
    }
    if artifact_version == "linear-regression.v1" and model_type == "linear_regression":
        coefficients = value.get("coefficients")
        intercept = value.get("intercept")
        if (
            not isinstance(coefficients, list)
            or not isinstance(intercept, (int, float))
            or not math.isfinite(float(intercept))
            or len(coefficients) != len(names)
            or any(
                not isinstance(coefficient, (int, float))
                or not math.isfinite(float(coefficient))
                for coefficient in coefficients
            )
        ):
            raise OptimizationError("invalid_artifact", "linear model artifact schema is invalid")
        model["kind"] = "linear"
        model["coefficients"] = [float(item) for item in coefficients]
        model["intercept"] = float(intercept)
        return model
    if artifact_version == "sklearn-pickle.v1" and model_type in {
        "elasticnet",
        "random_forest",
        "hist_gradient_boosting",
        "lightgbm",
        "xgboost",
        "svr",
        "mlp",
    }:
        blob = value.get("model_pickle_base64")
        if not isinstance(blob, str) or not blob:
            raise OptimizationError("invalid_artifact", "sklearn model artifact is missing its model blob")
        try:
            _validate_environment_lock(value)
            estimator = _load_trusted_model_pickle(_decode_model_pickle(blob))
        except Exception as error:
            raise OptimizationError("invalid_artifact", "sklearn model artifact could not be loaded") from error
        if not callable(getattr(estimator, "predict", None)):
            raise OptimizationError("invalid_artifact", "sklearn model artifact has no predictor")
        model["kind"] = "sklearn"
        model["estimator"] = estimator
        return model
    if artifact_version == "transformer.v1" and model_type == "transformer":
        try:
            import numpy as np

            from .transformer import _decode_weights, _forward
        except ImportError as error:
            raise OptimizationError("runtime_unavailable", "numpy is not installed") from error
        target_scaling = preprocessing.get("target_scaling")
        if not isinstance(target_scaling, Mapping):
            raise OptimizationError("invalid_artifact", "transformer artifact is missing target scaling")
        target_mean = float(target_scaling.get("mean", 0.0))
        target_scale = float(target_scaling.get("scale", 1.0))
        if not math.isfinite(target_mean) or not math.isfinite(target_scale) or target_scale == 0:
            raise OptimizationError("invalid_artifact", "transformer target scaling is invalid")
        try:
            weights = _decode_weights(value.get("weights_base64"), len(names))
        except ValueError as error:
            raise OptimizationError("invalid_artifact", str(error)) from error
        model["kind"] = "transformer"
        model["weights"] = weights
        model["target_mean"] = target_mean
        model["target_scale"] = target_scale
        model["_forward"] = _forward
        model["_np"] = np
        return model
    raise OptimizationError("invalid_artifact", "unsupported model artifact version or type")


def _predict(model: Mapping[str, Any], feature_names: Sequence[str], values: Mapping[str, float]) -> float:
    normalized = [
        (values[name] - model["means"][index]) / model["scales"][index]
        for index, name in enumerate(feature_names)
    ]
    kind = model["kind"]
    if kind == "linear":
        total = model["intercept"]
        for coefficient, value in zip(model["coefficients"], normalized):
            total += coefficient * value
        return float(total)
    if kind == "transformer":
        matrix = model["_np"].asarray([normalized], dtype=model["_np"].float64)
        logits, _ = model["_forward"](matrix, model["weights"])
        return float(logits[0]) * model["target_scale"] + model["target_mean"]
    try:
        import numpy as np

        prediction = model["estimator"].predict(np.asarray([normalized], dtype=np.float64))
        return float(prediction[0])
    except Exception as error:
        raise OptimizationError("invalid_artifact", "model prediction failed") from error


# ---------------------------------------------------------------------------
# 输入解析


def _algorithm(value: Any, multi: bool) -> str:
    if value is None:
        return "nsga2" if multi else "tpe"
    if value not in SUPPORTED_ALGORITHMS:
        raise OptimizationError(
            "invalid_payload",
            f"algorithm must be one of {sorted(SUPPORTED_ALGORITHMS)}",
        )
    if value in {"ga", "pso"} and multi:
        raise OptimizationError(
            "invalid_payload",
            f"{value} supports a single objective; use nsga2 for multi-objective search",
        )
    return str(value)


def _bounds(value: Any, feature_count: int) -> list[tuple[float, float]]:
    if not isinstance(value, list) or not value or len(value) != feature_count:
        raise OptimizationError("invalid_payload", "bounds must cover every feature")
    result: list[tuple[float, float]] = []
    for index, item in enumerate(value):
        if not isinstance(item, Mapping):
            raise OptimizationError("invalid_bounds", f"bounds[{index}] must be an object")
        minimum = _finite(item.get("min"), f"bounds[{index}].min")
        maximum = _finite(item.get("max"), f"bounds[{index}].max")
        if minimum > maximum:
            raise OptimizationError("invalid_bounds", f"bounds[{index}] is inverted")
        result.append((minimum, maximum))
    return result


def _objectives(value: Any, feature_names: Sequence[str]) -> list[str]:
    if not isinstance(value, list) or not value or len(value) > 4:
        raise OptimizationError("invalid_payload", "objectives must list 1-4 feature names")
    names: list[str] = []
    for item in value:
        if not isinstance(item, str) or item not in feature_names:
            raise OptimizationError("invalid_objective", f"unknown objective feature: {item!r}")
        if item in names:
            raise OptimizationError("invalid_objective", f"duplicate objective feature: {item!r}")
        names.append(item)
    return names


def _direction(value: Any) -> str:
    if value not in {"minimize", "maximize"}:
        raise OptimizationError("invalid_payload", "direction must be minimize or maximize")
    return str(value)


def _trials(value: Any) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise OptimizationError("invalid_payload", "trials must be an integer")
    if value < MIN_TRIALS or value > MAX_TRIALS:
        raise OptimizationError(
            "invalid_payload", f"trials must be between {MIN_TRIALS} and {MAX_TRIALS}"
        )
    return value


def _seed(value: Any) -> int:
    if value is None:
        return 0
    if isinstance(value, bool) or not isinstance(value, int):
        raise OptimizationError("invalid_payload", "seed must be an integer")
    return value


def _fixed_values(
    value: Any, feature_names: Sequence[str], bounds: Sequence[tuple[float, float]]
) -> dict[str, float]:
    if value is None:
        return {}
    if not isinstance(value, Mapping):
        raise OptimizationError("invalid_payload", "fixed_values must be an object")
    fixed: dict[str, float] = {}
    for name, raw in value.items():
        if not isinstance(name, str) or name not in feature_names:
            raise OptimizationError("invalid_fixed_value", f"unknown fixed feature: {name!r}")
        number = _finite(raw, f"fixed_values.{name}")
        minimum, maximum = bounds[feature_names.index(name)]
        if number < minimum or number > maximum:
            raise OptimizationError("invalid_fixed_value", f"fixed value for {name} is outside its bounds")
        fixed[name] = number
    return fixed


def _constraints(value: Any, feature_names: Sequence[str]) -> list[dict[str, Any]]:
    if value is None:
        return []
    if not isinstance(value, list) or len(value) > 64:
        raise OptimizationError("invalid_payload", "constraints must be a list of at most 64 entries")
    constraints: list[dict[str, Any]] = []
    for index, item in enumerate(value):
        if not isinstance(item, Mapping):
            raise OptimizationError("invalid_constraint", f"constraints[{index}] must be an object")
        kind = item.get("kind")
        if kind not in {"equality", "inequality"}:
            raise OptimizationError(
                "invalid_constraint", f"constraints[{index}] kind must be equality or inequality"
            )
        coefficients = item.get("coefficients")
        if not isinstance(coefficients, Mapping) or not coefficients:
            raise OptimizationError(
                "invalid_constraint", f"constraints[{index}] coefficients are required"
            )
        resolved: dict[str, float] = {}
        for name, raw in coefficients.items():
            if not isinstance(name, str) or name not in feature_names:
                raise OptimizationError(
                    "invalid_constraint", f"constraints[{index}] references unknown feature {name!r}"
                )
            resolved[name] = _finite(raw, f"constraints[{index}].coefficients.{name}")
        target = _finite(item.get("value"), f"constraints[{index}].value")
        tolerance = _finite(item.get("tolerance", 1e-6), f"constraints[{index}].tolerance")
        if tolerance < 0:
            raise OptimizationError("invalid_constraint", "constraint tolerance must not be negative")
        constraints.append(
            {"kind": str(kind), "coefficients": resolved, "value": target, "tolerance": tolerance}
        )
    return constraints


# ---------------------------------------------------------------------------
# 搜索引擎（第 41 章）
#
# 统一产物：``list[dict]``，每项 {"params": 特征取值, "signed": 最小化约定
# 下的目标向量}。目标 0 是模型预测（按 direction 取符号），附加目标是各自
# 特征值（按 direction 取符号）。


def _signed_objectives(
    values: Mapping[str, float],
    prediction: float,
    objectives: Sequence[str],
    direction: str,
) -> list[float]:
    return [
        (prediction if direction == "minimize" else -prediction)
        if index == 0
        else (float(values[feature]) if direction == "minimize" else -float(values[feature]))
        for index, feature in enumerate(objectives)
    ]


def _free_dims(
    feature_names: Sequence[str],
    bounds: Sequence[tuple[float, float]],
    fixed: Mapping[str, float],
) -> list[tuple[str, float, float]]:
    free: list[tuple[str, float, float]] = []
    for index, name in enumerate(feature_names):
        if name in fixed:
            continue
        minimum, maximum = bounds[index]
        if maximum <= minimum:
            continue
        free.append((name, minimum, maximum))
    return free


def _compose_values(
    free: Sequence[tuple[str, float, float]],
    genome: Sequence[float],
    feature_names: Sequence[str],
    bounds: Sequence[tuple[float, float]],
    fixed: Mapping[str, float],
) -> dict[str, float]:
    values: dict[str, float] = {}
    for index, name in enumerate(feature_names):
        if name in fixed:
            values[name] = fixed[name]
            continue
        minimum, maximum = bounds[index]
        values[name] = minimum if maximum <= minimum else minimum
    for (name, minimum, _maximum), value in zip(free, genome):
        values[name] = float(value)
    return values


def _search(
    *,
    algorithm: str,
    model: Mapping[str, Any],
    feature_names: Sequence[str],
    bounds: Sequence[tuple[float, float]],
    objectives: Sequence[str],
    direction: str,
    constraints: Sequence[Mapping[str, Any]],
    fixed: Mapping[str, float],
    trials: int,
    seed: int,
    multi: bool,
    report: Callable[[str, int], None],
    is_cancelled: Callable[[], bool] | None,
) -> list[dict[str, Any]]:
    if algorithm in {"tpe", "nsga2"}:
        return _run_optuna(
            algorithm=algorithm,
            model=model,
            feature_names=feature_names,
            bounds=bounds,
            objectives=objectives,
            direction=direction,
            constraints=constraints,
            fixed=fixed,
            trials=trials,
            seed=seed,
            multi=multi,
            report=report,
            is_cancelled=is_cancelled,
        )
    if algorithm == "grid":
        return _run_grid(
            model=model,
            feature_names=feature_names,
            bounds=bounds,
            objectives=objectives,
            direction=direction,
            constraints=constraints,
            fixed=fixed,
            trials=trials,
            report=report,
            is_cancelled=is_cancelled,
        )
    if algorithm == "ga":
        return _run_evolutionary(
            model=model,
            feature_names=feature_names,
            bounds=bounds,
            objectives=objectives,
            direction=direction,
            constraints=constraints,
            fixed=fixed,
            trials=trials,
            seed=seed,
            report=report,
            is_cancelled=is_cancelled,
        )
    return _run_pso(
        model=model,
        feature_names=feature_names,
        bounds=bounds,
        objectives=objectives,
        direction=direction,
        constraints=constraints,
        fixed=fixed,
        trials=trials,
        seed=seed,
        report=report,
        is_cancelled=is_cancelled,
    )


def _check_cancel(is_cancelled: Callable[[], bool] | None) -> None:
    if is_cancelled is not None and is_cancelled():
        raise OptimizationError("optimization_cancelled", "optimization was cancelled")


def _run_optuna(
    *,
    algorithm: str,
    model: Mapping[str, Any],
    feature_names: Sequence[str],
    bounds: Sequence[tuple[float, float]],
    objectives: Sequence[str],
    direction: str,
    constraints: Sequence[Mapping[str, Any]],
    fixed: Mapping[str, float],
    trials: int,
    seed: int,
    multi: bool,
    report: Callable[[str, int], None],
    is_cancelled: Callable[[], bool] | None,
) -> list[dict[str, Any]]:
    try:
        import optuna
    except ImportError as error:
        raise OptimizationError("runtime_unavailable", "optuna is not installed") from error

    optuna.logging.set_verbosity(optuna.logging.WARNING)

    def suggest(trial: Any) -> dict[str, float]:
        values: dict[str, float] = {}
        for index, name in enumerate(feature_names):
            if name in fixed:
                values[name] = fixed[name]
            else:
                minimum, maximum = bounds[index]
                if minimum == maximum:
                    values[name] = minimum
                else:
                    values[name] = trial.suggest_float(name, minimum, maximum)
        return values

    def constraints_func(trial: Any) -> list[float]:
        values = {name: float(trial.params[name]) for name in feature_names if name in trial.params}
        for name, fixed_value in fixed.items():
            values.setdefault(name, fixed_value)
        return [_constraint_violation(constraint, values) for constraint in constraints]

    def objective(trial: Any) -> Any:
        _check_cancel(is_cancelled)
        values = suggest(trial)
        prediction = _predict(model, feature_names, values)
        report("searching", min(10 + int(80 * (trial.number + 1) / trials), 90))
        return _signed_objectives(values, prediction, objectives, direction)

    if multi:
        sampler = optuna.samplers.NSGAIISampler(seed=seed, constraints_func=constraints_func)
        study = optuna.create_study(directions=["minimize"] * len(objectives), sampler=sampler)
    else:
        sampler = optuna.samplers.TPESampler(seed=seed, constraints_func=constraints_func)
        study = optuna.create_study(direction="minimize", sampler=sampler)
    study.optimize(objective, n_trials=trials, catch=())

    completed: list[dict[str, Any]] = []
    for trial in study.trials:
        if trial.state.name != "COMPLETE" or not trial.params:
            continue
        values = {name: float(param) for name, param in trial.params.items()}
        for name, fixed_value in fixed.items():
            values.setdefault(name, fixed_value)
        prediction = _predict(model, feature_names, values)
        completed.append(
            {
                "params": values,
                "signed": _signed_objectives(values, prediction, objectives, direction),
            }
        )
    return completed


def _run_grid(
    *,
    model: Mapping[str, Any],
    feature_names: Sequence[str],
    bounds: Sequence[tuple[float, float]],
    objectives: Sequence[str],
    direction: str,
    constraints: Sequence[Mapping[str, Any]],
    fixed: Mapping[str, float],
    trials: int,
    report: Callable[[str, int], None],
    is_cancelled: Callable[[], bool] | None,
) -> list[dict[str, Any]]:
    free = _free_dims(feature_names, bounds, fixed)
    if not free:
        return [_evaluate_single_point(model, feature_names, bounds, fixed, objectives, direction)]
    if len(free) > 10:
        raise OptimizationError(
            "invalid_payload",
            "grid_search supports at most 10 free feature dimensions; use nsga2 or tpe instead",
        )
    points_per_dim = max(2, int(trials ** (1 / len(free))))
    while points_per_dim ** len(free) > MAX_GRID_POINTS and points_per_dim > 2:
        points_per_dim -= 1
    if points_per_dim ** len(free) > MAX_GRID_POINTS:
        raise OptimizationError(
            "invalid_payload",
            "grid_search would exceed the evaluation budget; reduce free feature dimensions or use nsga2",
        )
    axes = [
        [minimum + (maximum - minimum) * step / (points_per_dim - 1) for step in range(points_per_dim)]
        for _name, minimum, maximum in free
    ]
    completed: list[dict[str, Any]] = []
    total = points_per_dim ** len(free)
    done = 0
    for combination in itertools.product(*axes):
        _check_cancel(is_cancelled)
        values = _compose_values(free, combination, feature_names, bounds, fixed)
        prediction = _predict(model, feature_names, values)
        completed.append(
            {
                "params": values,
                "signed": _signed_objectives(values, prediction, objectives, direction),
            }
        )
        done += 1
        report("searching", min(10 + int(80 * done / total), 90))
    return completed


def _run_evolutionary(
    *,
    model: Mapping[str, Any],
    feature_names: Sequence[str],
    bounds: Sequence[tuple[float, float]],
    objectives: Sequence[str],
    direction: str,
    constraints: Sequence[Mapping[str, Any]],
    fixed: Mapping[str, float],
    trials: int,
    seed: int,
    report: Callable[[str, int], None],
    is_cancelled: Callable[[], bool] | None,
) -> list[dict[str, Any]]:
    import numpy as np

    free = _free_dims(feature_names, bounds, fixed)
    if not free:
        return [_evaluate_single_point(model, feature_names, bounds, fixed, objectives, direction)]
    population_size = max(8, min(40, trials // 3))
    generations = max(1, trials // population_size)
    rng = np.random.default_rng(seed)
    lows = np.asarray([minimum for _name, minimum, _maximum in free], dtype=np.float64)
    highs = np.asarray([maximum for _name, _minimum, maximum in free], dtype=np.float64)
    spans = highs - lows
    width = len(free)

    def evaluate(genome: "np.ndarray") -> tuple[float, dict[str, float], list[float]]:
        values = _compose_values(free, genome, feature_names, bounds, fixed)
        prediction = _predict(model, feature_names, values)
        signed = _signed_objectives(values, prediction, objectives, direction)
        violation = sum(_constraint_violation(constraint, values) for constraint in constraints)
        # 返回原始目标（最小化约定）；违约惩罚在需要时单独叠加。
        return float(signed[0]), values, signed

    population = rng.uniform(lows, highs, size=(population_size, width))
    scored = [evaluate(population[index]) for index in range(population_size)]
    penalty = 10.0 * (1.0 + max(abs(entry[2][0]) for entry in scored))

    def penalize(values: Mapping[str, float], raw: float) -> float:
        violation = sum(_constraint_violation(constraint, values) for constraint in constraints)
        return raw + penalty * violation

    completed: list[dict[str, Any]] = [
        {"params": values, "signed": signed} for _fitness, values, signed in scored
    ]

    evaluated = population_size
    for generation in range(generations):
        _check_cancel(is_cancelled)
        order = sorted(range(population_size), key=lambda index: penalize(scored[index][1], scored[index][0]))
        elite = population[order[0]].copy()
        parent_pool = [population[index] for index in order]
        offspring: list["np.ndarray"] = [elite]
        while len(offspring) < population_size:
            first = parent_pool[_tournament(rng, population_size)]
            second = parent_pool[_tournament(rng, population_size)]
            alpha = 0.3
            low = np.minimum(first, second) - alpha * np.abs(first - second)
            high = np.maximum(first, second) + alpha * np.abs(first - second)
            child = rng.uniform(low, high)
            mutation = rng.random(width) < (1.0 / width)
            if mutation.any():
                child = np.where(
                    mutation,
                    child + rng.normal(0.0, 0.05, size=width) * spans,
                    child,
                )
            offspring.append(np.clip(child, lows, highs))
        population = np.asarray(offspring)
        scored = [evaluate(population[index]) for index in range(population_size)]
        evaluated += population_size
        for _fitness, values, signed in scored:
            completed.append({"params": values, "signed": signed})
        report("searching", min(10 + int(80 * evaluated / max(1, population_size * generations)), 90))
    return completed


def _tournament(rng: "Any", population_size: int, size: int = 3) -> int:
    return int(min(rng.integers(0, population_size, size=size)))


def _run_pso(
    *,
    model: Mapping[str, Any],
    feature_names: Sequence[str],
    bounds: Sequence[tuple[float, float]],
    objectives: Sequence[str],
    direction: str,
    constraints: Sequence[Mapping[str, Any]],
    fixed: Mapping[str, float],
    trials: int,
    seed: int,
    report: Callable[[str, int], None],
    is_cancelled: Callable[[], bool] | None,
) -> list[dict[str, Any]]:
    import numpy as np

    free = _free_dims(feature_names, bounds, fixed)
    if not free:
        return [_evaluate_single_point(model, feature_names, bounds, fixed, objectives, direction)]
    swarm_size = max(8, min(40, trials // 4))
    iterations = max(1, trials // swarm_size)
    rng = np.random.default_rng(seed)
    lows = np.asarray([minimum for _name, minimum, _maximum in free], dtype=np.float64)
    highs = np.asarray([maximum for _name, _minimum, maximum in free], dtype=np.float64)
    spans = highs - lows
    width = len(free)

    def evaluate(positions: "np.ndarray") -> tuple[list[float], list[dict[str, float]], list[list[float]]]:
        fitness: list[float] = []
        value_rows: list[dict[str, float]] = []
        signed_rows: list[list[float]] = []
        for index in range(len(positions)):
            values = _compose_values(free, positions[index], feature_names, bounds, fixed)
            prediction = _predict(model, feature_names, values)
            signed = _signed_objectives(values, prediction, objectives, direction)
            fitness.append(float(signed[0]))
            value_rows.append(values)
            signed_rows.append(signed)
        return fitness, value_rows, signed_rows

    positions = rng.uniform(lows, highs, size=(swarm_size, width))
    velocities = rng.uniform(-0.1, 0.1, size=(swarm_size, width)) * spans
    fitness, value_rows, signed_rows = evaluate(positions)
    penalty = 10.0 * (1.0 + max(abs(signed[0]) for signed in signed_rows))

    def penalize(values: Mapping[str, float], raw: float) -> float:
        violation = sum(_constraint_violation(constraint, values) for constraint in constraints)
        return raw + penalty * violation

    completed: list[dict[str, Any]] = [
        {"params": values, "signed": signed} for values, signed in zip(value_rows, signed_rows)
    ]
    personal_best = positions.copy()
    personal_fitness = [
        penalize(value_rows[index], fitness[index]) for index in range(swarm_size)
    ]
    evaluated = swarm_size

    for iteration in range(iterations):
        _check_cancel(is_cancelled)
        best_index = int(np.argmin(personal_fitness))
        inertia, cognitive, social = 0.7, 1.4, 1.4
        r1 = rng.random((swarm_size, width))
        r2 = rng.random((swarm_size, width))
        velocities = (
            inertia * velocities
            + cognitive * r1 * (personal_best - positions)
            + social * r2 * (personal_best[best_index] - positions)
        )
        positions = np.clip(positions + velocities, lows, highs)
        fitness, new_values, new_signed = evaluate(positions)
        evaluated += swarm_size
        for values, signed in zip(new_values, new_signed):
            completed.append({"params": values, "signed": signed})
        for index in range(swarm_size):
            candidate = penalize(new_values[index], fitness[index])
            if candidate < personal_fitness[index]:
                personal_fitness[index] = candidate
                personal_best[index] = positions[index].copy()
        report("searching", min(10 + int(80 * evaluated / max(1, swarm_size * iterations)), 90))
    return completed


def _evaluate_single_point(
    model: Mapping[str, Any],
    feature_names: Sequence[str],
    bounds: Sequence[tuple[float, float]],
    fixed: Mapping[str, float],
    objectives: Sequence[str],
    direction: str,
) -> dict[str, Any]:
    values = _compose_values([], [], feature_names, bounds, fixed)
    prediction = _predict(model, feature_names, values)
    return {
        "params": values,
        "signed": _signed_objectives(values, prediction, objectives, direction),
    }


# ---------------------------------------------------------------------------
# 候选筛选与复核


def _select_candidates(
    completed: list[dict[str, Any]],
    objectives: Sequence[str],
    direction: str,
    multi: bool,
    constraints: Sequence[Mapping[str, Any]],
) -> list[dict[str, float]]:
    feasible = [entry for entry in completed if _is_feasible(constraints, entry["params"])]
    has_equalities = any(constraint["kind"] == "equality" for constraint in constraints)
    if multi:
        front = _non_dominated_indices([entry["signed"] for entry in feasible])
        ranked = [feasible[index]["params"] for index in front[: RECOMMENDATION_LIMIT * 2]]
        if not ranked:
            ranked = [entry["params"] for entry in sorted(feasible, key=lambda entry: sum(entry["signed"]))[:RECOMMENDATION_LIMIT]]
        if has_equalities:
            seen = {id(values) for values in ranked}
            for entry in sorted(completed, key=lambda entry: sum(entry["signed"]))[:RECOMMENDATION_LIMIT]:
                if id(entry["params"]) not in seen:
                    ranked.append(entry["params"])
    else:
        ranked = [entry["params"] for entry in sorted(feasible, key=lambda entry: entry["signed"][0])[:RECOMMENDATION_LIMIT]]
        if has_equalities:
            seen = {id(values) for values in ranked}
            for entry in sorted(completed, key=lambda entry: entry["signed"][0])[:RECOMMENDATION_LIMIT]:
                if id(entry["params"]) not in seen:
                    ranked.append(entry["params"])
    return ranked


def _non_dominated_indices(signed_rows: Sequence[Sequence[float]]) -> list[int]:
    """最小化约定下的非支配解下标（跳过重复点，保持首次出现顺序）。"""
    front: list[int] = []
    for index, row in enumerate(signed_rows):
        dominated = False
        for other, candidate in enumerate(signed_rows):
            if other == index:
                continue
            if all(candidate[k] <= row[k] for k in range(len(row))) and any(
                candidate[k] < row[k] for k in range(len(row))
            ):
                dominated = True
                break
            if other in front and all(candidate[k] == row[k] for k in range(len(row))):
                dominated = True
                break
        if not dominated:
            front.append(index)
    return front


def _objective_vector(
    values: Mapping[str, float],
    prediction: float,
    objectives: Sequence[str],
    direction: str,
) -> list[float]:
    """把最小化约定的 signed 向量还原为用户视角的目标值。"""
    signed = _signed_objectives(values, prediction, objectives, direction)
    return signed if direction == "minimize" else [-item for item in signed]


def _constraint_residual(constraint: Mapping[str, Any], values: Mapping[str, float]) -> float:
    expression = sum(coefficient * values[name] for name, coefficient in constraint["coefficients"].items())
    if constraint["kind"] == "equality":
        return expression - constraint["value"]
    # Inequality means expression >= value; a negative residual is a violation.
    return expression - constraint["value"]


def _is_feasible(constraints: Sequence[Mapping[str, Any]], values: Mapping[str, float]) -> bool:
    for constraint in constraints:
        residual = _constraint_residual(constraint, values)
        tolerance = constraint["tolerance"]
        if constraint["kind"] == "equality" and abs(residual) > tolerance:
            return False
        if constraint["kind"] == "inequality" and residual < -tolerance:
            return False
    return True


def _constraint_violation(constraint: Mapping[str, Any], values: Mapping[str, float]) -> float:
    """Return a non-negative violation magnitude; 0 means satisfied."""
    residual = _constraint_residual(constraint, values)
    if constraint["kind"] == "equality":
        return max(0.0, abs(residual) - constraint["tolerance"])
    return max(0.0, -residual - constraint["tolerance"])


def _within_bounds(
    values: Mapping[str, float], feature_names: Sequence[str], bounds: Sequence[tuple[float, float]]
) -> bool:
    for index, name in enumerate(feature_names):
        minimum, maximum = bounds[index]
        number = values.get(name)
        if number is None or number < minimum - 1e-12 or number > maximum + 1e-12:
            return False
    return True


def _project_equalities(
    values: Mapping[str, float],
    constraints: Sequence[Mapping[str, Any]],
    fixed: Mapping[str, float],
) -> tuple[dict[str, float], bool]:
    """Deterministically project a candidate onto the equality constraints.

    The projection is the minimum-norm solution of the linear equality system,
    so a candidate near the feasible surface becomes exactly feasible. Fixed
    dimensions are never moved.
    """
    equalities = [constraint for constraint in constraints if constraint["kind"] == "equality"]
    projected = {name: float(value) for name, value in values.items()}
    if not equalities:
        return projected, False
    names = sorted(projected)
    width = len(names)
    rows: list[list[float]] = []
    targets: list[float] = []
    for constraint in equalities:
        row = [float(constraint["coefficients"].get(name, 0.0)) for name in names]
        rows.append(row)
        current = sum(coefficient * projected[name] for name, coefficient in constraint["coefficients"].items())
        targets.append(constraint["value"] - current)

    # Normal equations A A^T delta = targets give the minimum-norm step A^T delta.
    count = len(rows)
    matrix = [[sum(rows[left][k] * rows[right][k] for k in range(width)) for right in range(count)] for left in range(count)]
    solution = _solve(matrix, targets)
    if solution is None:
        return projected, True
    for index, name in enumerate(names):
        if name in fixed:
            continue
        step = sum(rows[equation][index] * solution[equation] for equation in range(count))
        projected[name] = projected[name] + step
    return projected, False


def _solve(matrix: list[list[float]], vector: list[float]) -> list[float] | None:
    size = len(vector)
    augmented = [row[:] + [vector[index]] for index, row in enumerate(matrix)]
    for column in range(size):
        pivot = max(range(column, size), key=lambda row: abs(augmented[row][column]))
        if abs(augmented[pivot][column]) <= 1e-12:
            return None
        augmented[column], augmented[pivot] = augmented[pivot], augmented[column]
        divisor = augmented[column][column]
        augmented[column] = [value / divisor for value in augmented[column]]
        for row in range(size):
            if row == column:
                continue
            factor = augmented[row][column]
            if factor == 0:
                continue
            augmented[row] = [left - factor * right for left, right in zip(augmented[row], augmented[column])]
    return [augmented[index][-1] for index in range(size)]


def _re_evaluate(
    model: Mapping[str, Any],
    feature_names: Sequence[str],
    values: Mapping[str, float],
    constraints: Sequence[Mapping[str, Any]],
) -> dict[str, Any]:
    prediction = _predict(model, feature_names, values)
    residuals = {
        f"{constraint['kind']}:{'+'.join(sorted(constraint['coefficients']))}": _constraint_residual(
            constraint, values
        )
        for constraint in constraints
    }
    return {
        "prediction": prediction,
        "feasible": _is_feasible(constraints, values),
        "residuals": residuals,
    }


def _worst_violations(completed: list[dict[str, Any]], constraints: Sequence[Mapping[str, Any]]) -> list[dict[str, Any]]:
    violations: list[dict[str, Any]] = []
    for constraint in constraints:
        best_residual: float | None = None
        for entry in completed:
            residual = _constraint_residual(constraint, entry["params"])
            magnitude = abs(residual) if constraint["kind"] == "equality" else -residual
            if best_residual is None or magnitude < best_residual:
                best_residual = magnitude
        violations.append(
            {
                "kind": constraint["kind"],
                "value": constraint["value"],
                "tolerance": constraint["tolerance"],
                "best_residual": best_residual,
            }
        )
    return violations


def _finite(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise OptimizationError("invalid_payload", f"{label} must be a number")
    number = float(value)
    if not math.isfinite(number):
        raise OptimizationError("invalid_bounds" if label.startswith("bounds") else "invalid_payload", f"{label} must be finite")
    return number
