use bloomery::storage::migrations::migrate;
use bloomery::tasks::model::{NewTask, TaskState};
use bloomery::tasks::repository;
use rusqlite::Connection;

const WORKSPACE: &str = "local";

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open database");
    migrate(&mut connection).expect("migrate database");
    connection
}

fn mineru_task(conn: &Connection) -> bloomery::tasks::TaskRecord {
    repository::create(
        conn,
        NewTask {
            workspace_id: WORKSPACE.to_string(),
            kind: "mineru_parse".to_string(),
            payload_json: r#"{"file_name":"spec-sheet.pdf"}"#.to_string(),
            checkpoint_json: None,
            next_run_at: Some("2026-08-17T10:00:00.000Z".to_string()),
            progress: 0,
        },
    )
    .expect("create mineru task")
}

#[test]
fn task_records_start_and_finish_timestamps() {
    let mut conn = database();
    let task = mineru_task(&conn);

    // queued before claim: no timing recorded
    assert!(task.started_at.is_none());
    assert!(task.finished_at.is_none());

    let claimed = repository::claim_next(&mut conn, WORKSPACE, "2026-08-17T10:01:00.000Z")
        .expect("claim")
        .expect("a task was claimed");
    assert_eq!(claimed.id, task.id);
    assert_eq!(claimed.state, TaskState::Running);
    assert_eq!(
        claimed.started_at.as_deref(),
        Some("2026-08-17T10:01:00.000Z")
    );

    let finished = repository::transition(
        &mut conn,
        WORKSPACE,
        task.id,
        1,
        TaskState::Running,
        TaskState::Completed,
        None,
    )
    .expect("complete");
    assert_eq!(finished.state, TaskState::Completed);
    assert!(finished.finished_at.is_some());
    assert!(finished.started_at.is_some());
}

#[test]
fn non_terminal_transition_does_not_stamp_finish() {
    let mut conn = database();
    let task = mineru_task(&conn);
    repository::claim_next(&mut conn, WORKSPACE, "2026-08-17T10:01:00.000Z").expect("claim");

    // pause is not terminal, so finished_at stays empty
    let paused = repository::transition(
        &mut conn,
        WORKSPACE,
        task.id,
        1,
        TaskState::Running,
        TaskState::Paused,
        None,
    )
    .expect("pause");
    assert_eq!(paused.state, TaskState::Paused);
    assert!(paused.finished_at.is_none());
}
