import io
import json
import os
import subprocess
from pathlib import Path

import pytest

from bloomery_worker.protocol import encode_frame, read_frame

DIST_ROOT = Path(__file__).resolve().parents[1] / "dist"
EXECUTABLE = DIST_ROOT / "bloomery-compute-worker.exe"
MANIFEST = DIST_ROOT / "worker-artifact-manifest.json"


def frame(request_id: str, method: str, params: dict) -> bytes:
    return encode_frame(
        {
            "jsonrpc": "2.0",
            "protocol_version": "1.0",
            "id": request_id,
            "method": method,
            "params": params,
        }
    )


@pytest.mark.skipif(not EXECUTABLE.exists(), reason="packaged worker not built")
def test_packaged_worker_runs_hello_and_shutdown_without_system_python() -> None:
    environment = {
        key: value
        for key, value in os.environ.items()
        if key.lower() not in {"virtual_env", "pythonhome", "pythonpath"}
    }
    process = subprocess.Popen(
        [str(EXECUTABLE)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        env=environment,
    )
    stdin = process.stdin
    assert stdin is not None
    stdin.write(frame("hello-1", "hello", {}) + frame("shutdown-1", "shutdown", {}))
    stdin.flush()
    stdout = process.stdout
    assert stdout is not None
    messages = []
    while (message := read_frame(stdout)) is not None:
        messages.append(message)
    process.wait(timeout=60)

    assert messages[0]["id"] == "hello-1"
    assert "train_sklearn_model" in messages[0]["result"]["operations"]
    assert messages[1]["id"] == "shutdown-1"
    assert messages[1]["result"] == {"state": "stopped"}
    assert process.returncode == 0


@pytest.mark.skipif(not MANIFEST.exists(), reason="packaged worker manifest not built")
def test_artifact_manifest_records_versions_hash_and_unsigned_marker() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8-sig"))
    assert manifest["schema_version"] == "1.0.0"
    assert manifest["artifact"] == "bloomery-compute-worker"
    assert len(manifest["sha256"]) == 64
    assert manifest["signature"] == "unsigned-explicit"
    assert manifest["private_urls"] == []
    assert manifest["python"]
    names = [component["name"] for component in manifest["packages"]]
    assert "onnxruntime" in names
    assert "scikit-learn" in names
    checksum = (DIST_ROOT / "bloomery-compute-worker.sha256").read_text(encoding="utf-8-sig")
    assert checksum.strip().startswith(manifest["sha256"])
