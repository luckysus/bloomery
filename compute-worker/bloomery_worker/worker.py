from __future__ import annotations

from typing import Any, BinaryIO

from .onnx_inference import OnnxInferenceError, predict_onnx
from .optimization import OptimizationError, optimize_constrained
from .protocol import PROTOCOL_VERSION, FrameError, encode_frame, parse_request, read_frame
from .training import predict_linear_regression, train_linear_regression

WORKER_VERSION = "0.4.0"


def _response(request_id: str, *, result: dict[str, Any] | None = None, error: dict[str, Any] | None = None) -> dict[str, Any]:
    message: dict[str, Any] = {
        "jsonrpc": "2.0",
        "protocol_version": PROTOCOL_VERSION,
        "id": request_id,
    }
    if result is not None:
        message["result"] = result
    if error is not None:
        message["error"] = error
    return message


def _error(request_id: str, code: str, message: str) -> dict[str, Any]:
    return _response(request_id, error={"code": code, "message": message})


def _progress(task_id: str, progress: int, stage: str) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "protocol_version": PROTOCOL_VERSION,
        "method": "progress",
        "params": {"task_id": task_id, "progress": progress, "stage": stage},
    }


def _write(stream: BinaryIO, message: dict[str, Any]) -> None:
    stream.write(encode_frame(message))
    stream.flush()


def _dispatch(request: dict[str, Any], output: BinaryIO) -> bool:
    request_id = request["id"]
    method = request["method"]
    params = request.get("params", {})

    if method == "hello":
        _write(
            output,
            _response(
                request_id,
                result={
                    "protocol_version": PROTOCOL_VERSION,
                    "worker_version": WORKER_VERSION,
                    "capabilities": ["hello", "submit", "cancel", "shutdown", "training", "inference", "optimization"],
                    "operations": [
                        "echo",
                        "train_linear_regression",
                        "predict_linear_regression",
                        "predict_onnx",
                        "optimize_constrained",
                    ],
                },
            ),
        )
        return False

    if method == "shutdown":
        _write(output, _response(request_id, result={"state": "stopped"}))
        return True

    if method == "cancel":
        task_id = params.get("task_id")
        if not isinstance(task_id, str) or not task_id.strip():
            _write(output, _error(request_id, "invalid_params", "task_id is required"))
        else:
            _write(
                output,
                _response(
                    request_id,
                    result={"task_id": task_id, "state": "cancelled"},
                ),
            )
        return False

    if method == "submit":
        task_id = params.get("task_id")
        operation = params.get("operation")
        if not isinstance(task_id, str) or not task_id.strip():
            _write(output, _error(request_id, "invalid_params", "task_id is required"))
        elif operation not in {
            "echo",
            "train_linear_regression",
            "predict_linear_regression",
            "predict_onnx",
            "optimize_constrained",
        }:
            _write(
                output,
                _error(request_id, "unsupported_operation", "the requested worker operation is not available"),
            )
        elif operation == "echo":
            _write(output, _progress(task_id, 100, "completed"))
            _write(
                output,
                _response(
                    request_id,
                    result={
                        "task_id": task_id,
                        "state": "completed",
                        "value": params.get("payload", {}),
                    },
                ),
            )
        elif operation == "train_linear_regression":
            payload = params.get("payload", {})
            if not isinstance(payload, dict):
                _write(output, _error(request_id, "invalid_params", "payload must be an object"))
                return False
            _write(output, _progress(task_id, 10, "training"))
            try:
                artifact = train_linear_regression(payload)
            except ValueError as error:
                _write(output, _error(request_id, "invalid_payload", str(error)))
            else:
                _write(output, _progress(task_id, 100, "completed"))
                _write(
                    output,
                    _response(
                        request_id,
                        result={"task_id": task_id, "state": "completed", "artifact": artifact},
                    ),
                )
        elif operation == "predict_linear_regression":
            payload = params.get("payload", {})
            if not isinstance(payload, dict):
                _write(output, _error(request_id, "invalid_params", "payload must be an object"))
                return False
            artifact = payload.get("artifact")
            features = payload.get("features")
            if not isinstance(artifact, dict) or not isinstance(features, list):
                _write(output, _error(request_id, "invalid_params", "artifact and features are required"))
                return False
            _write(output, _progress(task_id, 10, "inference"))
            try:
                prediction = predict_linear_regression(artifact, features)
            except ValueError as error:
                _write(output, _error(request_id, "invalid_payload", str(error)))
            else:
                _write(output, _progress(task_id, 100, "completed"))
                _write(
                    output,
                    _response(
                        request_id,
                        result={"task_id": task_id, "state": "completed", **prediction},
                    ),
                )
        elif operation == "predict_onnx":
            payload = params.get("payload", {})
            if not isinstance(payload, dict):
                _write(output, _error(request_id, "invalid_params", "payload must be an object"))
                return False
            _write(output, _progress(task_id, 10, "inference"))
            try:
                prediction = predict_onnx(
                    payload,
                    report=lambda stage, progress: _write(
                        output, _progress(task_id, progress, stage)
                    ),
                )
            except OnnxInferenceError as error:
                _write(output, _error(request_id, error.code, str(error)))
            else:
                _write(output, _progress(task_id, 100, "completed"))
                _write(
                    output,
                    _response(
                        request_id,
                        result={"task_id": task_id, "state": "completed", **prediction},
                    ),
                )
        elif operation == "optimize_constrained":
            payload = params.get("payload", {})
            if not isinstance(payload, dict):
                _write(output, _error(request_id, "invalid_params", "payload must be an object"))
                return False
            _write(output, _progress(task_id, 5, "validated"))
            try:
                result = optimize_constrained(
                    payload,
                    report=lambda stage, progress: _write(
                        output, _progress(task_id, progress, stage)
                    ),
                )
            except OptimizationError as error:
                if error.code == "optimization_cancelled":
                    _write(
                        output,
                        _response(
                            request_id,
                            result={"task_id": task_id, "state": "cancelled"},
                        ),
                    )
                else:
                    _write(output, _error(request_id, error.code, str(error)))
            else:
                _write(output, _progress(task_id, 100, "completed"))
                _write(
                    output,
                    _response(
                        request_id,
                        result={"task_id": task_id, "state": "completed", **result},
                    ),
                )
        else:
            _write(
                output,
                _error(
                    request_id,
                    "unsupported_operation",
                    "supported operations are echo, train_linear_regression, predict_linear_regression, predict_onnx, and optimize_constrained",
                ),
            )
        return False

    _write(output, _error(request_id, "method_not_found", f"unknown worker method: {method}"))
    return False


def serve(input_stream: BinaryIO, output_stream: BinaryIO) -> None:
    while True:
        message = read_frame(input_stream)
        if message is None:
            return
        try:
            request = parse_request(message)
        except FrameError:
            # A malformed envelope has no trustworthy request ID to reply to.
            raise
        if _dispatch(request, output_stream):
            return
