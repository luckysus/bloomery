use bloomery::compute::protocol::{encode_frame, WorkerRequest};
use bloomery::compute::worker::{read_response, WorkerClient, WorkerConfig};
use serde_json::json;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

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
