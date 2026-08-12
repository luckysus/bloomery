use bloomery::compute::protocol::{encode_frame, WorkerRequest};
use bloomery::compute::worker::{read_response, WorkerClient, WorkerConfig};
use serde_json::json;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn supervisor_rejects_a_missing_worker_before_spawning() {
    let path = PathBuf::from(format!("missing-bloomery-worker-{}", uuid::Uuid::new_v4()));
    let error =
        WorkerClient::spawn(WorkerConfig::new(path)).expect_err("missing worker must be rejected");
    assert!(error
        .to_string()
        .contains("worker executable does not exist"));
}

#[test]
fn supervisor_skips_progress_notifications_until_matching_response() {
    let mut bytes = encode_frame(&json!({
        "jsonrpc": "2.0",
        "protocol_version": "1.0",
        "method": "progress",
        "params": {"task_id": "job-1", "progress": 50, "stage": "training"}
    }))
    .expect("encode progress");
    bytes.extend_from_slice(
        &encode_frame(&json!({
            "jsonrpc": "2.0",
            "protocol_version": "1.0",
            "id": "run-1",
            "result": {"task_id": "job-1", "state": "completed"}
        }))
        .expect("encode response"),
    );

    let response = read_response(&mut Cursor::new(bytes), "run-1").expect("read response");
    assert_eq!(response["result"]["state"], "completed");
}

#[test]
fn supervisor_round_trips_a_real_python_training_worker() {
    let python = Command::new("where.exe")
        .arg("python")
        .output()
        .expect("Windows Python lookup must be available");
    assert!(
        python.status.success(),
        "python must be installed for the worker contract"
    );
    let executable = String::from_utf8_lossy(&python.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .expect("Python lookup must return an executable");

    let working_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("compute-worker");
    let mut config = WorkerConfig::new(executable);
    config.args = vec!["-m".into(), "bloomery_worker".into()];
    config.working_directory = Some(working_directory);
    let mut client = WorkerClient::spawn(config).expect("spawn Python compute worker");

    let hello = client
        .request(&WorkerRequest::new("hello-1", "hello", json!({})))
        .expect("worker hello");
    assert_eq!(hello["result"]["protocol_version"], "1.0");
    assert!(hello["result"]["operations"]
        .as_array()
        .expect("worker operations")
        .iter()
        .any(|operation| operation == "train_linear_regression"));

    let trained = client
        .request(&WorkerRequest::new(
            "train-1",
            "submit",
            json!({
                "task_id": "job-1",
                "operation": "train_linear_regression",
                "payload": {
                    "features": [[0], [1], [2], [3]],
                    "targets": [1, 3, 5, 7],
                    "feature_names": ["temperature"],
                    "split_policy": {"kind": "time", "validation_fraction": 0.25}
                }
            }),
        ))
        .expect("worker training");
    assert_eq!(trained["result"]["state"], "completed");
    assert_eq!(
        trained["result"]["artifact"]["model_type"],
        "linear_regression"
    );
    assert_eq!(
        trained["result"]["artifact"]["feature_names"],
        json!(["temperature"])
    );

    let shutdown = client
        .shutdown(&WorkerRequest::new("shutdown-1", "shutdown", json!({})))
        .expect("worker shutdown");
    assert_eq!(shutdown["result"]["state"], "stopped");
}

#[test]
fn supervisor_terminates_a_blocked_worker_when_cancellation_is_requested() {
    let python = Command::new("where.exe")
        .arg("python")
        .output()
        .expect("Windows Python lookup must be available");
    assert!(python.status.success());
    let executable = String::from_utf8_lossy(&python.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .expect("Python lookup must return an executable");
    let script = r#"
import sys
import time

header = bytearray()
while not header.endswith(b"\r\n\r\n"):
    byte = sys.stdin.buffer.read(1)
    if not byte:
        raise SystemExit(0)
    header.extend(byte)
length = int(next(line for line in header.decode("ascii").split("\r\n") if line.startswith("Content-Length:")).split(":", 1)[1])
sys.stdin.buffer.read(length)
time.sleep(30)
"#;
    let mut config = WorkerConfig::new(executable);
    config.args = vec!["-c".into(), script.into()];
    let mut client = WorkerClient::spawn(config).expect("spawn blocked worker fixture");
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancelled);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        signal.store(true, Ordering::SeqCst);
    });

    let started = Instant::now();
    let error = client
        .request_with_progress_and_cancel(
            &WorkerRequest::new("blocked-1", "submit", json!({"payload": {}})),
            |_| Ok(()),
            move || cancelled.load(Ordering::SeqCst),
        )
        .expect_err("cancellation must terminate a blocked worker request");

    assert!(matches!(
        error,
        bloomery::compute::worker::WorkerSupervisorError::Cancelled
    ));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "blocked worker cancellation took too long"
    );
}

#[test]
fn supervisor_rejects_a_response_with_the_wrong_protocol_envelope() {
    let bytes = encode_frame(&json!({
        "jsonrpc": "1.0",
        "protocol_version": "0.9",
        "id": "run-1",
        "result": {"state": "completed"}
    }))
    .expect("encode response");

    let error = read_response(&mut std::io::Cursor::new(bytes), "run-1")
        .expect_err("wrong worker protocol envelope must be rejected");
    assert!(
        error.to_string().contains("protocol"),
        "unexpected error: {error}"
    );
}

#[test]
fn supervisor_kills_a_worker_that_does_not_exit_after_shutdown() {
    let python = Command::new("where.exe")
        .arg("python")
        .output()
        .expect("Windows Python lookup must be available");
    assert!(python.status.success());
    let executable = String::from_utf8_lossy(&python.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .expect("Python lookup must return an executable");
    let script = r#"
import json
import sys
import time

header = bytearray()
while not header.endswith(b"\r\n\r\n"):
    byte = sys.stdin.buffer.read(1)
    if not byte:
        raise SystemExit(0)
    header.extend(byte)
length = int(next(line for line in header.decode("ascii").split("\r\n") if line.startswith("Content-Length:")).split(":", 1)[1])
body = json.loads(sys.stdin.buffer.read(length))
response = {
    "jsonrpc": "2.0",
    "protocol_version": "1.0",
    "id": body["id"],
    "result": {"state": "stopped"},
}
encoded = json.dumps(response, separators=(",", ":")).encode("utf-8")
sys.stdout.buffer.write(f"Content-Length: {len(encoded)}\r\n\r\n".encode("ascii") + encoded)
sys.stdout.buffer.flush()
time.sleep(30)
"#;
    let mut config = WorkerConfig::new(executable);
    config.args = vec!["-c".into(), script.into()];
    let client = WorkerClient::spawn(config).expect("spawn non-terminating worker fixture");

    let started = Instant::now();
    let error = client
        .shutdown_with_timeout(
            &WorkerRequest::new("shutdown-timeout-1", "shutdown", json!({})),
            Duration::from_millis(250),
        )
        .expect_err("shutdown must fail closed when the worker ignores process exit");

    assert!(
        error.to_string().contains("timed out"),
        "unexpected error: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "worker shutdown timeout took too long"
    );
}

#[test]
fn supervisor_times_out_when_shutdown_receives_no_response() {
    let python = Command::new("where.exe")
        .arg("python")
        .output()
        .expect("Windows Python lookup must be available");
    assert!(python.status.success());
    let executable = String::from_utf8_lossy(&python.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .expect("Python lookup must return an executable");
    let script = r#"
import sys
import time

header = bytearray()
while not header.endswith(b"\r\n\r\n"):
    byte = sys.stdin.buffer.read(1)
    if not byte:
        raise SystemExit(0)
    header.extend(byte)
length = int(next(line for line in header.decode("ascii").split("\r\n") if line.startswith("Content-Length:")).split(":", 1)[1])
sys.stdin.buffer.read(length)
time.sleep(2)
"#;
    let mut config = WorkerConfig::new(executable);
    config.args = vec!["-c".into(), script.into()];
    let client = WorkerClient::spawn(config).expect("spawn unresponsive worker fixture");

    let started = Instant::now();
    let error = client
        .shutdown_with_timeout(
            &WorkerRequest::new("shutdown-no-response-1", "shutdown", json!({})),
            Duration::from_millis(250),
        )
        .expect_err("shutdown without a response must time out");

    assert!(
        error.to_string().contains("timed out"),
        "unexpected error: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "shutdown response timeout took too long"
    );
}
