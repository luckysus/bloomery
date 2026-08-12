use bloomery::app::compute_commands::logic::{
    optimization_task_status_on_connection, submit_optimization_on_connection,
    OptimizeSteelProcessRequest,
};
use bloomery::storage::migrations::migrate;
use bloomery::tasks::model::NewTask;
use bloomery::tasks::repository;
use rusqlite::Connection;
use serde_json::json;

fn migrated_database() -> (std::path::PathBuf, Connection) {
    let path = std::env::temp_dir().join(format!(
        "bloomery-optimization-gateway-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let mut connection = Connection::open(&path).expect("open optimization database");
    migrate(&mut connection).expect("migrate optimization database");
    (path, connection)
}

fn completed_training_task(connection: &mut Connection) -> uuid::Uuid {
    let task = repository::create(
        connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: "compute_train_linear_regression".to_string(),
            payload_json: json!({
                "operation": "train_linear_regression",
                "payload": {"dataset_id": "dataset-1"}
            })
            .to_string(),
            checkpoint_json: None,
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create training task");
    connection
        .execute(
            "UPDATE background_tasks SET state = 'completed', progress = 100, next_run_at = NULL, checkpoint_json = ?1 WHERE id = ?2",
            rusqlite::params![
                json!({
                    "stage": "completed",
                    "result": {
                        "artifact": {
                            "artifact_version": "linear-regression.v1",
                            "model_id": "model-1",
                            "model_type": "linear_regression",
                            "feature_names": ["temperature", "carbon"],
                            "feature_schema": {"count": 2}
                        }
                    }
                })
                .to_string(),
                task.id.to_string()
            ],
        )
        .expect("complete training task fixture");
    task.id
}

fn completed_sklearn_training_task(connection: &mut Connection) -> uuid::Uuid {
    let task = repository::create(
        connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: "compute_train_sklearn_model".to_string(),
            payload_json: json!({
                "operation": "train_sklearn_model",
                "payload": {"dataset_id": "dataset-1", "algorithm": "elasticnet"}
            })
            .to_string(),
            checkpoint_json: None,
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create sklearn training task");
    connection
        .execute(
            "UPDATE background_tasks SET state = 'completed', progress = 100, next_run_at = NULL, checkpoint_json = ?1 WHERE id = ?2",
            rusqlite::params![
                json!({
                    "stage": "completed",
                    "result": {
                        "artifact": {
                            "artifact_version": "sklearn-pickle.v1",
                            "model_id": "sklearn-model-1",
                            "model_type": "elasticnet",
                            "feature_names": ["temperature", "carbon"],
                            "feature_schema": {"count": 2},
                            "preprocessing": {"means": [0.0, 0.0], "scales": [1.0, 1.0]},
                            "model_pickle_base64": "c2FmZS1maXh0dXJl"
                        }
                    }
                })
                .to_string(),
                task.id.to_string()
            ],
        )
        .expect("complete sklearn training task fixture");
    task.id
}

fn optimization_request(training_task_id: uuid::Uuid) -> OptimizeSteelProcessRequest {
    OptimizeSteelProcessRequest {
        dataset_id: "dataset-1".to_string(),
        training_task_id: training_task_id.to_string(),
        direction: "minimize".to_string(),
        objective_columns: vec![0],
        bounds: vec![
            json!({"min": 0.0, "max": 10.0}),
            json!({"min": 0.0, "max": 5.0}),
        ],
        fixed_values: vec![None, Some(2.0)],
        constraints: vec![json!({
            "kind": "inequality",
            "coefficients": [1.0, 0.0],
            "value": 4.0
        })],
        trials: 24,
        seed: 7,
    }
}

#[test]
fn gateway_submission_creates_a_persisted_optimization_task() {
    let (path, mut connection) = migrated_database();
    let training_task_id = completed_training_task(&mut connection);

    let task = submit_optimization_on_connection(
        &mut connection,
        &optimization_request(training_task_id),
        training_task_id,
    )
    .expect("submit optimization task");

    assert_eq!(task.kind, "compute_optimize_constrained");
    let payload: serde_json::Value =
        serde_json::from_str(&task.payload_json).expect("decode optimization payload");
    assert_eq!(payload["operation"], "optimize_constrained");
    assert_eq!(
        payload["payload"]["training_task_id"],
        training_task_id.to_string()
    );
    assert_eq!(payload["payload"]["fixed_values"], json!({"carbon": 2.0}));
    assert_eq!(
        payload["payload"]["constraints"][0]["coefficients"],
        json!({"temperature": 1.0, "carbon": 0.0})
    );

    let status = optimization_task_status_on_connection(&connection, task.id)
        .expect("read queued optimization status");
    assert_eq!(status["state"], "queued");
    assert!(status.get("result").is_none());

    connection
        .execute(
            "UPDATE background_tasks SET state = 'completed', progress = 100, next_run_at = NULL, checkpoint_json = ?1 WHERE id = ?2",
            rusqlite::params![
                json!({
                    "stage": "completed",
                    "result": {
                        "method": "tpe",
                        "recommendations": [{"values": {"temperature": 4.0}, "feasible": true}]
                    }
                })
                .to_string(),
                task.id.to_string()
            ],
        )
        .expect("complete optimization task fixture");
    let status = optimization_task_status_on_connection(&connection, task.id)
        .expect("read completed optimization status");
    assert_eq!(status["state"], "completed");
    assert_eq!(
        status["result"]["recommendations"][0]["feasible"],
        json!(true)
    );

    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn gateway_submission_rejects_mismatched_datasets_and_unfinished_training() {
    let (path, mut connection) = migrated_database();
    let training_task_id = completed_training_task(&mut connection);

    let mut request = optimization_request(training_task_id);
    request.dataset_id = "other-dataset".to_string();
    let error = submit_optimization_on_connection(&mut connection, &request, training_task_id)
        .expect_err("mismatched dataset must be rejected");
    assert_eq!(
        error,
        "optimization dataset does not match the training task"
    );

    let pending_training = repository::create(
        &mut connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: "compute_train_linear_regression".to_string(),
            payload_json: json!({
                "operation": "train_linear_regression",
                "payload": {"dataset_id": "dataset-1"}
            })
            .to_string(),
            checkpoint_json: None,
            next_run_at: None,
            progress: 0,
        },
    )
    .expect("create pending training task");
    let error = submit_optimization_on_connection(
        &mut connection,
        &optimization_request(training_task_id),
        pending_training.id,
    )
    .expect_err("unfinished training must be rejected");
    assert_eq!(error, "training task must be completed before optimization");

    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn gateway_submission_accepts_a_completed_sklearn_training_task() {
    let (path, mut connection) = migrated_database();
    let training_task_id = completed_sklearn_training_task(&mut connection);

    let task = submit_optimization_on_connection(
        &mut connection,
        &optimization_request(training_task_id),
        training_task_id,
    )
    .expect("sklearn training task should be eligible for optimization");

    let payload: serde_json::Value =
        serde_json::from_str(&task.payload_json).expect("decode sklearn optimization payload");
    assert_eq!(
        payload["payload"]["artifact"]["artifact_version"],
        "sklearn-pickle.v1"
    );
    assert_eq!(payload["payload"]["artifact"]["model_type"], "elasticnet");

    drop(connection);
    let _ = std::fs::remove_file(path);
}
