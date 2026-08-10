from __future__ import annotations

import math
from collections.abc import Callable, Mapping, Sequence
from typing import Any


MAX_TRIALS = 500
MIN_TRIALS = 1


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
    """Run an Optuna-backed constrained search over a local model artifact.

    Every returned candidate is re-evaluated through the active model and the
    hard constraints; infeasible recommendations are rejected, never hidden.
    """

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
    _report("validated", 10)

    study, sampler_name = _run_study(
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

    candidates = _collect_candidates(study, objectives, direction, multi, fixed, constraints)
    if not candidates:
        raise OptimizationError(
            "optimization_infeasible",
            "optimization completed but no candidate satisfied the constraints",
            details={"violations": _worst_violations(study, constraints)},
        )

    recommendations = []
    for values, objectives_value in candidates:
        projected, projection_failed = _project_equalities(values, constraints, fixed)
        if projection_failed or not _within_bounds(projected, feature_names, bounds):
            continue
        recomputed = _predict(model, feature_names, projected)
        if multi:
            recomputed_objectives = [
                (recomputed if direction == "minimize" else -recomputed)
                if index == 0
                else _secondary_objective(projected, feature, direction)
                for index, feature in enumerate(objectives)
            ]
        else:
            recomputed_objectives = [recomputed]
        recheck = _re_evaluate(model, feature_names, projected, objectives, constraints)
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
            details={"violations": _worst_violations(study, constraints)},
        )
    _report("validated", 99)

    return {
        "method": sampler_name,
        "direction": direction,
        "objectives": objectives,
        "feature_names": feature_names,
        "model_id": model["model_id"],
        "model_type": model["model_type"],
        "trials_completed": len(study.trials),
        "deterministic_seed": seed,
        "recommendations": recommendations,
    }


def _model(value: Any) -> dict[str, Any]:
    if not isinstance(value, Mapping):
        raise OptimizationError("invalid_artifact", "model artifact is required")
    if value.get("artifact_version") != "linear-regression.v1":
        raise OptimizationError("invalid_artifact", "unsupported model artifact version")
    if value.get("model_type") != "linear_regression":
        raise OptimizationError("invalid_artifact", "unsupported model type")
    names = value.get("feature_names")
    preprocessing = value.get("preprocessing")
    coefficients = value.get("coefficients")
    intercept = value.get("intercept")
    if (
        not isinstance(names, list)
        or not names
        or not all(isinstance(name, str) and name.strip() for name in names)
        or not isinstance(preprocessing, Mapping)
        or not isinstance(coefficients, list)
        or not isinstance(intercept, (int, float))
        or len(coefficients) != len(names)
    ):
        raise OptimizationError("invalid_artifact", "model artifact schema is invalid")
    means = preprocessing.get("means")
    scales = preprocessing.get("scales")
    if (
        not isinstance(means, list)
        or not isinstance(scales, list)
        or len(means) != len(names)
        or len(scales) != len(names)
        or any(not isinstance(scale, (int, float)) or float(scale) <= 0 for scale in scales)
    ):
        raise OptimizationError("invalid_artifact", "model preprocessing is invalid")
    return {
        "model_id": str(value.get("model_id", "")),
        "model_type": str(value.get("model_type")),
        "feature_names": [str(name) for name in names],
        "means": [float(item) for item in means],
        "scales": [float(item) for item in scales],
        "coefficients": [float(item) for item in coefficients],
        "intercept": float(intercept),
    }


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


def _predict(model: Mapping[str, Any], feature_names: Sequence[str], values: Mapping[str, float]) -> float:
    total = model["intercept"]
    for index, name in enumerate(feature_names):
        normalized = (values[name] - model["means"][index]) / model["scales"][index]
        total += model["coefficients"][index] * normalized
    return float(total)


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


def _run_study(
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
    multi: bool,
    report: Callable[[str, int], None],
    is_cancelled: Callable[[], bool] | None,
) -> tuple[Any, str]:
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
        if is_cancelled is not None and is_cancelled():
            raise OptimizationError("optimization_cancelled", "optimization was cancelled")
        values = suggest(trial)
        prediction = _predict(model, feature_names, values)
        report("searching", min(10 + int(80 * (trial.number + 1) / trials), 90))
        if not multi:
            return prediction if direction == "minimize" else -prediction
        # The first objective follows the model prediction; each additional
        # objective follows its own feature value so the Pareto front spans
        # genuine trade-offs between model output and process setpoints.
        return [
            (prediction if direction == "minimize" else -prediction)
            if index == 0
            else (values[feature] if direction == "minimize" else -values[feature])
            for index, feature in enumerate(objectives)
        ]

    if multi:
        sampler = optuna.samplers.NSGAIISampler(seed=seed, constraints_func=constraints_func)
        sampler_name = "nsga2"
        study = optuna.create_study(
            directions=["minimize"] * len(objectives),
            sampler=sampler,
        )
    else:
        sampler = optuna.samplers.TPESampler(seed=seed, constraints_func=constraints_func)
        sampler_name = "tpe"
        study = optuna.create_study(direction="minimize", sampler=sampler)

    study.optimize(objective, n_trials=trials, catch=())
    return study, sampler_name


def _collect_candidates(
    study: Any,
    objectives: Sequence[str],
    direction: str,
    multi: bool,
    fixed: Mapping[str, float],
    constraints: Sequence[Mapping[str, Any]],
) -> list[tuple[dict[str, float], list[float]]]:
    completed: list[tuple[Any, dict[str, float]]] = []
    for trial in study.trials:
        if trial.state.name != "COMPLETE" or not trial.params:
            continue
        values = {name: float(param) for name, param in trial.params.items()}
        for name, fixed_value in fixed.items():
            values.setdefault(name, fixed_value)
        completed.append((trial, values))
    # The plan requires rejecting infeasible recommendations rather than hiding
    # violations, so feasibility is the primary filter before ranking. Trials
    # that miss equality constraints are kept for deterministic projection.
    feasible = [(trial, values) for trial, values in completed if _is_feasible(constraints, values)]
    has_equalities = any(constraint["kind"] == "equality" for constraint in constraints)
    candidates: list[tuple[dict[str, float], list[float]]] = []
    if multi:
        selected_ids = {trial.number for trial in study.best_trials}
        ranked = [(trial, values) for trial, values in feasible if trial.number in selected_ids]
        if not ranked:
            ranked = sorted(feasible, key=lambda item: sum(item[0].values))[:8]
        if has_equalities:
            seen = {trial.number for trial, _ in ranked}
            for trial, values in sorted(completed, key=lambda item: sum(item[0].values))[:8]:
                if trial.number not in seen:
                    ranked.append((trial, values))
    else:
        ranked = sorted(feasible, key=lambda item: float(item[0].value))[:8]
        if has_equalities:
            seen = {trial.number for trial, _ in ranked}
            for trial, values in sorted(completed, key=lambda item: float(item[0].value))[:8]:
                if trial.number not in seen:
                    ranked.append((trial, values))
    for trial, values in ranked:
        if multi:
            objectives_value = [float(item) for item in trial.values]
            if direction == "maximize":
                objectives_value = [-item for item in objectives_value]
        else:
            signed = float(trial.value)
            objectives_value = [signed if direction == "minimize" else -signed]
        candidates.append((values, objectives_value))
    return candidates


def _secondary_objective(values: Mapping[str, float], feature: str, direction: str) -> float:
    value = float(values[feature])
    return value if direction == "minimize" else -value


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
    objectives: Sequence[str],
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


def _worst_violations(study: Any, constraints: Sequence[Mapping[str, Any]]) -> list[dict[str, Any]]:
    violations: list[dict[str, Any]] = []
    for constraint in constraints:
        best_residual: float | None = None
        for trial in study.trials:
            if trial.state.name != "COMPLETE" or not trial.params:
                continue
            residual = _constraint_residual(constraint, {name: float(value) for name, value in trial.params.items()})
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
