from __future__ import annotations

import hashlib
import json
import math
import random
from collections.abc import Mapping, Sequence
from typing import Any


MAX_ROWS = 100_000
MAX_FEATURES = 128
ARTIFACT_VERSION = "linear-regression.v1"


def train_linear_regression(payload: Mapping[str, Any]) -> dict[str, Any]:
    """Fit a deterministic, standard-library linear regression artifact."""
    features, targets, feature_names = _validate_dataset(payload)
    split = _split_indices(len(features), payload)
    means, scales, transformed = _fit_preprocessing(features, split["train_indices"])
    coefficients, intercept = _fit_ordinary_least_squares(
        transformed,
        targets,
        split["train_indices"],
        _ridge_value(payload),
    )

    artifact: dict[str, Any] = {
        "artifact_version": ARTIFACT_VERSION,
        "model_type": "linear_regression",
        "data_version": str(payload.get("data_version", "unknown")),
        "feature_names": feature_names,
        "feature_schema": {"count": len(feature_names), "names": feature_names},
        "field_mapping": _mapping(payload.get("field_mapping", {})),
        "preprocessing": {
            "fit_scope": "train_only",
            "imputation": "train_mean",
            "means": means,
            "scales": scales,
        },
        "coefficients": coefficients,
        "intercept": intercept,
        "parameters": {
            "algorithm": "linear_regression",
            "ridge": _ridge_value(payload),
        },
        "split": split,
        "metrics": {
            "train": _metrics(
                targets,
                _predict_transformed(transformed, split["train_indices"], coefficients, intercept),
                split["train_indices"],
            ),
            "validation": _metrics(
                targets,
                _predict_transformed(transformed, split["validation_indices"], coefficients, intercept),
                split["validation_indices"],
            ),
        },
        "feature_importance": [abs(value) for value in coefficients],
        "applicability_range": _applicability_range(features, split["train_indices"]),
    }
    artifact["model_id"] = _artifact_id(artifact)
    return artifact


def predict_linear_regression(
    artifact: Mapping[str, Any], features: Sequence[Sequence[Any]]
) -> dict[str, Any]:
    if artifact.get("artifact_version") != ARTIFACT_VERSION:
        raise ValueError("unsupported model artifact version")
    if artifact.get("model_type") != "linear_regression":
        raise ValueError("unsupported model type")
    names = artifact.get("feature_names")
    preprocessing = artifact.get("preprocessing")
    coefficients = artifact.get("coefficients")
    intercept = artifact.get("intercept")
    if not isinstance(names, list) or not isinstance(preprocessing, Mapping):
        raise ValueError("model artifact schema is invalid")
    if not isinstance(coefficients, list) or not isinstance(intercept, (int, float)):
        raise ValueError("model artifact parameters are invalid")
    means = preprocessing.get("means")
    scales = preprocessing.get("scales")
    if not isinstance(means, list) or not isinstance(scales, list) or len(means) != len(names) or len(scales) != len(names):
        raise ValueError("model preprocessing schema is invalid")
    rows = _normalise_features(features, len(names))
    transformed = [
        [
            ((value if value is not None else float(means[column])) - float(means[column]))
            / float(scales[column])
            for column, value in enumerate(row)
        ]
        for row in rows
    ]
    predictions = [
        float(intercept) + sum(float(coefficient) * value for coefficient, value in zip(coefficients, row))
        for row in transformed
    ]
    return {
        "model_id": str(artifact.get("model_id", "")),
        "model_type": "linear_regression",
        "predictions": predictions,
        "feature_names": names,
    }


def _validate_dataset(payload: Mapping[str, Any]) -> tuple[list[list[float | None]], list[float], list[str]]:
    algorithm = payload.get("algorithm", "linear_regression")
    if algorithm != "linear_regression":
        raise ValueError(f"unsupported training algorithm: {algorithm}")
    features = payload.get("features")
    targets = payload.get("targets")
    if not isinstance(features, Sequence) or isinstance(features, (str, bytes)) or not features:
        raise ValueError("features must be a non-empty matrix")
    if len(features) > MAX_ROWS:
        raise ValueError(f"features exceed the maximum row count of {MAX_ROWS}")
    if not isinstance(targets, Sequence) or isinstance(targets, (str, bytes)) or len(targets) != len(features):
        raise ValueError("targets must have one value for every feature row")
    first_row = features[0]
    if not isinstance(first_row, Sequence) or isinstance(first_row, (str, bytes)) or not first_row:
        raise ValueError("features must contain non-empty rows")
    feature_count = len(first_row)
    if feature_count > MAX_FEATURES:
        raise ValueError(f"features exceed the maximum column count of {MAX_FEATURES}")
    rows = _normalise_features(features, feature_count)
    numeric_targets = [_required_number(value, f"targets[{index}]") for index, value in enumerate(targets)]
    names = payload.get("feature_names")
    if names is None:
        feature_names = [f"feature_{index + 1}" for index in range(feature_count)]
    elif isinstance(names, Sequence) and not isinstance(names, (str, bytes)) and len(names) == feature_count and all(isinstance(name, str) and name.strip() for name in names):
        feature_names = [str(name) for name in names]
    else:
        raise ValueError("feature_names must match the feature column count")
    return rows, numeric_targets, feature_names


def _normalise_features(features: Sequence[Sequence[Any]], feature_count: int) -> list[list[float | None]]:
    rows: list[list[float | None]] = []
    for row_index, row in enumerate(features):
        if not isinstance(row, Sequence) or isinstance(row, (str, bytes)) or len(row) != feature_count:
            raise ValueError(f"features[{row_index}] must have exactly {feature_count} columns")
        rows.append([_optional_number(value, f"features[{row_index}][{column}]") for column, value in enumerate(row)])
    return rows


def _optional_number(value: Any, label: str) -> float | None:
    if value is None:
        return None
    return _required_number(value, label)


def _required_number(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(float(value)):
        raise ValueError(f"{label} must be a finite number")
    return float(value)


def _split_indices(row_count: int, payload: Mapping[str, Any]) -> dict[str, Any]:
    policy_value = payload.get("split_policy", {})
    if not isinstance(policy_value, Mapping):
        raise ValueError("split_policy must be an object")
    kind = policy_value.get("kind", "random")
    if kind not in {"random", "group", "time"}:
        raise ValueError(f"unsupported split policy: {kind}")
    fraction = policy_value.get("validation_fraction", 0.2)
    fraction = _required_number(fraction, "split_policy.validation_fraction")
    if not 0 < fraction < 1:
        raise ValueError("split_policy.validation_fraction must be between 0 and 1")
    seed_value = policy_value.get("seed", payload.get("seed", 0))
    if isinstance(seed_value, bool) or not isinstance(seed_value, int):
        raise ValueError("split_policy.seed must be an integer")
    validation_count = max(1, int(row_count * fraction + 0.5))
    if validation_count >= row_count:
        validation_count = row_count - 1
    if validation_count < 1:
        raise ValueError("training requires at least two rows")

    if kind == "time":
        train_indices = list(range(row_count - validation_count))
        validation_indices = list(range(row_count - validation_count, row_count))
        return {
            "kind": kind,
            "seed": seed_value,
            "validation_fraction": fraction,
            "train_indices": train_indices,
            "validation_indices": validation_indices,
        }

    if kind == "group":
        groups = payload.get("groups")
        if not isinstance(groups, Sequence) or isinstance(groups, (str, bytes)) or len(groups) != row_count:
            raise ValueError("groups must have one value for every feature row")
        ordered_groups: list[str] = []
        for group in groups:
            if group is None or isinstance(group, (dict, list, tuple, set)):
                raise ValueError("groups must contain scalar values")
            value = str(group)
            if value not in ordered_groups:
                ordered_groups.append(value)
        if len(ordered_groups) < 2:
            raise ValueError("group split requires at least two groups")
        rng = random.Random(seed_value)
        shuffled_groups = ordered_groups[:]
        rng.shuffle(shuffled_groups)
        validation_group_count = max(1, int(len(shuffled_groups) * fraction + 0.5))
        if validation_group_count >= len(shuffled_groups):
            validation_group_count = len(shuffled_groups) - 1
        validation_groups = set(shuffled_groups[:validation_group_count])
        validation_indices = [index for index, group in enumerate(groups) if str(group) in validation_groups]
        train_indices = [index for index in range(row_count) if index not in set(validation_indices)]
        return {
            "kind": kind,
            "seed": seed_value,
            "validation_fraction": fraction,
            "validation_groups": sorted(validation_groups),
            "train_indices": train_indices,
            "validation_indices": validation_indices,
        }

    indices = list(range(row_count))
    random.Random(seed_value).shuffle(indices)
    validation_indices = sorted(indices[:validation_count])
    validation_set = set(validation_indices)
    train_indices = [index for index in range(row_count) if index not in validation_set]
    return {
        "kind": kind,
        "seed": seed_value,
        "validation_fraction": fraction,
        "train_indices": train_indices,
        "validation_indices": validation_indices,
    }


def _fit_preprocessing(
    features: Sequence[Sequence[float | None]], train_indices: Sequence[int]
) -> tuple[list[float], list[float], list[list[float]]]:
    feature_count = len(features[0])
    means: list[float] = []
    scales: list[float] = []
    for column in range(feature_count):
        values = [features[index][column] for index in train_indices if features[index][column] is not None]
        if not values:
            raise ValueError(f"feature column {column} has no usable training values")
        mean = sum(values) / len(values)
        variance = sum((value - mean) ** 2 for value in values) / len(values)
        scale = math.sqrt(variance) if variance > 1e-24 else 1.0
        means.append(mean)
        scales.append(scale)
    transformed = [
        [
            ((value if value is not None else means[column]) - means[column]) / scales[column]
            for column, value in enumerate(row)
        ]
        for row in features
    ]
    return means, scales, transformed


def _fit_ordinary_least_squares(
    features: Sequence[Sequence[float]],
    targets: Sequence[float],
    train_indices: Sequence[int],
    ridge: float,
) -> tuple[list[float], float]:
    width = len(features[0]) + 1
    normal = [[0.0 for _ in range(width)] for _ in range(width)]
    vector = [0.0 for _ in range(width)]
    for index in train_indices:
        row = [1.0, *features[index]]
        target = targets[index]
        for left in range(width):
            vector[left] += row[left] * target
            for right in range(width):
                normal[left][right] += row[left] * row[right]
    for diagonal in range(1, width):
        normal[diagonal][diagonal] += ridge
    solution = _solve_linear_system(normal, vector)
    return solution[1:], solution[0]


def _solve_linear_system(matrix: list[list[float]], vector: list[float]) -> list[float]:
    size = len(vector)
    augmented = [row[:] + [vector[index]] for index, row in enumerate(matrix)]
    for column in range(size):
        pivot = max(range(column, size), key=lambda row: abs(augmented[row][column]))
        if abs(augmented[pivot][column]) <= 1e-12:
            raise ValueError("training features are singular")
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


def _predict_transformed(
    transformed: Sequence[Sequence[float]],
    indices: Sequence[int],
    coefficients: Sequence[float],
    intercept: float,
) -> list[float]:
    return [intercept + sum(coefficient * value for coefficient, value in zip(coefficients, transformed[index])) for index in indices]


def _metrics(targets: Sequence[float], predictions: Sequence[float], indices: Sequence[int]) -> dict[str, float | int | None]:
    actual = [targets[index] for index in indices]
    if not actual:
        return {"sample_count": 0, "mae": None, "rmse": None, "r2": None}
    errors = [prediction - target for prediction, target in zip(predictions, actual)]
    squared = sum(error * error for error in errors)
    mean_target = sum(actual) / len(actual)
    total = sum((target - mean_target) ** 2 for target in actual)
    return {
        "sample_count": len(actual),
        "mae": sum(abs(error) for error in errors) / len(errors),
        "rmse": math.sqrt(squared / len(errors)),
        "r2": None if total <= 1e-24 else 1 - squared / total,
    }


def _applicability_range(
    features: Sequence[Sequence[float | None]], train_indices: Sequence[int]
) -> list[dict[str, float | None]]:
    ranges: list[dict[str, float | None]] = []
    for column in range(len(features[0])):
        values = [features[index][column] for index in train_indices if features[index][column] is not None]
        ranges.append({"min": min(values) if values else None, "max": max(values) if values else None})
    return ranges


def _ridge_value(payload: Mapping[str, Any]) -> float:
    value = payload.get("ridge", 1e-10)
    value = _required_number(value, "ridge")
    if value < 0:
        raise ValueError("ridge must not be negative")
    return value


def _mapping(value: Any) -> dict[str, str]:
    if value is None:
        return {}
    if not isinstance(value, Mapping):
        raise ValueError("field_mapping must be an object")
    if not all(isinstance(key, str) and isinstance(item, str) for key, item in value.items()):
        raise ValueError("field_mapping must contain string values")
    return {str(key): str(item) for key, item in value.items()}


def _artifact_id(artifact: Mapping[str, Any]) -> str:
    without_id = {key: value for key, value in artifact.items() if key != "model_id"}
    encoded = json.dumps(without_id, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()[:24]
