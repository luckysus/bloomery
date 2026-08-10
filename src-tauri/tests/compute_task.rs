use bloomery::compute::handler::{
    ComputeExportOnnxTaskHandler, ComputeOnnxPredictionTaskHandler, ComputeOptimizationTaskHandler,
    ComputePredictionTaskHandler, ComputeSklearnTrainingTaskHandler, ComputeTaskHandler,
    ComputeTrainedPredictionTaskHandler, COMPUTE_EXPORT_ONNX_KIND,
    COMPUTE_OPTIMIZE_CONSTRAINED_KIND, COMPUTE_PREDICT_LINEAR_REGRESSION_KIND,
    COMPUTE_PREDICT_ONNX_KIND, COMPUTE_PREDICT_TRAINED_KIND, COMPUTE_TRAIN_LINEAR_REGRESSION_KIND,
    COMPUTE_TRAIN_SKLEARN_KIND,
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

fn decode_base64(input: &str) -> Vec<u8> {
    fn val(c: u8) -> u32 {
        match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("invalid base64 byte"),
        }
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|c| **c == b'=').count();
        let vals: Vec<u32> = chunk
            .iter()
            .map(|c| if *c == b'=' { 0 } else { val(*c) })
            .collect();
        let triple = (vals[0] << 18) | (vals[1] << 12) | (vals[2] << 6) | vals[3];
        out.push((triple >> 16) as u8);
        if pad < 2 {
            out.push((triple >> 8) as u8);
        }
        if pad < 1 {
            out.push(triple as u8);
        }
    }
    out
}

#[test]
fn scheduler_trains_sklearn_model_and_predicts_through_trained_path() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-sklearn-task-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let mut connection = Connection::open(&path).expect("open task database");
    migrate(&mut connection).expect("migrate task database");
    let features: Vec<Vec<f64>> = (0..40)
        .map(|i| vec![f64::from(i), f64::from(i % 5)])
        .collect();
    let targets: Vec<f64> = features.iter().map(|row| 2.0 * row[0] + row[1]).collect();
    let task = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: COMPUTE_TRAIN_SKLEARN_KIND.to_string(),
            payload_json: serde_json::to_string(&json!({
                "operation": "train_sklearn_model",
                "payload": {
                    "dataset_id": "dataset-1",
                    "algorithm": "random_forest",
                    "n_estimators": 20,
                    "seed": 7,
                    "features": features,
                    "targets": targets,
                    "feature_names": ["temperature", "carbon"],
                    "split_policy": {"kind": "random", "validation_fraction": 0.25, "seed": 7}
                }
            }))
            .expect("encode sklearn training payload"),
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create sklearn training task");
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
        vec![Arc::new(ComputeSklearnTrainingTaskHandler::from_optional(
            Some(python_worker_config()),
        ))],
        sink.clone(),
    )
    .expect("create sklearn training scheduler");

    let artifact = {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            scheduler.tick().expect("run sklearn training tick");
            let connection = Connection::open(&path).expect("open task database");
            let current = repository::get(&connection, "local", task.id)
                .expect("read task")
                .expect("task exists");
            if matches!(current.state, TaskState::Completed | TaskState::Failed) {
                assert_eq!(
                    current.state,
                    TaskState::Completed,
                    "sklearn training failed: {current:?}"
                );
                let checkpoint: serde_json::Value =
                    serde_json::from_str(current.checkpoint_json.as_deref().expect("checkpoint"))
                        .expect("decode checkpoint");
                break checkpoint["result"]["artifact"].clone();
            }
            assert!(Instant::now() < deadline, "sklearn training timed out");
            thread::yield_now();
        }
    };
    drop(scheduler);
    assert_eq!(artifact["model_type"], "random_forest");
    assert_eq!(artifact["artifact_version"], "sklearn-pickle.v1");

    let mut connection = Connection::open(&path).expect("reopen task database");
    let predict_task = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: COMPUTE_PREDICT_TRAINED_KIND.to_string(),
            payload_json: serde_json::to_string(&json!({
                "operation": "predict_trained_model",
                "payload": {
                    "dataset_id": "dataset-1",
                    "training_task_id": task.id.to_string(),
                    "artifact": artifact,
                    "features": [[10.0, 2.0]]
                }
            }))
            .expect("encode trained prediction payload"),
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create trained prediction task");
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
        vec![Arc::new(
            ComputeTrainedPredictionTaskHandler::from_optional(Some(python_worker_config())),
        )],
        sink.clone(),
    )
    .expect("create trained prediction scheduler");

    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        scheduler.tick().expect("run trained prediction tick");
        let connection = Connection::open(&path).expect("open task database");
        let current = repository::get(&connection, "local", predict_task.id)
            .expect("read task")
            .expect("task exists");
        if matches!(current.state, TaskState::Completed | TaskState::Failed) {
            assert_eq!(
                current.state,
                TaskState::Completed,
                "trained prediction failed: {current:?}"
            );
            let checkpoint: serde_json::Value =
                serde_json::from_str(current.checkpoint_json.as_deref().expect("checkpoint"))
                    .expect("decode checkpoint");
            let prediction = checkpoint["result"]["predictions"][0]
                .as_f64()
                .expect("trained prediction value");
            assert!(prediction.is_finite());
            // The random forest must track the synthetic 2a+b surface.
            assert!((prediction - 22.0).abs() < 6.0, "prediction {prediction}");
            break;
        }
        assert!(Instant::now() < deadline, "trained prediction timed out");
        thread::yield_now();
    }

    drop(scheduler);
    let _ = std::fs::remove_file(path);
}

#[test]
fn scheduler_exports_onnx_and_imported_model_matches_source_predictions() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-export-task-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let mut connection = Connection::open(&path).expect("open task database");
    migrate(&mut connection).expect("migrate task database");
    let artifact = json!({
        "artifact_version": "linear-regression.v1",
        "model_id": "model-export",
        "model_type": "linear_regression",
        "feature_names": ["temperature"],
        "preprocessing": {"means": [25.0], "scales": [10.0]},
        "coefficients": [2.0],
        "intercept": 5.0,
        "applicability_range": [{"min": 0.0, "max": 100.0}]
    });
    let task = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: COMPUTE_EXPORT_ONNX_KIND.to_string(),
            payload_json: serde_json::to_string(&json!({
                "operation": "export_linear_onnx",
                "payload": {
                    "dataset_id": "dataset-1",
                    "training_task_id": "training-1",
                    "artifact": artifact,
                    "model_version": "1.0.0"
                }
            }))
            .expect("encode export payload"),
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create export task");
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
        vec![Arc::new(ComputeExportOnnxTaskHandler::from_optional(Some(
            python_worker_config(),
        )))],
        sink.clone(),
    )
    .expect("create export scheduler");

    let exported = {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            scheduler.tick().expect("run export scheduler tick");
            let connection = Connection::open(&path).expect("open task database");
            let current = repository::get(&connection, "local", task.id)
                .expect("read task")
                .expect("task exists");
            if matches!(current.state, TaskState::Completed | TaskState::Failed) {
                assert_eq!(
                    current.state,
                    TaskState::Completed,
                    "export task failed: {current:?}"
                );
                let checkpoint: serde_json::Value =
                    serde_json::from_str(current.checkpoint_json.as_deref().expect("checkpoint"))
                        .expect("decode checkpoint");
                break checkpoint["result"].clone();
            }
            assert!(Instant::now() < deadline, "export task timed out");
            thread::yield_now();
        }
    };
    drop(scheduler);

    assert_eq!(exported["manifest"]["model_id"], "model-export");
    let model_base64 = exported["model_base64"].as_str().expect("model base64");
    let model_bytes = decode_base64(model_base64);
    let mut digest = Sha256::new();
    digest.update(&model_bytes);
    assert_eq!(
        format!("{:x}", digest.finalize()),
        exported["model_sha256"].as_str().expect("model sha256")
    );
    let model_path =
        std::env::temp_dir().join(format!("bloomery-exported-{}.onnx", uuid::Uuid::new_v4()));
    std::fs::write(&model_path, &model_bytes).expect("write exported model");

    // Import the exported model through the ONNX prediction pipeline and
    // require numeric parity with the source linear artifact.
    let mut connection = Connection::open(&path).expect("reopen task database");
    let predict_task = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: COMPUTE_PREDICT_ONNX_KIND.to_string(),
            payload_json: serde_json::to_string(&json!({
                "operation": "predict_onnx",
                "payload": {
                    "model_path": model_path.to_string_lossy(),
                    "model_sha256": exported["model_sha256"],
                    "manifest": exported["manifest"],
                    "features": [[125.0]]
                }
            }))
            .expect("encode parity payload"),
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create parity task");
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
    .expect("create parity scheduler");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        scheduler.tick().expect("run parity scheduler tick");
        let connection = Connection::open(&path).expect("open task database");
        let current = repository::get(&connection, "local", predict_task.id)
            .expect("read task")
            .expect("task exists");
        if matches!(current.state, TaskState::Completed | TaskState::Failed) {
            assert_eq!(
                current.state,
                TaskState::Completed,
                "parity task failed: {current:?}"
            );
            let checkpoint: serde_json::Value =
                serde_json::from_str(current.checkpoint_json.as_deref().expect("checkpoint"))
                    .expect("decode checkpoint");
            // Source artifact predicts (125-25)/10*2+5 = 25.
            let prediction = checkpoint["result"]["predictions"][0][0]
                .as_f64()
                .expect("parity prediction");
            assert!(
                (prediction - 25.0).abs() <= 1e-4,
                "exported model diverges from source: {prediction}"
            );
            break;
        }
        assert!(Instant::now() < deadline, "parity task timed out");
        thread::yield_now();
    }

    drop(scheduler);
    let _ = std::fs::remove_file(model_path);
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
