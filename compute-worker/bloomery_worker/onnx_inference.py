from __future__ import annotations

import hashlib
import math
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any


MAX_ROWS = 100_000
MAX_FEATURES = 128
INFERENCE_CHUNK_ROWS = 8192

# Opset window matches the pinned ONNX Runtime: older graphs still load, and
# anything newer has not been exercised by the worker whitelist tests.
MIN_OPSET = 7
MAX_OPSET = 21
SUPPORTED_DOMAINS = {"", "ai.onnx"}

# Whitelist of operators allowed for local steel-domain regression graphs.
# Adding an operator requires a worker test and a release note.
SUPPORTED_OPERATORS = frozenset(
    {
        "Abs",
        "Add",
        "BatchNormalization",
        "Cast",
        "Clip",
        "Concat",
        "Constant",
        "Div",
        "Dropout",
        "Elu",
        "Erf",
        "Exp",
        "Flatten",
        "Gelu",
        "Gemm",
        "HardSigmoid",
        "Identity",
        "LeakyRelu",
        "Log",
        "MatMul",
        "Mul",
        "Neg",
        "Pow",
        "PRelu",
        "ReduceMax",
        "ReduceMean",
        "ReduceMin",
        "ReduceSum",
        "Relu",
        "Reshape",
        "Selu",
        "Shape",
        "Sigmoid",
        "Softmax",
        "Softplus",
        "Sqrt",
        "Squeeze",
        "Sub",
        "Sum",
        "Tanh",
        "Transpose",
        "Unsqueeze",
    }
)


class OnnxInferenceError(ValueError):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


def predict_onnx(
    payload: Mapping[str, Any],
    report: "callable | None" = None,
) -> dict[str, Any]:
    def _report(stage: str, progress: int) -> None:
        if report is not None:
            report(stage, progress)

    if not isinstance(payload, Mapping):
        raise OnnxInferenceError("invalid_payload", "ONNX payload must be an object")

    path = _model_path(payload.get("model_path"))
    manifest = payload.get("manifest")
    if not isinstance(manifest, Mapping):
        raise OnnxInferenceError("model_schema_mismatch", "ONNX manifest must be an object")

    expected_hash = payload.get("model_sha256", manifest.get("model_sha256"))
    if not isinstance(expected_hash, str) or len(expected_hash) != 64:
        raise OnnxInferenceError("model_hash_mismatch", "ONNX model sha256 is required")
    if any(char not in "0123456789abcdef" for char in expected_hash):
        raise OnnxInferenceError("model_hash_mismatch", "ONNX model sha256 is invalid")
    actual_hash = _sha256(path)
    if actual_hash != expected_hash:
        raise OnnxInferenceError("model_hash_mismatch", "model hash does not match")
    _report("validated", 20)

    opset, operators = _model_provenance(path)
    _report("validated", 35)

    features = _features(payload.get("features"))
    preprocessing = _preprocessing(manifest.get("preprocessing"), len(features[0]))
    normalized = _normalize(features, preprocessing)
    _report("normalized", 55)

    session, input_specs, output_specs = _load_session(path)
    _validate_io_schema(manifest, input_specs, output_specs, len(normalized), len(normalized[0]))
    _report("validated", 65)

    try:
        import numpy as np

        input_name = input_specs[0]["name"]
        chunks = [
            normalized[start:start + INFERENCE_CHUNK_ROWS]
            for start in range(0, len(normalized), INFERENCE_CHUNK_ROWS)
        ]
        chunk_outputs: list[list[Any]] = []
        for index, chunk in enumerate(chunks):
            chunk_outputs.append(
                session.run(None, {input_name: np.asarray(chunk, dtype=np.float32)})
            )
            _report("inference", 65 + int(30 * (index + 1) / len(chunks)))
        output_values = [np.concatenate(parts, axis=0) for parts in zip(*chunk_outputs)]
    except OnnxInferenceError:
        raise
    except Exception as error:
        raise OnnxInferenceError("inference_failed", "ONNX inference failed") from error

    if not output_values:
        raise OnnxInferenceError("model_schema_mismatch", "ONNX model returned no outputs")
    if any(not _is_numeric_tensor(spec["type"]) for spec in output_specs):
        raise OnnxInferenceError(
            "model_schema_mismatch", "ONNX outputs must be numeric tensors"
        )

    warnings = _applicability_warnings(
        features, preprocessing["feature_names"], manifest.get("applicability_range")
    )
    confidence = _confidence(manifest.get("confidence"), features, manifest.get("applicability_range"))
    outputs = {
        spec["name"]: _json_value(value)
        for spec, value in zip(output_specs, output_values)
    }
    return {
        "model_id": _required_text(manifest.get("model_id"), "model_id"),
        "model_version": _required_text(manifest.get("model_version"), "model_version"),
        "model_sha256": actual_hash,
        "opset_version": opset,
        "operators": operators,
        "input_schema": input_specs,
        "output_schema": output_specs,
        "preprocessing": dict(manifest["preprocessing"]),
        "normalized_inputs": normalized,
        "predictions": _json_value(output_values[0]),
        "outputs": outputs,
        "applicability_warnings": warnings,
        "confidence": confidence,
        "constraints": list(manifest.get("constraints", [])),
    }


def _model_provenance(path: Path) -> tuple[int, list[str]]:
    try:
        import onnx
    except ImportError as error:
        raise OnnxInferenceError(
            "runtime_unavailable", "onnx is not installed"
        ) from error

    try:
        model = onnx.load(str(path), load_external_data=False)
    except Exception as error:
        raise OnnxInferenceError("invalid_model", "could not parse ONNX model graph") from error

    opset: int | None = None
    for entry in model.opset_import:
        if entry.domain not in SUPPORTED_DOMAINS:
            raise OnnxInferenceError(
                "unsupported_operator_domain", "ONNX model uses an unsupported operator domain"
            )
        if opset is not None:
            raise OnnxInferenceError(
                "unsupported_opset", "ONNX model declares duplicate default opsets"
            )
        opset = int(entry.version)
    if opset is None:
        raise OnnxInferenceError("unsupported_opset", "ONNX model declares no default opset")
    if opset < MIN_OPSET or opset > MAX_OPSET:
        raise OnnxInferenceError(
            "unsupported_opset",
            f"ONNX opset {opset} is outside the supported range {MIN_OPSET}-{MAX_OPSET}",
        )

    operators = sorted({node.op_type for node in model.graph.node})
    unsupported = [operator for operator in operators if operator not in SUPPORTED_OPERATORS]
    if unsupported:
        raise OnnxInferenceError(
            "unsupported_operator",
            f"ONNX model uses unsupported operators: {', '.join(unsupported)}",
        )
    if not operators:
        raise OnnxInferenceError("invalid_model", "ONNX model graph has no operators")
    return opset, operators


def _confidence(declaration: Any, features: Sequence[Sequence[float]], ranges_value: Any) -> list[float] | None:
    if declaration is None:
        return None
    if not isinstance(declaration, Mapping) or declaration.get("kind") != "applicability_distance":
        raise OnnxInferenceError(
            "confidence_invalid", "confidence manifest must declare kind applicability_distance"
        )
    if ranges_value is None:
        raise OnnxInferenceError(
            "confidence_invalid", "confidence requires an applicability_range declaration"
        )
    ranges: list[tuple[float | None, float | None]] = []
    for item in ranges_value:
        if not isinstance(item, Mapping):
            raise OnnxInferenceError("confidence_invalid", "applicability range is invalid")
        minimum = item.get("min")
        maximum = item.get("max")
        if minimum is not None:
            minimum = _finite_number(minimum, "applicability_range.min")
        if maximum is not None:
            maximum = _finite_number(maximum, "applicability_range.max")
        ranges.append((minimum, maximum))

    values: list[float] = []
    for row in features:
        penalty = 0.0
        for column, number in enumerate(row):
            if column >= len(ranges):
                continue
            minimum, maximum = ranges[column]
            violation = 0.0
            if minimum is not None and number < minimum:
                violation = minimum - number
            elif maximum is not None and number > maximum:
                violation = number - maximum
            if violation <= 0.0:
                continue
            if minimum is not None and maximum is not None and maximum > minimum:
                penalty += violation / (maximum - minimum)
            else:
                penalty += 1.0
        values.append(1.0 / (1.0 + penalty))
    return values


def _model_path(value: Any) -> Path:
    if not isinstance(value, str) or not value.strip():
        raise OnnxInferenceError("invalid_model", "model_path is required")
    path = Path(value)
    if not path.is_file():
        raise OnnxInferenceError("invalid_model", "ONNX model file was not found")
    return path


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _load_session(path: Path) -> tuple[Any, list[dict[str, Any]], list[dict[str, Any]]]:
    try:
        import onnxruntime as ort
    except ImportError as error:
        raise OnnxInferenceError(
            "runtime_unavailable", "onnxruntime is not installed"
        ) from error

    try:
        session = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])
    except Exception as error:
        raise OnnxInferenceError("invalid_model", "could not load ONNX model") from error

    inputs = [_io_spec(value) for value in session.get_inputs()]
    outputs = [_io_spec(value) for value in session.get_outputs()]
    if len(inputs) != 1 or not _is_numeric_tensor(inputs[0]["type"]):
        raise OnnxInferenceError(
            "model_schema_mismatch", "ONNX model must expose one numeric tensor input"
        )
    return session, inputs, outputs


def _io_spec(value: Any) -> dict[str, Any]:
    return {
        "name": str(value.name),
        "type": str(value.type),
        "shape": [_dimension(item) for item in value.shape],
    }


def _dimension(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value if value >= 0 else None
    return None


def _validate_io_schema(
    manifest: Mapping[str, Any],
    inputs: list[dict[str, Any]],
    outputs: list[dict[str, Any]],
    row_count: int,
    feature_count: int,
) -> None:
    declared_inputs = manifest.get("inputs")
    declared_outputs = manifest.get("outputs")
    if not isinstance(declared_inputs, list) or not isinstance(declared_outputs, list):
        raise OnnxInferenceError(
            "model_schema_mismatch", "ONNX manifest inputs and outputs are required"
        )
    if len(declared_inputs) != len(inputs) or len(declared_outputs) != len(outputs):
        raise OnnxInferenceError("model_schema_mismatch", "ONNX I/O schema does not match")
    for declared, actual in zip(declared_inputs, inputs):
        _validate_io_entry(declared, actual)
    for declared, actual in zip(declared_outputs, outputs):
        _validate_io_entry(declared, actual)
    actual_shape = inputs[0]["shape"]
    if len(actual_shape) != 2:
        raise OnnxInferenceError(
            "model_schema_mismatch", "ONNX input must be a two-dimensional tensor"
        )
    _validate_dimension(actual_shape[0], row_count, "row count")
    _validate_dimension(actual_shape[1], feature_count, "feature count")


def _validate_io_entry(declared: Any, actual: Mapping[str, Any]) -> None:
    if not isinstance(declared, Mapping):
        raise OnnxInferenceError("model_schema_mismatch", "ONNX I/O entry is invalid")
    if declared.get("name") != actual["name"]:
        raise OnnxInferenceError("model_schema_mismatch", "input schema does not match")
    if _dtype_name(declared.get("dtype")) != _dtype_name(actual["type"]):
        raise OnnxInferenceError("model_schema_mismatch", "ONNX dtype schema does not match")
    declared_shape = declared.get("shape")
    if not isinstance(declared_shape, list) or not _shape_matches(declared_shape, actual["shape"]):
        raise OnnxInferenceError("model_schema_mismatch", "ONNX shape schema does not match")


def _shape_matches(declared: Sequence[Any], actual: Sequence[Any]) -> bool:
    if len(declared) != len(actual):
        return False
    for expected, current in zip(declared, actual):
        if expected in (-1, None) or isinstance(expected, str):
            continue
        if not isinstance(expected, int) or isinstance(expected, bool):
            return False
        if current is not None and expected != current:
            return False
    return True


def _validate_dimension(actual: int | None, expected: int, label: str) -> None:
    if actual is not None and actual != expected:
        raise OnnxInferenceError("input_shape_mismatch", f"ONNX {label} does not match")


def _features(value: Any) -> list[list[float]]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)) or not value:
        raise OnnxInferenceError("invalid_payload", "features must be a non-empty matrix")
    if len(value) > MAX_ROWS:
        raise OnnxInferenceError("invalid_payload", "features exceed the row limit")
    first = value[0]
    if not isinstance(first, Sequence) or isinstance(first, (str, bytes)) or not first:
        raise OnnxInferenceError("invalid_payload", "features must contain non-empty rows")
    width = len(first)
    if width > MAX_FEATURES:
        raise OnnxInferenceError("invalid_payload", "features exceed the column limit")
    rows: list[list[float]] = []
    for row_index, row in enumerate(value):
        if not isinstance(row, Sequence) or isinstance(row, (str, bytes)) or len(row) != width:
            raise OnnxInferenceError("input_shape_mismatch", f"features[{row_index}] has invalid width")
        rows.append([_finite_number(item, f"features[{row_index}][{column}]") for column, item in enumerate(row)])
    return rows


def _preprocessing(value: Any, feature_count: int) -> dict[str, list[Any]]:
    if not isinstance(value, Mapping):
        raise OnnxInferenceError("preprocessing_invalid", "preprocessing manifest is required")
    names = value.get("feature_names")
    means = value.get("means")
    scales = value.get("scales")
    if (
        not isinstance(names, list)
        or len(names) != feature_count
        or not all(isinstance(name, str) and name.strip() for name in names)
        or not isinstance(means, list)
        or not isinstance(scales, list)
        or len(means) != feature_count
        or len(scales) != feature_count
    ):
        raise OnnxInferenceError("preprocessing_invalid", "preprocessing manifest does not match inputs")
    numeric_means = [_finite_number(value, "preprocessing.means") for value in means]
    numeric_scales = [_finite_number(value, "preprocessing.scales") for value in scales]
    if any(value <= 0 for value in numeric_scales):
        raise OnnxInferenceError("preprocessing_invalid", "preprocessing scales must be positive")
    return {
        "feature_names": [str(name) for name in names],
        "means": numeric_means,
        "scales": numeric_scales,
    }


def _normalize(features: Sequence[Sequence[float]], preprocessing: Mapping[str, Sequence[float]]) -> list[list[float]]:
    means = preprocessing["means"]
    scales = preprocessing["scales"]
    return [
        [float((value - means[column]) / scales[column]) for column, value in enumerate(row)]
        for row in features
    ]


def _applicability_warnings(
    features: Sequence[Sequence[float]], names: Sequence[str], value: Any
) -> list[dict[str, Any]]:
    if value is None:
        return []
    if not isinstance(value, list) or len(value) != len(names):
        raise OnnxInferenceError("applicability_invalid", "applicability range does not match inputs")
    ranges: list[tuple[float | None, float | None]] = []
    for item in value:
        if not isinstance(item, Mapping):
            raise OnnxInferenceError("applicability_invalid", "applicability range is invalid")
        minimum = item.get("min")
        maximum = item.get("max")
        if minimum is not None:
            minimum = _finite_number(minimum, "applicability_range.min")
        if maximum is not None:
            maximum = _finite_number(maximum, "applicability_range.max")
        if minimum is not None and maximum is not None and minimum > maximum:
            raise OnnxInferenceError("applicability_invalid", "applicability range is inverted")
        ranges.append((minimum, maximum))
    warnings = []
    for row_index, row in enumerate(features):
        for column, number in enumerate(row):
            minimum, maximum = ranges[column]
            if (minimum is not None and number < minimum) or (maximum is not None and number > maximum):
                warnings.append(
                    {
                        "row": row_index,
                        "feature": names[column],
                        "index": column,
                        "value": number,
                        "min": minimum,
                        "max": maximum,
                        "code": "outside_applicability_range",
                    }
                )
    return warnings


def _required_text(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise OnnxInferenceError("model_schema_mismatch", f"{label} is required")
    return value.strip()


def _finite_number(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise OnnxInferenceError("invalid_payload", f"{label} must be a number")
    number = float(value)
    if not math.isfinite(number):
        raise OnnxInferenceError("invalid_payload", f"{label} must be finite")
    return number


def _dtype_name(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    normalized = value.lower().strip()
    if normalized.startswith("tensor(") and normalized.endswith(")"):
        normalized = normalized[7:-1]
    aliases = {"float": "float32", "double": "float64", "int64": "int64", "int32": "int32"}
    return aliases.get(normalized, normalized)


def _is_numeric_tensor(value: Any) -> bool:
    return _dtype_name(value) in {"float32", "float64", "int32", "int64"}


def _json_value(value: Any) -> Any:
    if hasattr(value, "tolist"):
        return _json_value(value.tolist())
    if isinstance(value, list):
        return [_json_value(item) for item in value]
    if isinstance(value, tuple):
        return [_json_value(item) for item in value]
    if isinstance(value, Mapping):
        return {str(key): _json_value(item) for key, item in value.items()}
    if isinstance(value, (int, float, str, bool)) or value is None:
        return value
    if hasattr(value, "item"):
        return _json_value(value.item())
    raise OnnxInferenceError("inference_failed", "ONNX output is not JSON serializable")
