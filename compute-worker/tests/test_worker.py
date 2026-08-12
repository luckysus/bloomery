import io

from bloomery_worker.protocol import encode_frame, read_frame
from bloomery_worker.worker import serve


def request(request_id: str, method: str, params: dict) -> bytes:
    return encode_frame(
        {
            "jsonrpc": "2.0",
            "protocol_version": "1.0",
            "id": request_id,
            "method": method,
            "params": params,
        }
    )


def read_all(stream: io.BytesIO) -> list[dict]:
    stream.seek(0)
    messages = []
    while (message := read_frame(stream)) is not None:
        messages.append(message)
    return messages


def test_worker_reports_capabilities_and_shuts_down_cleanly() -> None:
    input_stream = io.BytesIO(
        request("hello-1", "hello", {}) + request("shutdown-1", "shutdown", {})
    )
    output_stream = io.BytesIO()

    serve(input_stream, output_stream)
    messages = read_all(output_stream)

    assert messages[0]["id"] == "hello-1"
    assert messages[0]["result"]["protocol_version"] == "1.0"
    assert "submit" in messages[0]["result"]["capabilities"]
    assert "train_linear_regression" in messages[0]["result"]["operations"]
    assert messages[1]["id"] == "shutdown-1"
    assert messages[1]["result"] == {"state": "stopped"}


def test_echo_task_emits_progress_then_a_result() -> None:
    input_stream = io.BytesIO(
        request(
            "run-1",
            "submit",
            {"task_id": "job-1", "operation": "echo", "payload": {"value": "钢"}},
        )
        + request("shutdown-1", "shutdown", {})
    )
    output_stream = io.BytesIO()

    serve(input_stream, output_stream)
    messages = read_all(output_stream)

    assert messages[0]["method"] == "progress"
    assert messages[0]["params"] == {
        "task_id": "job-1",
        "progress": 100,
        "stage": "completed",
    }
    assert messages[1]["id"] == "run-1"
    assert messages[1]["result"] == {
        "task_id": "job-1",
        "state": "completed",
        "value": {"value": "钢"},
    }


def test_cancel_returns_a_terminal_cancelled_state() -> None:
    input_stream = io.BytesIO(
        request("cancel-1", "cancel", {"task_id": "job-1"})
        + request("shutdown-1", "shutdown", {})
    )
    output_stream = io.BytesIO()

    serve(input_stream, output_stream)
    messages = read_all(output_stream)

    assert messages[0]["id"] == "cancel-1"
    assert messages[0]["result"] == {"task_id": "job-1", "state": "cancelled"}


def test_linear_training_emits_progress_and_returns_a_model_artifact() -> None:
    input_stream = io.BytesIO(
        request(
            "train-1",
            "submit",
            {
                "task_id": "job-1",
                "operation": "train_linear_regression",
                "payload": {
                    "features": [[0], [1], [2], [3]],
                    "targets": [1, 3, 5, 7],
                    "feature_names": ["temperature"],
                    "split_policy": {"kind": "time", "validation_fraction": 0.25},
                },
            },
        )
        + request("shutdown-1", "shutdown", {})
    )
    output_stream = io.BytesIO()

    serve(input_stream, output_stream)
    messages = read_all(output_stream)

    assert messages[0]["method"] == "progress"
    assert messages[0]["params"]["stage"] == "training"
    assert messages[1]["method"] == "progress"
    assert messages[1]["params"]["stage"] == "completed"
    assert messages[2]["id"] == "train-1"
    assert messages[2]["result"]["state"] == "completed"
    assert messages[2]["result"]["artifact"]["model_type"] == "linear_regression"
    assert messages[2]["result"]["artifact"]["feature_names"] == ["temperature"]


def test_worker_rejects_unsupported_operations_and_unknown_methods() -> None:
    input_stream = io.BytesIO(
        request(
            "run-1",
            "submit",
            {"task_id": "job-1", "operation": "train", "payload": {}},
        )
        + request("unknown-1", "not-a-method", {})
        + request("shutdown-1", "shutdown", {})
    )
    output_stream = io.BytesIO()

    serve(input_stream, output_stream)
    messages = read_all(output_stream)

    assert messages[0]["id"] == "run-1"
    assert messages[0]["error"]["code"] == "unsupported_operation"
    assert messages[1]["id"] == "unknown-1"
    assert messages[1]["error"]["code"] == "method_not_found"


def test_worker_rejects_non_string_operations_without_crashing() -> None:
    input_stream = io.BytesIO(
        request(
            "run-1",
            "submit",
            {"task_id": "job-1", "operation": [], "payload": {}},
        )
        + request("shutdown-1", "shutdown", {})
    )
    output_stream = io.BytesIO()

    serve(input_stream, output_stream)
    messages = read_all(output_stream)

    assert messages[0]["id"] == "run-1"
    assert messages[0]["error"]["code"] == "invalid_params"
