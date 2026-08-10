use bloomery::compute::handler::{ComputeTaskHandler, COMPUTE_TRAIN_LINEAR_REGRESSION_KIND};
use bloomery::compute::worker::{WorkerClient, WorkerConfig};
use bloomery::storage::migrations::migrate;
use bloomery::tasks::model::{NewTask, TaskState};
use bloomery::tasks::repository;
use bloomery::tasks::scheduler::{
    EventSink, Scheduler, SchedulerConfig, SchedulerEvent, SystemClock,
};
use rusqlite::Connection;
use serde_json::json;
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
