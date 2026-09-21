use bloomery::storage::migrations::migrate;
use bloomery::tasks::model::NewTask;
use bloomery::tasks::sources::{create, AgentTaskSource};
use rusqlite::{params, Connection};
use uuid::Uuid;

fn seed_run(connection: &Connection, workspace_id: &str, run_id: Uuid) -> Uuid {
    let conversation_id = Uuid::new_v4();
    let message_id = Uuid::new_v4();
    connection
        .execute(
            "INSERT INTO conversations
                (id, workspace_id, title, created_at, updated_at)
             VALUES (?1, ?2, 'test', ?3, ?3)",
            params![
                conversation_id.to_string(),
                workspace_id,
                "2026-09-21T00:00:00Z"
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO messages
                (id, workspace_id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, 'user', 'run', ?4)",
            params![
                message_id.to_string(),
                workspace_id,
                conversation_id.to_string(),
                "2026-09-21T00:00:00Z"
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO agent_runs
                (id, workspace_id, conversation_id, user_message_id, state,
                 next_sequence, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'executing_tools', 1, ?5, ?5)",
            params![
                run_id.to_string(),
                workspace_id,
                conversation_id.to_string(),
                message_id.to_string(),
                "2026-09-21T00:00:00Z"
            ],
        )
        .unwrap();
    message_id
}

fn new_task(workspace_id: &str) -> NewTask {
    NewTask {
        workspace_id: workspace_id.to_string(),
        kind: "shell".to_string(),
        payload_json: "{\"command\":\"echo ok\"}".to_string(),
        checkpoint_json: None,
        next_run_at: None,
        progress: 0,
    }
}

#[test]
fn source_creation_is_atomic_and_idempotent() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    let run_id = Uuid::new_v4();
    seed_run(&connection, "local", run_id);
    let source = AgentTaskSource {
        workspace_id: "local".to_string(),
        run_id,
        tool_call_id: Uuid::new_v4(),
    };
    let task = create(&mut connection, new_task("local"), source.clone()).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_task_sources WHERE task_id = ?1",
                params![task.id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    let duplicate = create(&mut connection, new_task("local"), source).unwrap_err();
    assert_eq!(duplicate.code(), "duplicate_task_submission");
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM background_tasks", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        1
    );
}

#[test]
fn source_requires_active_tool_execution_run() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let run_id = Uuid::new_v4();
    seed_run(&connection, "local", run_id);
    connection
        .execute(
            "UPDATE agent_runs SET state = 'generating' WHERE id = ?1",
            params![run_id.to_string()],
        )
        .unwrap();
    let error = create(
        &mut connection,
        new_task("local"),
        AgentTaskSource {
            workspace_id: "local".to_string(),
            run_id,
            tool_call_id: Uuid::new_v4(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code(), "invalid_task_source");
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM background_tasks", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}
