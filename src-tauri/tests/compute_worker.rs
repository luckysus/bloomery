use bloomery::compute::protocol::encode_frame;
use bloomery::compute::worker::{read_response, WorkerClient, WorkerConfig};
use serde_json::json;
use std::io::Cursor;
use std::path::PathBuf;

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
