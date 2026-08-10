use bloomery::compute::handler::{
    ComputeOnnxPredictionTaskHandler, ComputeOptimizationTaskHandler, ComputePredictionTaskHandler,
    ComputeTaskHandler, COMPUTE_OPTIMIZE_CONSTRAINED_KIND, COMPUTE_PREDICT_LINEAR_REGRESSION_KIND,
    COMPUTE_PREDICT_ONNX_KIND, COMPUTE_TRAIN_LINEAR_REGRESSION_KIND,
};
use bloomery::compute::worker::{WorkerClient, WorkerConfig};
use bloomery::storage::migrations::migrate;
use bloomery::tasks::model::{NewTask, TaskState};
use bloomery::tasks::repository;
use bloomery::tasks::scheduler::{
    EventSink, Scheduler, SchedulerConfig, SchedulerEvent, SystemClock,
};
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<SchedulerEvent>>,
}

impl EventSink for RecordingSink {
    fn emit(&self, event: SchedulerEvent) {
        self.events.lock().expect("event lock").push(event);
    }
}

fn python_worker_config() -> WorkerConfig {
    let python = std::process::Command::new("where.exe")
        .arg("python")
        .output()
        .expect("Windows Python lookup must be available");
    assert!(python.status.success(), "python must be installed");
    let executable = String::from_utf8_lossy(&python.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .expect("Python lookup must return an executable");
    let mut config = WorkerConfig::new(executable);
    config.args = vec!["-m".into(), "bloomery_worker".into()];
    config.working_directory = Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("compute-worker"),
    );
    config
}

fn onnx_model_path_and_hash() -> (PathBuf, String) {
    let output = std::process::Command::new("python")
        .args([
            "-c",
            "from onnxruntime.datasets import get_example; print(get_example('mul_1.onnx'))",
        ])
        .output()
        .expect("Python ONNX Runtime lookup must be available");
    assert!(
        output.status.success(),
        "Python ONNX Runtime lookup must succeed"
    );
    let path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .expect("ONNX Runtime lookup must return a model path");
    let bytes = std::fs::read(&path).expect("read ONNX fixture");
    let mut digest = Sha256::new();
    digest.update(bytes);
    (path, format!("{:x}", digest.finalize()))
}

#[test]
fn scheduler_runs_training_and_persists_a_queryable_result() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-compute-task-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let mut connection = Connection::open(&path).expect("open task database");
    migrate(&mut connection).expect("migrate task database");
    let task = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: COMPUTE_TRAIN_LINEAR_REGRESSION_KIND.to_string(),
            payload_json: serde_json::to_string(&json!({
                "operation": "train_linear_regression",
                "payload": {
                    "features": [[0], [1], [2], [3]],
                    "targets": [1, 3, 5, 7],
                    "feature_names": ["temperature"],
                    "split_policy": {"kind": "time", "validation_fraction": 0.25}
                }
            }))
            .expect("encode task payload"),
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create training task");
    drop(connection);

    let sink = Arc::new(RecordingSink::default());
    let mut scheduler = Scheduler::new(
        path.clone(),
        "local".to_string(),
        SchedulerConfig {
            max_workers: 1,
            max_attempts: 1,
            retry_base: Duration::from_millis(1),
            retry_max: Duration::from_millis(1),
            poll_interval: Duration::from_millis(1),
        },
        Arc::new(SystemClock),
        vec![Arc::new(ComputeTaskHandler::new(python_worker_config()))],
        sink.clone(),
    )
    .expect("create compute scheduler");

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        scheduler.tick().expect("run scheduler tick");
        let connection = Connection::open(&path).expect("open task database");
        let current = repository::get(&connection, "local", task.id)
            .expect("read task")
            .expect("task exists");
        if matches!(current.state, TaskState::Completed | TaskState::Failed) {
            assert_eq!(
                current.state,
                TaskState::Completed,
                "training task failed: {current:?}"
            );
            let checkpoint: serde_json::Value =
                serde_json::from_str(current.checkpoint_json.as_deref().expect("checkpoint"))
                    .expect("decode checkpoint");
            assert_eq!(checkpoint["stage"], "completed");
            assert_eq!(checkpoint["result"]["state"], "completed");
            assert_eq!(
                checkpoint["result"]["artifact"]["model_type"],
                "linear_regression"
            );
            break;
        }
        assert!(Instant::now() < deadline, "training task timed out");
        thread::yield_now();
    }

    let events = sink.events.lock().expect("event lock");
    assert!(!events.is_empty(), "training must emit progress");
    for event in events.iter() {
        let encoded = serde_json::to_string(event).expect("encode progress event");
        assert!(!encoded.contains("payload_json"));
        assert!(!encoded.contains("features"));
        assert!(!encoded.contains("checkpoint_json"));
    }

    let _ = WorkerClient::spawn;
    drop(scheduler);
    let _ = std::fs::remove_file(path);
}

#[test]
fn scheduler_runs_prediction_and_records_applicability_metadata() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-predict-task-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let mut connection = Connection::open(&path).expect("open task database");
    migrate(&mut connection).expect("migrate task database");
    let task = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: COMPUTE_PREDICT_LINEAR_REGRESSION_KIND.to_string(),
            payload_json: serde_json::to_string(&json!({
                "operation": "predict_linear_regression",
                "payload": {
                    "dataset_id": "dataset-1",
                    "training_task_id": "training-1",
                    "artifact": {
                        "artifact_version": "linear-regression.v1",
                        "model_id": "model-1",
                        "model_type": "linear_regression",
                        "feature_names": ["temperature"],
                        "preprocessing": {"means": [25.0], "scales": [10.0]},
                        "coefficients": [2.0],
                        "intercept": 5.0,
                        "applicability_range": [{"min": 10.0, "max": 40.0}]
                    },
                    "features": [[125.0]]
                }
            }))
            .expect("encode prediction payload"),
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create prediction task");
    drop(connection);

    let sink = Arc::new(RecordingSink::default());
    let mut scheduler = Scheduler::new(
        path.clone(),
        "local".to_string(),
        SchedulerConfig {
            max_workers: 1,
            max_attempts: 1,
            retry_base: Duration::from_millis(1),
            retry_max: Duration::from_millis(1),
            poll_interval: Duration::from_millis(1),
        },
        Arc::new(SystemClock),
        vec![Arc::new(ComputePredictionTaskHandler::from_optional(Some(
            python_worker_config(),
        )))],
        sink.clone(),
    )
    .expect("create prediction scheduler");

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        scheduler.tick().expect("run scheduler tick");
        let connection = Connection::open(&path).expect("open task database");
        let current = repository::get(&connection, "local", task.id)
            .expect("read task")
            .expect("task exists");
        if matches!(current.state, TaskState::Completed | TaskState::Failed) {
            assert_eq!(
                current.state,
                TaskState::Completed,
                "prediction task failed: {current:?}"
            );
            let checkpoint: serde_json::Value =
                serde_json::from_str(current.checkpoint_json.as_deref().expect("checkpoint"))
                    .expect("decode checkpoint");
            assert_eq!(checkpoint["result"]["state"], "completed");
            assert_eq!(checkpoint["result"]["model_id"], "model-1");
            assert_eq!(checkpoint["result"]["predictions"], json!([25.0]));
            assert_eq!(checkpoint["result"]["input_values"], json!([125.0]));
            assert_eq!(
                checkpoint["result"]["applicability_warnings"][0]["feature"],
                "temperature"
            );
            break;
        }
        assert!(Instant::now() < deadline, "prediction task timed out");
        thread::yield_now();
    }

    let events = sink.events.lock().expect("event lock");
    assert!(!events.is_empty(), "prediction must emit progress");
    for event in events.iter() {
        let encoded = serde_json::to_string(event).expect("encode progress event");
        assert!(!encoded.contains("payload_json"));
        assert!(!encoded.contains("coefficients"));
    }

    drop(scheduler);
    let _ = std::fs::remove_file(path);
}

#[test]
fn scheduler_runs_optimization_and_enforces_constraints() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-optimize-task-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let mut connection = Connection::open(&path).expect("open task database");
    migrate(&mut connection).expect("migrate task database");
    let task = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: COMPUTE_OPTIMIZE_CONSTRAINED_KIND.to_string(),
            payload_json: serde_json::to_string(&json!({
                "operation": "optimize_constrained",
                "payload": {
                    "dataset_id": "dataset-1",
                    "training_task_id": "training-1",
                    "artifact": {
                        "artifact_version": "linear-regression.v1",
                        "model_id": "model-1",
                        "model_type": "linear_regression",
                        "feature_names": ["temperature"],
                        "preprocessing": {"means": [0.0], "scales": [1.0]},
                        "coefficients": [2.0],
                        "intercept": 0.0,
                        "applicability_range": [{"min": 0.0, "max": 10.0}]
                    },
                    "direction": "minimize",
                    "objectives": ["temperature"],
                    "bounds": [{"min": 0.0, "max": 10.0}],
                    "fixed_values": {},
                    "constraints": [{
                        "kind": "inequality",
                        "coefficients": {"temperature": 1.0},
                        "value": 4.0,
                        "tolerance": 0.0
                    }],
                    "trials": 48,
                    "seed": 7
                }
            }))
            .expect("encode optimization payload"),
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create optimization task");
    drop(connection);

    let sink = Arc::new(RecordingSink::default());
    let mut scheduler = Scheduler::new(
        path.clone(),
        "local".to_string(),
        SchedulerConfig {
            max_workers: 1,
            max_attempts: 1,
            retry_base: Duration::from_millis(1),
            retry_max: Duration::from_millis(1),
            poll_interval: Duration::from_millis(1),
        },
        Arc::new(SystemClock),
        vec![Arc::new(ComputeOptimizationTaskHandler::from_optional(
            Some(python_worker_config()),
        ))],
        sink.clone(),
    )
    .expect("create optimization scheduler");

    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        scheduler.tick().expect("run optimization scheduler tick");
        let connection = Connection::open(&path).expect("open task database");
        let current = repository::get(&connection, "local", task.id)
            .expect("read task")
            .expect("task exists");
        if matches!(current.state, TaskState::Completed | TaskState::Failed) {
            assert_eq!(
                current.state,
                TaskState::Completed,
                "optimization task failed: {current:?}"
            );
            let checkpoint: serde_json::Value =
                serde_json::from_str(current.checkpoint_json.as_deref().expect("checkpoint"))
                    .expect("decode checkpoint");
            let result = &checkpoint["result"];
            assert_eq!(result["state"], "completed");
            assert_eq!(result["model_id"], "model-1");
            assert_eq!(result["method"], "tpe");
            assert_eq!(result["deterministic_seed"], json!(7));
            let recommendation = &result["recommendations"][0];
            assert_eq!(recommendation["feasible"], json!(true));
            let temperature = recommendation["values"]["temperature"]
                .as_f64()
                .expect("temperature value");
            assert!(
                temperature >= 4.0 - 1e-9,
                "recommendation must satisfy the hard constraint, got {temperature}"
            );
            assert!(temperature <= 10.0);
            break;
        }
        assert!(Instant::now() < deadline, "optimization task timed out");
        thread::yield_now();
    }

    let events = sink.events.lock().expect("event lock");
    assert!(!events.is_empty(), "optimization must emit progress");
    for event in events.iter() {
        let encoded = serde_json::to_string(event).expect("encode progress event");
        assert!(!encoded.contains("payload_json"));
        assert!(!encoded.contains("coefficients"));
    }

    drop(scheduler);
    let _ = std::fs::remove_file(path);
}

#[test]
fn scheduler_runs_onnx_prediction_and_persists_model_provenance() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-onnx-task-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let (model_path, model_sha256) = onnx_model_path_and_hash();
    let mut connection = Connection::open(&path).expect("open task database");
    migrate(&mut connection).expect("migrate task database");
    let task = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: COMPUTE_PREDICT_ONNX_KIND.to_string(),
            payload_json: serde_json::to_string(&json!({
                "operation": "predict_onnx",
                "payload": {
                    "model_path": model_path,
                    "model_sha256": model_sha256,
                    "manifest": {
                        "model_id": "mul-model",
                        "model_version": "1.0.0",
                        "inputs": [{"name": "X", "dtype": "float32", "shape": [-1, 2]}],
                        "outputs": [{"name": "Y", "dtype": "float32", "shape": [-1, 2]}],
                        "preprocessing": {
                            "feature_names": ["temperature", "carbon"],
                            "means": [0.0, 0.0],
                            "scales": [1.0, 1.0]
                        },
                        "applicability_range": [
                            {"min": 0.0, "max": 10.0},
                            {"min": 0.0, "max": 10.0}
                        ]
                    },
                    "features": [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]
                }
            }))
            .expect("encode ONNX task payload"),
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create ONNX task");
    drop(connection);

    let sink = Arc::new(RecordingSink::default());
    let mut scheduler = Scheduler::new(
        path.clone(),
        "local".to_string(),
        SchedulerConfig {
            max_workers: 1,
            max_attempts: 1,
            retry_base: Duration::from_millis(1),
            retry_max: Duration::from_millis(1),
            poll_interval: Duration::from_millis(1),
        },
        Arc::new(SystemClock),
        vec![Arc::new(ComputeOnnxPredictionTaskHandler::from_optional(
            Some(python_worker_config()),
        ))],
        sink.clone(),
    )
    .expect("create ONNX scheduler");

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        scheduler.tick().expect("run ONNX scheduler tick");
        let connection = Connection::open(&path).expect("open task database");
        let current = repository::get(&connection, "local", task.id)
            .expect("read task")
            .expect("task exists");
        if matches!(current.state, TaskState::Completed | TaskState::Failed) {
            assert_eq!(
                current.state,
                TaskState::Completed,
                "ONNX task failed: {current:?}"
            );
            let checkpoint: serde_json::Value =
                serde_json::from_str(current.checkpoint_json.as_deref().expect("checkpoint"))
                    .expect("decode checkpoint");
            assert_eq!(checkpoint["result"]["state"], "completed");
            assert_eq!(checkpoint["result"]["model_id"], "mul-model");
            assert_eq!(
                checkpoint["result"]["predictions"],
                json!([[1.0, 4.0], [9.0, 16.0], [25.0, 36.0]])
            );
            assert_eq!(checkpoint["result"]["model_sha256"], model_sha256);
            break;
        }
        assert!(Instant::now() < deadline, "ONNX task timed out");
        thread::yield_now();
    }

    let events = sink.events.lock().expect("event lock");
    assert!(!events.is_empty(), "ONNX task must emit progress");
    for event in events.iter() {
        let encoded = serde_json::to_string(event).expect("encode progress event");
        assert!(!encoded.contains("model_path"));
        assert!(!encoded.contains("model_sha256"));
    }

    drop(scheduler);
    let _ = std::fs::remove_file(path);
}
