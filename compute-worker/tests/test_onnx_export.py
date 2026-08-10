import base64
import io

import pytest

from bloomery_worker.onnx_export import EXPORT_OPSET, export_linear_onnx
from bloomery_worker.onnx_inference import SUPPORTED_OPERATORS, OnnxInferenceError, predict_onnx
from bloomery_worker.protocol import encode_frame, read_frame
from bloomery_worker.training import predict_linear_regression, train_linear_regression
from bloomery_worker.worker import serve


def trained_artifact() -> dict:
    return train_linear_regression(
        {
            "features": [[0.0], [10.0], [20.0], [30.0], [40.0], [50.0], [60.0], [70.0]],
            "targets": [5.0, 25.0, 45.0, 65.0, 85.0, 105.0, 125.0, 145.0],
            "feature_names": ["temperature"],
            "split_policy": {"kind": "time", "validation_fraction": 0.25},
        }
    )


def export_to_file(exported: dict, path) -> None:
    path.write_bytes(base64.b64decode(exported["model_base64"]))


def test_export_produces_a_whitelisted_model_with_pinned_manifest(tmp_path) -> None:
    artifact = trained_artifact()
    exported = export_linear_onnx({"artifact": artifact})

    assert exported["opset_version"] == EXPORT_OPSET
    assert set(exported["operators"]) <= SUPPORTED_OPERATORS
    manifest = exported["manifest"]
    assert manifest["model_id"] == artifact["model_id"]
    assert manifest["inputs"][0]["shape"] == [-1, 1]
    assert manifest["outputs"][0]["shape"] == [-1, 1]
    assert manifest["preprocessing"]["feature_names"] == ["temperature"]
    assert manifest["confidence"] == {"kind": "applicability_distance"}

    model_path = tmp_path / "exported.onnx"
    export_to_file(exported, model_path)
    assert model_path.stat().st_size > 0


def test_exported_model_is_numerically_consistent_with_source_model(tmp_path) -> None:
    artifact = trained_artifact()
    exported = export_linear_onnx({"artifact": artifact})
    model_path = tmp_path / "parity.onnx"
    export_to_file(exported, model_path)

    features = [[5.0], [25.0], [45.0], [75.0]]
    source = predict_linear_regression(artifact, features)
    onnx_result = predict_onnx(
        {
            "model_path": str(model_path),
            "model_sha256": exported["model_sha256"],
            "manifest": exported["manifest"],
            "features": features,
        }
    )

    exported_predictions = [row[0] for row in onnx_result["predictions"]]
    for expected, actual in zip(source["predictions"], exported_predictions):
        assert abs(expected - actual) <= 1e-4, (expected, actual)
    assert onnx_result["model_sha256"] == exported["model_sha256"]
    assert onnx_result["opset_version"] == EXPORT_OPSET
    assert onnx_result["confidence"] is not None


def test_export_rejects_invalid_artifacts() -> None:
    with pytest.raises(OnnxInferenceError) as excinfo:
        export_linear_onnx({"artifact": {"artifact_version": "bogus"}})
    assert excinfo.value.code == "invalid_artifact"

    artifact = trained_artifact()
    artifact["preprocessing"]["scales"] = [0.0]
    with pytest.raises(OnnxInferenceError) as excinfo:
        export_linear_onnx({"artifact": artifact})
    assert excinfo.value.code == "invalid_artifact"


def test_worker_dispatch_exposes_export_operation() -> None:
    artifact = trained_artifact()
    input_stream = io.BytesIO(
        encode_frame(
            {
                "jsonrpc": "2.0",
                "protocol_version": "1.0",
                "id": "hello-1",
                "method": "hello",
                "params": {},
            }
        )
        + encode_frame(
            {
                "jsonrpc": "2.0",
                "protocol_version": "1.0",
                "id": "export-1",
                "method": "submit",
                "params": {
                    "task_id": "job-export",
                    "operation": "export_linear_onnx",
                    "payload": {"artifact": artifact},
                },
            }
        )
        + encode_frame(
            {
                "jsonrpc": "2.0",
                "protocol_version": "1.0",
                "id": "shutdown-1",
                "method": "shutdown",
                "params": {},
            }
        )
    )
    output_stream = io.BytesIO()
    serve(input_stream, output_stream)

    messages = []
    output_stream.seek(0)
    while (message := read_frame(output_stream)) is not None:
        messages.append(message)

    assert "export_linear_onnx" in messages[0]["result"]["operations"]
    result = messages[-2]["result"]
    assert result["state"] == "completed"
    assert result["model_base64"]
    assert len(result["model_sha256"]) == 64
