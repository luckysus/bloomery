from __future__ import annotations

import hashlib
from pathlib import Path

import pytest

onnxruntime = pytest.importorskip("onnxruntime")

from bloomery_worker.onnx_inference import OnnxInferenceError, predict_onnx
from bloomery_worker.protocol import encode_frame, read_frame
from bloomery_worker.worker import serve


def _model_path() -> Path:
    from onnxruntime.datasets import get_example

    return Path(get_example("mul_1.onnx"))


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _payload(path: Path | None = None) -> dict:
    path = path or _model_path()
    model_sha256 = _sha256(path)
    return {
        "model_path": str(path),
        "model_sha256": model_sha256,
        "manifest": {
            "model_id": "mul-model",
            "model_version": "1.0.0",
            "inputs": [{"name": "X", "dtype": "float32", "shape": [-1, 2]}],
            "outputs": [{"name": "Y", "dtype": "float32", "shape": [-1, 2]}],
            "preprocessing": {
                "feature_names": ["temperature", "carbon"],
                "means": [0.0, 0.0],
                "scales": [1.0, 1.0],
            },
            "applicability_range": [
                {"min": 0.0, "max": 10.0},
                {"min": 0.0, "max": 10.0},
            ],
        },
        "features": [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]],
    }


def test_onnx_inference_validates_manifest_and_returns_batch_provenance() -> None:
    result = predict_onnx(_payload())

    assert result["model_id"] == "mul-model"
    assert result["model_version"] == "1.0.0"
    assert result["predictions"] == [[1.0, 4.0], [9.0, 16.0], [25.0, 36.0]]
    assert result["normalized_inputs"] == [
        [1.0, 2.0],
        [3.0, 4.0],
        [5.0, 6.0],
    ]
    assert result["applicability_warnings"] == []
    assert result["confidence"] is None
    assert result["opset_version"] == 7
    assert result["operators"] == ["Mul"]


def test_onnx_inference_reports_out_of_range_inputs_without_hiding_predictions() -> None:
    payload = _payload()
    payload["features"] = [[11.0, 2.0], [3.0, -1.0], [5.0, 6.0]]

    result = predict_onnx(payload)

    assert result["predictions"] == [[11.0, 4.0], [9.0, -4.0], [25.0, 36.0]]
    assert [warning["feature"] for warning in result["applicability_warnings"]] == [
        "temperature",
        "carbon",
    ]


def test_onnx_inference_rejects_a_model_hash_mismatch() -> None:
    payload = _payload()
    payload["model_sha256"] = "0" * 64

    with pytest.raises(OnnxInferenceError, match="model hash does not match") as error:
        predict_onnx(payload)

    assert error.value.code == "model_hash_mismatch"


def test_onnx_inference_rejects_input_schema_mismatch() -> None:
    payload = _payload()
    payload["manifest"]["inputs"][0]["name"] = "wrong-input"

    with pytest.raises(OnnxInferenceError, match="input schema does not match") as error:
        predict_onnx(payload)

    assert error.value.code == "model_schema_mismatch"


def test_onnx_inference_rejects_invalid_or_unsupported_model_bytes(tmp_path: Path) -> None:
    path = tmp_path / "unsupported.onnx"
    path.write_bytes(b"not-an-onnx-model")

    with pytest.raises(OnnxInferenceError, match="could not parse ONNX model graph") as error:
        predict_onnx(_payload(path))

    assert error.value.code == "invalid_model"


def test_worker_exposes_and_runs_onnx_inference_operation() -> None:
    payload = _payload()
    request = {
        "jsonrpc": "2.0",
        "protocol_version": "1.0",
        "id": "onnx-1",
        "method": "submit",
        "params": {
            "task_id": "onnx-task-1",
            "operation": "predict_onnx",
            "payload": payload,
        },
    }
    shutdown = {
        "jsonrpc": "2.0",
        "protocol_version": "1.0",
        "id": "shutdown-1",
        "method": "shutdown",
        "params": {},
    }
    import io

    input_stream = io.BytesIO(encode_frame({
        "jsonrpc": "2.0",
        "protocol_version": "1.0",
        "id": "hello-1",
        "method": "hello",
        "params": {},
    }) + encode_frame(request) + encode_frame(shutdown))
    output_stream = io.BytesIO()

    serve(input_stream, output_stream)
    output_stream.seek(0)
    messages = []
    while (message := read_frame(output_stream)) is not None:
        messages.append(message)

    assert "predict_onnx" in messages[0]["result"]["operations"]
    progress = [message for message in messages if message.get("method") == "progress"]
    stages = [message["params"]["stage"] for message in progress]
    assert stages[0] == "inference"
    assert "validated" in stages
    assert "normalized" in stages
    assert stages[-1] == "completed"
    values = [message["params"]["progress"] for message in progress]
    assert values == sorted(values)
    result = next(message for message in messages if message.get("id") == "onnx-1")
    assert result["result"]["state"] == "completed"
    assert result["result"]["predictions"] == [[1.0, 4.0], [9.0, 16.0], [25.0, 36.0]]


def _write_single_node_model(
    tmp_path: Path, op_type: str, opset: int, domain: str = ""
) -> Path:
    import onnx
    from onnx import TensorProto, helper

    x = helper.make_tensor_value_info("X", TensorProto.FLOAT, [None, 2])
    y = helper.make_tensor_value_info("Y", TensorProto.FLOAT, [None, 2])
    node = helper.make_node(op_type, ["X"], ["Y"], name="node-1")
    graph = helper.make_graph([node], "graph", [x], [y])
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid(domain, opset)])
    model.ir_version = 8
    path = tmp_path / f"{op_type.lower()}-opset{opset}.onnx"
    onnx.save(model, str(path))
    return path


def test_onnx_inference_rejects_unsupported_operator(tmp_path: Path) -> None:
    path = _write_single_node_model(tmp_path, "LSTM", 13)

    with pytest.raises(OnnxInferenceError, match="unsupported operators: LSTM") as error:
        predict_onnx(_payload(path))

    assert error.value.code == "unsupported_operator"


def test_onnx_inference_rejects_opset_outside_supported_range(tmp_path: Path) -> None:
    path = _write_single_node_model(tmp_path, "Mul", 99)

    with pytest.raises(OnnxInferenceError, match="outside the supported range") as error:
        predict_onnx(_payload(path))

    assert error.value.code == "unsupported_opset"


def test_onnx_inference_rejects_custom_operator_domains(tmp_path: Path) -> None:
    path = _write_single_node_model(tmp_path, "Mul", 13, domain="com.example")

    with pytest.raises(OnnxInferenceError, match="unsupported operator domain") as error:
        predict_onnx(_payload(path))

    assert error.value.code == "unsupported_operator_domain"


def test_onnx_inference_reports_confidence_from_applicability_distance() -> None:
    payload = _payload()
    payload["manifest"]["confidence"] = {"kind": "applicability_distance"}
    payload["features"] = [[1.0, 2.0], [3.0, 4.0], [11.0, 2.0]]

    result = predict_onnx(payload)

    assert result["confidence"] == pytest.approx([1.0, 1.0, 1.0 / 1.1])


def test_onnx_inference_rejects_confidence_declaration_without_ranges() -> None:
    payload = _payload()
    payload["manifest"]["confidence"] = {"kind": "applicability_distance"}
    del payload["manifest"]["applicability_range"]

    with pytest.raises(OnnxInferenceError, match="requires an applicability_range") as error:
        predict_onnx(payload)

    assert error.value.code == "confidence_invalid"


def test_onnx_inference_chunks_large_batches_and_keeps_row_order(tmp_path: Path) -> None:
    from bloomery_worker import onnx_inference

    path = _write_single_node_model(tmp_path, "Sqrt", 13)
    payload = _payload(path)
    payload["features"] = [[float(index % 10), float(index % 5)] for index in range(8193)]

    assert onnx_inference.INFERENCE_CHUNK_ROWS < 8193
    result = predict_onnx(payload)

    assert len(result["predictions"]) == 8193
    assert result["predictions"][0] == [0.0, 0.0]
    last = result["predictions"][-1]
    assert last[0] == pytest.approx(float(8192 % 10) ** 0.5)
    assert last[1] == pytest.approx(float(8192 % 5) ** 0.5)
