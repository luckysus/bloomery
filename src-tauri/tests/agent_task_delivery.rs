use rusqlite::{params, Connection};
use serde_json::json;
use suna::storage::migrations::migrate;
use suna::tasks::{
    agent_delivery, repository,
    sources::{self, AgentTaskSource},
    NewTask,
};
use uuid::Uuid;

fn seed_run(connection: &Connection, workspace: &str) -> Uuid {
    let conversation = Uuid::new_v4().to_string();
    let message = Uuid::new_v4().to_string();
    let run = Uuid::new_v4();
    connection
        .execute(
            "INSERT INTO conversations (id, workspace_id, title, created_at, updated_at)
        VALUES (?1, ?2, 'delivery', '2026-10-09T00:00:00Z', '2026-10-09T00:00:00Z')",
            params![conversation, workspace],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO messages (id, workspace_id, conversation_id, role, content, created_at)
        VALUES (?1, ?2, ?3, 'user', 'train', '2026-10-09T00:00:00Z')",
            params![message, workspace, conversation],
        )
        .unwrap();
    connection.execute("INSERT INTO agent_runs (id, workspace_id, conversation_id, user_message_id, state, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, 'executing_tools', '2026-10-09T00:00:00Z', '2026-10-09T00:00:00Z')",
        params![run.to_string(), workspace, conversation, message]).unwrap();
    run
}

fn task(workspace: &str) -> NewTask {
    NewTask {
        workspace_id: workspace.into(),
        kind: "compute_train_linear_regression".into(),
        payload_json: "{}".into(),
        checkpoint_json: None,
        next_run_at: None,
        progress: 0,
    }
}

#[test]
fn source_submission_replay_and_checkpoint_ack_are_atomic() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let run = seed_run(&connection, "local");
    let source = AgentTaskSource {
        workspace_id: "local".into(),
        run_id: run,
        tool_call_id: Uuid::new_v4(),
    };
    let submitted = sources::submit_with(&mut connection, source.clone(), |conn| {
        repository::create(conn, task("local")).map_err(|error| error.to_string())
    })
    .unwrap();
    let replay = sources::submit_with(&mut connection, source.clone(), |_| {
        panic!("submission must not run twice")
    })
    .unwrap();
    assert_eq!(submitted.id, replay.id);
    assert_eq!(
        agent_delivery::pending_for_run(&connection, "local", run)
            .unwrap()
            .len(),
        1
    );
    assert!(agent_delivery::ready_for_run(&connection, "other", run)
        .unwrap()
        .is_empty());
    agent_delivery::acknowledge(&connection, "local", run, &[submitted.id]).unwrap();
    assert_eq!(
        agent_delivery::pending_for_run(&connection, "local", run)
            .unwrap()
            .len(),
        1,
        "cannot ack running tasks"
    );
    connection.execute("UPDATE background_tasks SET state = 'completed', progress = 100,
        checkpoint_json = ?1 WHERE id = ?2", params![json!({"result":{"metrics":{"r2":0.94},"model_pickle_base64":"secret_blob","apiKey":"must_redact"}}).to_string(), submitted.id.to_string()]).unwrap();
    let ready = agent_delivery::ready_for_run(&connection, "local", run).unwrap();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].result["metrics"]["r2"], json!(0.94));
    assert!(ready[0].result.get("model_pickle_base64").is_none());
    assert!(!ready[0].message().contains("must_redact"));
    assert!(ready[0].message().contains(&submitted.id.to_string()));
    {
        let transaction = connection.transaction().unwrap();
        agent_delivery::acknowledge(&transaction, "local", run, &[submitted.id]).unwrap();
        transaction.rollback().unwrap();
    }
    assert_eq!(
        agent_delivery::ready_for_run(&connection, "local", run)
            .unwrap()
            .len(),
        1,
        "rollback must retain the result"
    );
    let transaction = connection.transaction().unwrap();
    agent_delivery::acknowledge(&transaction, "local", run, &[submitted.id]).unwrap();
    agent_delivery::acknowledge(&transaction, "local", run, &[submitted.id]).unwrap();
    transaction.commit().unwrap();
    assert!(agent_delivery::pending_for_run(&connection, "local", run)
        .unwrap()
        .is_empty());
    let invalid_source = AgentTaskSource {
        tool_call_id: Uuid::new_v4(),
        ..source
    };
    assert!(
        sources::submit_with(&mut connection, invalid_source, |conn| repository::create(
            conn,
            task("other")
        )
        .map_err(|error| error.to_string()))
        .is_err()
    );
    let count: i64 = connection
        .query_row("SELECT count(*) FROM background_tasks", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        count, 1,
        "source validation rollback must not orphan a task"
    );
}

#[test]
fn failures_are_delivered_and_cancelled_original_run_never_resumes() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let run = seed_run(&connection, "local");
    let task = sources::create(
        &mut connection,
        task("local"),
        AgentTaskSource {
            workspace_id: "local".into(),
            run_id: run,
            tool_call_id: Uuid::new_v4(),
        },
    )
    .unwrap();
    connection
        .execute(
            "UPDATE background_tasks SET state = 'failed', error_code = 'worker_unavailable',
        checkpoint_json = ?1 WHERE id = ?2",
            params![
                json!({"result":{"message":"x".repeat(100_000)}}).to_string(),
                task.id.to_string()
            ],
        )
        .unwrap();
    let ready = agent_delivery::ready_for_run(&connection, "local", run).unwrap();
    assert_eq!(ready[0].error_code.as_deref(), Some("worker_unavailable"));
    assert!(ready[0].message().contains("\"success\":false"));
    assert!(ready[0].message().len() < 32 * 1024);
    connection.execute("UPDATE agent_runs SET state = 'cancelled', completed_at = '2026-10-09T00:01:00Z' WHERE id = ?1", params![run.to_string()]).unwrap();
    assert!(agent_delivery::pending_for_run(&connection, "local", run)
        .unwrap()
        .is_empty());
    connection.execute("UPDATE background_tasks SET state = 'completed', progress = 100, error_code = NULL WHERE id = ?1", params![task.id.to_string()]).unwrap();
    assert!(
        agent_delivery::ready_for_run(&connection, "local", run)
            .unwrap()
            .is_empty(),
        "late completion must not undo cancellation"
    );
}
