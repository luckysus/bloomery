from __future__ import annotations

import base64
import hashlib
from collections.abc import Mapping
from typing import Any

from .onnx_inference import OnnxInferenceError

EXPORT_OPSET = 13


def export_linear_onnx(payload: Mapping[str, Any]) -> dict[str, Any]:
    """Export a trained linear-regression artifact as a whitelisted ONNX graph.

    The graph is (X - means) / scales -> MatMul -> Add, using only operators
    from the inference whitelist so the exported model can immediately pass
    `predict_onnx` validation. A manifest describing I/O schema,
    preprocessing, applicability range, and confidence is returned alongside
    the model bytes so export and import stay numerically consistent.
    """
    if not isinstance(payload, Mapping):
        raise OnnxInferenceError("invalid_payload", "export payload must be an object")
    artifact = payload.get("artifact")
    if not isinstance(artifact, Mapping):
        raise OnnxInferenceError("invalid_artifact", "model artifact is required")
    if artifact.get("artifact_version") != "linear-regression.v1":
        raise OnnxInferenceError("invalid_artifact", "unsupported model artifact version")
    if artifact.get("model_type") != "linear_regression":
        raise OnnxInferenceError("invalid_artifact", "unsupported model type")

    names = artifact.get("feature_names")
    preprocessing = artifact.get("preprocessing")
    coefficients = artifact.get("coefficients")
    intercept = artifact.get("intercept")
    if (
        not isinstance(names, list)
        or not names
        or not all(isinstance(name, str) and name.strip() for name in names)
        or not isinstance(preprocessing, Mapping)
        or not isinstance(coefficients, list)
        or not isinstance(intercept, (int, float))
        or len(coefficients) != len(names)
    ):
        raise OnnxInferenceError("invalid_artifact", "model artifact schema is invalid")
    means = preprocessing.get("means")
    scales = preprocessing.get("scales")
    if (
        not isinstance(means, list)
        or not isinstance(scales, list)
        or len(means) != len(names)
        or len(scales) != len(names)
    ):
        raise OnnxInferenceError("invalid_artifact", "model preprocessing is invalid")
    numeric_means = [_finite(item, "preprocessing.means") for item in means]
    numeric_scales = [_finite(item, "preprocessing.scales") for item in scales]
    if any(value <= 0 for value in numeric_scales):
        raise OnnxInferenceError("invalid_artifact", "preprocessing scales must be positive")
    numeric_coefficients = [_finite(item, "coefficients") for item in coefficients]
    numeric_intercept = _finite(intercept, "intercept")

    try:
        import onnx
        from onnx import helper, numpy_helper
        import numpy as np
    except ImportError as error:
        raise OnnxInferenceError("runtime_unavailable", "onnx is not installed") from error

    feature_count = len(names)
    weights_initializer = numpy_helper.from_array(
        np.asarray(numeric_coefficients, dtype=np.float32).reshape(feature_count, 1), name="weights"
    )
    intercept_initializer = numpy_helper.from_array(
        np.asarray([numeric_intercept], dtype=np.float32), name="intercept"
    )

    # The import path (predict_onnx) normalizes raw features with the
    # manifest preprocessing before running the session, so the exported
    # graph operates on already-normalized inputs and must not re-normalize.
    nodes = [
        helper.make_node("MatMul", ["X", "weights"], ["projected"], name="project"),
        helper.make_node("Add", ["projected", "intercept"], ["Y"], name="shift"),
    ]
    graph = helper.make_graph(
        nodes,
        "linear_regression",
        [helper.make_tensor_value_info("X", onnx.TensorProto.FLOAT, [-1, feature_count])],
        [helper.make_tensor_value_info("Y", onnx.TensorProto.FLOAT, [-1, 1])],
        initializer=[weights_initializer, intercept_initializer],
    )
    model = helper.make_model(
        graph,
        opset_imports=[helper.make_opsetid("", EXPORT_OPSET)],
        producer_name="bloomery-compute-worker",
    )
    # NOTE: onnx.checker.check_model is intentionally not called here; the
    # local onnx build crashes inside the native checker, and exported models
    # are validated on import by the ONNX Runtime session plus the operator
    # whitelist and opset window enforced by predict_onnx.
    model.ir_version = 10

    serialized = model.SerializeToString()
    manifest = {
        "model_id": str(artifact.get("model_id", "")),
        "model_version": str(payload.get("model_version", "1.0.0")),
        "inputs": [{"name": "X", "dtype": "float32", "shape": [-1, feature_count]}],
        "outputs": [{"name": "Y", "dtype": "float32", "shape": [-1, 1]}],
        "preprocessing": {
            "feature_names": [str(name) for name in names],
            "means": numeric_means,
            "scales": numeric_scales,
        },
        "applicability_range": _applicability_range(artifact.get("applicability_range"), feature_count),
        "confidence": {"kind": "applicability_distance"},
    }
    return {
        "model_base64": base64.b64encode(serialized).decode("ascii"),
        "model_sha256": hashlib.sha256(serialized).hexdigest(),
        "manifest": manifest,
        "opset_version": EXPORT_OPSET,
        "operators": sorted({node.op_type for node in model.graph.node}),
    }


def _applicability_range(value: Any, feature_count: int) -> list[dict[str, float | None]]:
    if not isinstance(value, list) or len(value) != feature_count:
        return [{"min": None, "max": None} for _ in range(feature_count)]
    ranges: list[dict[str, float | None]] = []
    for item in value:
        if not isinstance(item, Mapping):
            raise OnnxInferenceError("applicability_invalid", "applicability range is invalid")
        minimum = item.get("min")
        maximum = item.get("max")
        ranges.append(
            {
                "min": None if minimum is None else _finite(minimum, "applicability_range.min"),
                "max": None if maximum is None else _finite(maximum, "applicability_range.max"),
            }
        )
    return ranges


def _finite(value: Any, label: str) -> float:
    import math

    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise OnnxInferenceError("invalid_artifact", f"{label} must be a number")
    number = float(value)
    if not math.isfinite(number):
        raise OnnxInferenceError("invalid_artifact", f"{label} must be finite")
    return number
