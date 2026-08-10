use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bloomery::storage::migrations::migrate;
use bloomery::tasks::model::{NewTask, TaskState};
use bloomery::tasks::repository;
use bloomery::tasks::scheduler::{
    Clock, EventSink, HandlerContext, HandlerError, HandlerFuture, HandlerOutcome, Scheduler,
    SchedulerConfig, SchedulerEvent, SchedulerState, TaskHandler,
};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;
use uuid::Uuid;

const NOW: &str = "2026-07-31T00:00:00Z";

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open memory database");
    migrate(&mut connection).expect("migrate schema");
    connection
}

fn task(workspace_id: &str) -> NewTask {
    NewTask {
        workspace_id: workspace_id.to_string(),
        kind: "demo".to_string(),
        payload_json: r#"{"input":1}"#.to_string(),
        checkpoint_json: Some(r#"{"step":"start"}"#.to_string()),
        next_run_at: None,
        progress: 12,
    }
}

#[test]
fn transitions_cover_state_names_and_legal_edges() {
    let states = [
        ("queued", TaskState::Queued),
        ("running", TaskState::Running),
        ("waiting_external", TaskState::WaitingExternal),
        ("paused", TaskState::Paused),
        ("completed", TaskState::Completed),
        ("failed", TaskState::Failed),
        ("cancelled", TaskState::Cancelled),
        ("interrupted", TaskState::Interrupted),
    ];
    for (name, state) in states {
        assert_eq!(state.as_str(), name);
        assert_eq!(TaskState::from_str(name).unwrap(), state);
    }
    assert!(TaskState::from_str("unknown").is_err());

    let legal = [
        (TaskState::Queued, TaskState::Running),
        (TaskState::Queued, TaskState::Paused),
        (TaskState::Queued, TaskState::Cancelled),
        (TaskState::Running, TaskState::WaitingExternal),
        (TaskState::Running, TaskState::Paused),
        (TaskState::Running, TaskState::Completed),
        (TaskState::Running, TaskState::Failed),
        (TaskState::Running, TaskState::Cancelled),
        (TaskState::Running, TaskState::Interrupted),
        (TaskState::WaitingExternal, TaskState::Running),
        (TaskState::WaitingExternal, TaskState::Paused),
        (TaskState::WaitingExternal, TaskState::Completed),
        (TaskState::WaitingExternal, TaskState::Failed),
        (TaskState::WaitingExternal, TaskState::Cancelled),
        (TaskState::WaitingExternal, TaskState::Interrupted),
        (TaskState::Paused, TaskState::Queued),
        (TaskState::Paused, TaskState::Cancelled),
        (TaskState::Failed, TaskState::Queued),
        (TaskState::Cancelled, TaskState::Queued),
        (TaskState::Interrupted, TaskState::Queued),
        (TaskState::Interrupted, TaskState::Cancelled),
    ];
    let all_states = states.map(|(_, state)| state);
    for from in all_states {
        for to in all_states {
            assert_eq!(
                from.can_transition_to(to),
                legal.contains(&(from, to)),
                "unexpected transition result for {from:?} -> {to:?}"
            );
        }
    }
}

#[test]
fn creates_and_reads_all_persisted_fields_and_isolates_workspaces() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).expect("create task");
    assert_eq!(created.state, TaskState::Queued);
    assert_eq!(created.attempt, 0);
    assert!(!created.cancel_requested);
    assert_eq!(
        repository::get(&conn, "workspace-a", created.id).unwrap(),
        Some(created.clone())
    );
    assert!(repository::get(&conn, "workspace-b", created.id)
        .unwrap()
        .is_none());
    assert_eq!(
        repository::list(&conn, "workspace-a").unwrap(),
        vec![created.clone()]
    );
    assert!(repository::list(&conn, "workspace-b").unwrap().is_empty());
    assert_eq!(
        created.checkpoint_json.as_deref(),
        Some(r#"{"step":"start"}"#)
    );
    assert_eq!(created.progress, 12);
    assert!(created.created_at.ends_with('Z'));
    assert!(created.updated_at.ends_with('Z'));
}

#[test]
fn task_mutations_are_workspace_scoped() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();

    assert_eq!(
        repository::checkpoint(
            &mut conn,
            "workspace-b",
            created.id,
            created.attempt,
            Some("{}"),
            50,
            None,
        )
        .unwrap_err()
        .code(),
        "task_not_found"
    );
    assert_eq!(
        repository::transition(
            &mut conn,
            "workspace-b",
            created.id,
            created.attempt,
            TaskState::Queued,
            TaskState::Running,
            None,
        )
        .unwrap_err()
        .code(),
        "task_not_found"
    );
    assert_eq!(
        repository::request_cancellation(&mut conn, "workspace-b", created.id)
            .unwrap_err()
            .code(),
        "task_not_found"
    );
    assert_eq!(
        repository::get(&conn, "workspace-a", created.id).unwrap(),
        Some(created)
    );
}

#[test]
fn task_records_serialize_with_stable_fields() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();

    let value = serde_json::to_value(created).expect("serialize task record");

    assert_eq!(value["workspace_id"], "workspace-a");
    assert_eq!(value["state"], "queued");
    assert_eq!(value["progress"], 12);
    assert_eq!(value["cancel_requested"], false);
}

#[test]
fn rejects_inconsistent_error_state_loaded_from_storage() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();
    conn.pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    conn.execute(
        "UPDATE background_tasks SET error_code = 'unexpected' WHERE id = ?1",
        [created.id.to_string()],
    )
    .unwrap();

    assert_eq!(
        repository::get(&conn, "workspace-a", created.id)
            .unwrap_err()
            .code(),
        "storage_error"
    );

    conn.execute(
        "UPDATE background_tasks SET state = 'failed', error_code = NULL WHERE id = ?1",
        [created.id.to_string()],
    )
    .unwrap();
    assert_eq!(
        repository::get(&conn, "workspace-a", created.id)
            .unwrap_err()
            .code(),
        "storage_error"
    );
}

#[test]
fn rejects_invalid_fields_before_writing() {
    let mut conn = database();
    for invalid in [
        NewTask {
            workspace_id: "".into(),
            ..task("workspace-a")
        },
        NewTask {
            kind: " ".into(),
            ..task("workspace-a")
        },
        NewTask {
            kind: "bad\nkind".into(),
            ..task("workspace-a")
        },
        NewTask {
            payload_json: "not-json".into(),
            ..task("workspace-a")
        },
        NewTask {
            checkpoint_json: Some("not-json".into()),
            ..task("workspace-a")
        },
        NewTask {
            next_run_at: Some("tomorrow".into()),
            ..task("workspace-a")
        },
        NewTask {
            progress: 101,
            ..task("workspace-a")
        },
    ] {
        assert_eq!(
            repository::create(&mut conn, invalid).unwrap_err().code(),
            "invalid_task"
        );
    }
    let id = Uuid::new_v4();
    assert_eq!(
        repository::checkpoint(&mut conn, "workspace-a", id, 0, Some("{}"), 0, None)
            .unwrap_err()
            .code(),
        "task_not_found"
    );
}

#[test]
fn checkpoint_persists_json_progress_and_next_run() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();
    let claimed = repository::claim_next(&mut conn, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    let next_run = "2099-01-02T03:04:05Z";
    let updated = repository::checkpoint(
        &mut conn,
        "workspace-a",
        created.id,
        claimed.attempt,
        Some(r#"{"step":"middle"}"#),
        55,
        Some(next_run),
    )
    .unwrap();
    assert_eq!(
        updated.checkpoint_json.as_deref(),
        Some(r#"{"step":"middle"}"#)
    );
    assert_eq!(updated.progress, 55);
    assert_eq!(updated.next_run_at.as_deref(), Some(next_run));
    assert_eq!(
        repository::get(&conn, "workspace-a", created.id).unwrap(),
        Some(updated)
    );
}

#[test]
fn transition_enforces_legal_edges_and_rolls_back_illegal_updates() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();
    let running = repository::claim_next(&mut conn, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    assert_eq!(running.state, TaskState::Running);
    let error = repository::transition(
        &mut conn,
        "workspace-a",
        created.id,
        running.attempt,
        TaskState::Running,
        TaskState::Queued,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code(), "illegal_transition");
    assert_eq!(
        repository::get(&conn, "workspace-a", created.id)
            .unwrap()
            .unwrap()
            .state,
        TaskState::Running
    );

    let checkpointed = repository::checkpoint(
        &mut conn,
        "workspace-a",
        created.id,
        running.attempt,
        Some(r#"{"step":"retry"}"#),
        66,
        Some("2099-01-01T00:00:00Z"),
    )
    .unwrap();
    let cancelling =
        repository::request_cancellation(&mut conn, "workspace-a", created.id).unwrap();
    assert!(cancelling.cancel_requested);

    let failed = repository::transition(
        &mut conn,
        "workspace-a",
        created.id,
        running.attempt,
        TaskState::Running,
        TaskState::Failed,
        Some("upstream_timeout"),
    )
    .unwrap();
    assert_eq!(failed.error_code.as_deref(), Some("upstream_timeout"));
    assert_eq!(failed.checkpoint_json, checkpointed.checkpoint_json);
    assert_eq!(failed.progress, checkpointed.progress);
    assert_eq!(failed.next_run_at, checkpointed.next_run_at);
    assert!(failed.cancel_requested);
    let queued = repository::transition(
        &mut conn,
        "workspace-a",
        created.id,
        failed.attempt,
        TaskState::Failed,
        TaskState::Queued,
        None,
    )
    .unwrap();
    assert_eq!(queued.error_code, None);
    assert!(!queued.cancel_requested);
    assert_eq!(queued.checkpoint_json, failed.checkpoint_json);
    assert_eq!(queued.progress, failed.progress);
    assert_eq!(queued.next_run_at, failed.next_run_at);
}

#[test]
fn retry_update_is_atomic_and_rolls_back_without_stranding_failed_state() {
    let mut connection = database();
    let created = repository::create(&mut connection, task("workspace-a")).unwrap();
    let running = repository::claim_next(&mut connection, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    assert_eq!(
        repository::schedule_retry(
            &mut connection,
            "workspace-b",
            created.id,
            running.attempt,
            running.checkpoint_json.as_deref(),
            running.progress,
            "2026-07-31T00:00:10Z",
            NOW,
        )
        .unwrap_err()
        .code(),
        "task_not_found"
    );
    assert_eq!(
        repository::schedule_retry(
            &mut connection,
            "workspace-a",
            created.id,
            running.attempt + 1,
            running.checkpoint_json.as_deref(),
            running.progress,
            "2026-07-31T00:00:10Z",
            NOW,
        )
        .unwrap_err()
        .code(),
        "stale_claim"
    );
    connection
        .execute_batch(
            "CREATE TRIGGER abort_task_retry
             AFTER UPDATE OF state ON background_tasks
             WHEN NEW.state = 'queued'
             BEGIN
               SELECT RAISE(ABORT, 'retry write failed');
             END;",
        )
        .unwrap();

    let error = repository::schedule_retry(
        &mut connection,
        "workspace-a",
        created.id,
        running.attempt,
        Some(r#"{"step":"retry"}"#),
        55,
        "2026-07-31T00:00:10Z",
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code(), "storage_error");
    let rolled_back = repository::get(&connection, "workspace-a", created.id)
        .unwrap()
        .unwrap();
    assert_eq!(rolled_back.state, TaskState::Running);
    assert_eq!(rolled_back.checkpoint_json, running.checkpoint_json);
    assert_eq!(rolled_back.progress, running.progress);
    assert_eq!(rolled_back.next_run_at, running.next_run_at);
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM background_tasks WHERE state = 'failed'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );

    connection
        .execute_batch("DROP TRIGGER abort_task_retry")
        .unwrap();
    let queued = repository::schedule_retry(
        &mut connection,
        "workspace-a",
        created.id,
        running.attempt,
        Some(r#"{"step":"retry"}"#),
        55,
        "2026-07-31T00:00:10Z",
        NOW,
    )
    .unwrap();
    assert_eq!(queued.state, TaskState::Queued);
    assert_eq!(queued.attempt, running.attempt);
    assert_eq!(
        queued.checkpoint_json.as_deref(),
        Some(r#"{"step":"retry"}"#)
    );
    assert_eq!(queued.progress, 55);
    assert_eq!(queued.next_run_at.as_deref(), Some("2026-07-31T00:00:10Z"));
    assert_eq!(queued.error_code, None);
    assert!(!queued.cancel_requested);
    assert_eq!(queued.updated_at, NOW);
    assert_eq!(
        repository::schedule_retry(
            &mut connection,
            "workspace-a",
            created.id,
            running.attempt,
            queued.checkpoint_json.as_deref(),
            queued.progress,
            "2026-07-31T00:00:20Z",
            NOW,
        )
        .unwrap_err()
        .code(),
        "stale_claim"
    );
}

#[test]
fn retry_atomically_honors_a_committed_cancellation() {
    let mut connection = database();
    let created = repository::create(&mut connection, task("workspace-a")).unwrap();
    let running = repository::claim_next(&mut connection, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    assert!(
        repository::request_cancellation(&mut connection, "workspace-a", created.id)
            .unwrap()
            .cancel_requested
    );

    let cancelled = repository::schedule_retry(
        &mut connection,
        "workspace-a",
        created.id,
        running.attempt,
        Some(r#"{"step":"retry"}"#),
        55,
        "2026-07-31T00:00:10Z",
        NOW,
    )
    .unwrap();

    assert_eq!(cancelled.state, TaskState::Cancelled);
    assert_eq!(cancelled.attempt, running.attempt);
    assert!(cancelled.cancel_requested);
    assert_eq!(
        cancelled.checkpoint_json.as_deref(),
        Some(r#"{"step":"retry"}"#)
    );
    assert_eq!(cancelled.progress, 55);
    assert_eq!(
        cancelled.next_run_at.as_deref(),
        Some("2026-07-31T00:00:10Z")
    );
    assert_eq!(cancelled.error_code, None);
}

#[test]
fn completion_sets_progress_and_clears_schedule_and_error_code_is_failed_only() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();
    let running = repository::claim_next(&mut conn, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    let completed = repository::transition(
        &mut conn,
        "workspace-a",
        created.id,
        running.attempt,
        TaskState::Running,
        TaskState::Completed,
        None,
    )
    .unwrap();
    assert_eq!(completed.progress, 100);
    assert_eq!(completed.next_run_at, None);
    assert_eq!(completed.error_code, None);

    let second = repository::create(&mut conn, task("workspace-a")).unwrap();
    assert_eq!(
        repository::transition(
            &mut conn,
            "workspace-a",
            second.id,
            second.attempt,
            TaskState::Queued,
            TaskState::Failed,
            None,
        )
        .unwrap_err()
        .code(),
        "invalid_task"
    );
    assert_eq!(
        repository::transition(
            &mut conn,
            "workspace-a",
            second.id,
            second.attempt,
            TaskState::Queued,
            TaskState::Running,
            Some("ignored")
        )
        .unwrap_err()
        .code(),
        "invalid_task"
    );
}

#[test]
fn completed_task_cannot_be_checkpointed() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();
    let running = repository::claim_next(&mut conn, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    let completed = repository::transition(
        &mut conn,
        "workspace-a",
        created.id,
        running.attempt,
        TaskState::Running,
        TaskState::Completed,
        None,
    )
    .unwrap();

    let error = repository::checkpoint(
        &mut conn,
        "workspace-a",
        created.id,
        completed.attempt,
        Some(r#"{"step":"late"}"#),
        50,
        Some("2099-01-01T00:00:00Z"),
    )
    .unwrap_err();

    assert_eq!(error.code(), "stale_claim");
    assert_eq!(
        repository::get(&conn, "workspace-a", created.id).unwrap(),
        Some(completed)
    );
}

#[test]
fn rejects_corrupt_completed_record_loaded_from_storage() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();
    let running = repository::claim_next(&mut conn, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    repository::transition(
        &mut conn,
        "workspace-a",
        created.id,
        running.attempt,
        TaskState::Running,
        TaskState::Completed,
        None,
    )
    .unwrap();
    conn.pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    conn.execute(
        "UPDATE background_tasks SET progress = 99 WHERE id = ?1",
        [created.id.to_string()],
    )
    .unwrap();

    assert_eq!(
        repository::get(&conn, "workspace-a", created.id)
            .unwrap_err()
            .code(),
        "storage_error"
    );
}

#[test]
fn cancellation_only_sets_a_flag_and_claim_skips_future_and_cancelled_tasks() {
    let mut conn = database();
    let future = NewTask {
        next_run_at: Some("2099-01-01T00:00:00Z".to_string()),
        ..task("workspace-a")
    };
    let future = repository::create(&mut conn, future).unwrap();
    let cancelled = repository::create(&mut conn, task("workspace-a")).unwrap();
    repository::request_cancellation(&mut conn, "workspace-a", cancelled.id).unwrap();
    assert!(
        repository::get(&conn, "workspace-a", cancelled.id)
            .unwrap()
            .unwrap()
            .cancel_requested
    );
    assert!(repository::claim_next(&mut conn, "workspace-a", NOW)
        .unwrap()
        .is_none());
    assert_eq!(
        repository::get(&conn, "workspace-a", future.id)
            .unwrap()
            .unwrap()
            .state,
        TaskState::Queued
    );
}

#[test]
fn generic_transition_cannot_bypass_atomic_claim() {
    let mut conn = database();
    let future = repository::create(
        &mut conn,
        NewTask {
            next_run_at: Some("2099-01-01T00:00:00Z".into()),
            ..task("workspace-a")
        },
    )
    .unwrap();
    let cancelled = repository::create(&mut conn, task("workspace-a")).unwrap();
    repository::request_cancellation(&mut conn, "workspace-a", cancelled.id).unwrap();

    for record in [future, cancelled] {
        assert_eq!(
            repository::transition(
                &mut conn,
                "workspace-a",
                record.id,
                record.attempt,
                TaskState::Queued,
                TaskState::Running,
                None,
            )
            .unwrap_err()
            .code(),
            "claim_required"
        );
        assert_eq!(
            repository::get(&conn, "workspace-a", record.id)
                .unwrap()
                .unwrap()
                .state,
            TaskState::Queued
        );
    }
}

#[test]
fn claim_is_due_deterministic_and_increments_attempt_atomically() {
    let mut conn = database();
    let later = repository::create(
        &mut conn,
        NewTask {
            next_run_at: Some("2000-01-02T00:00:00Z".into()),
            ..task("workspace-a")
        },
    )
    .unwrap();
    let first = repository::create(
        &mut conn,
        NewTask {
            next_run_at: Some("2000-01-01T00:00:00Z".into()),
            ..task("workspace-a")
        },
    )
    .unwrap();
    let claimed = repository::claim_next(&mut conn, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, first.id);
    assert_eq!(claimed.state, TaskState::Running);
    assert_eq!(claimed.attempt, 1);
    assert_eq!(claimed.updated_at, NOW);
    assert_eq!(
        repository::get(&conn, "workspace-a", later.id)
            .unwrap()
            .unwrap()
            .state,
        TaskState::Queued
    );
}

#[test]
fn claim_is_workspace_scoped() {
    let mut conn = database();
    let other = repository::create(&mut conn, task("workspace-b")).unwrap();

    assert!(repository::claim_next(&mut conn, "workspace-a", NOW)
        .unwrap()
        .is_none());
    assert_eq!(
        repository::get(&conn, "workspace-b", other.id)
            .unwrap()
            .unwrap()
            .state,
        TaskState::Queued
    );
}

#[test]
fn claim_reports_corrupt_persisted_schedule() {
    let mut conn = database();
    let created = repository::create(&mut conn, task("workspace-a")).unwrap();
    conn.execute(
        "UPDATE background_tasks SET next_run_at = 'not-a-date' WHERE id = ?1",
        [created.id.to_string()],
    )
    .unwrap();

    assert_eq!(
        repository::claim_next(&mut conn, "workspace-a", NOW)
            .unwrap_err()
            .code(),
        "storage_error"
    );
}

#[test]
fn two_file_connections_claim_one_task() {
    let path = std::env::temp_dir().join(format!("bloomery-task-{}.sqlite3", Uuid::new_v4()));
    let mut setup = Connection::open(&path).unwrap();
    migrate(&mut setup).unwrap();
    repository::create(&mut setup, task("workspace-a")).unwrap();
    drop(setup);

    let ready = Arc::new((Mutex::new(0usize), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let handles = (0..2)
        .map(|_| {
            let path = path.clone();
            let ready = Arc::clone(&ready);
            let release = Arc::clone(&release);
            thread::spawn(move || {
                let mut connection = Connection::open(&path).unwrap();
                connection
                    .busy_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                {
                    let mut count = ready.0.lock().unwrap();
                    *count += 1;
                    ready.1.notify_all();
                }
                wait_for_release(&release);
                repository::claim_next(&mut connection, "workspace-a", NOW).unwrap()
            })
        })
        .collect::<Vec<_>>();
    wait_for_count(&ready, 2);
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    let claims = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    let claimed = claims.iter().filter(|claim| claim.is_some()).count();
    let check = Connection::open(&path).unwrap();
    check
        .busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let records = repository::list(&check, "workspace-a").unwrap();
    drop(check);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(claimed, 1);
    assert_eq!(records[0].state, TaskState::Running);
    assert_eq!(records[0].attempt, 1);
}

#[test]
fn stale_worker_cannot_mutate_a_reclaimed_task() {
    let path = std::env::temp_dir().join(format!("bloomery-stale-{}.sqlite3", Uuid::new_v4()));
    let mut old_worker = Connection::open(&path).unwrap();
    migrate(&mut old_worker).unwrap();
    let created = repository::create(&mut old_worker, task("workspace-a")).unwrap();
    let first_claim = repository::claim_next(&mut old_worker, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    repository::transition(
        &mut old_worker,
        "workspace-a",
        created.id,
        first_claim.attempt,
        TaskState::Running,
        TaskState::Failed,
        Some("retryable"),
    )
    .unwrap();
    repository::transition(
        &mut old_worker,
        "workspace-a",
        created.id,
        first_claim.attempt,
        TaskState::Failed,
        TaskState::Queued,
        None,
    )
    .unwrap();
    assert_eq!(
        repository::transition(
            &mut old_worker,
            "workspace-a",
            created.id,
            first_claim.attempt,
            TaskState::Queued,
            TaskState::Running,
            None,
        )
        .unwrap_err()
        .code(),
        "claim_required"
    );

    assert_eq!(
        repository::checkpoint(
            &mut old_worker,
            "workspace-a",
            created.id,
            first_claim.attempt,
            Some(r#"{"stale_before_reclaim":true}"#),
            80,
            None,
        )
        .unwrap_err()
        .code(),
        "stale_claim"
    );
    assert_eq!(
        repository::transition(
            &mut old_worker,
            "workspace-a",
            created.id,
            first_claim.attempt,
            TaskState::Running,
            TaskState::Completed,
            None,
        )
        .unwrap_err()
        .code(),
        "stale_claim"
    );

    let mut new_worker = Connection::open(&path).unwrap();
    new_worker
        .busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let second_claim = repository::claim_next(&mut new_worker, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    assert_eq!(second_claim.attempt, first_claim.attempt + 1);

    assert_eq!(
        repository::checkpoint(
            &mut old_worker,
            "workspace-a",
            created.id,
            first_claim.attempt,
            Some(r#"{"stale":true}"#),
            90,
            None,
        )
        .unwrap_err()
        .code(),
        "stale_claim"
    );
    assert_eq!(
        repository::transition(
            &mut old_worker,
            "workspace-a",
            created.id,
            first_claim.attempt,
            TaskState::Running,
            TaskState::Completed,
            None,
        )
        .unwrap_err()
        .code(),
        "stale_claim"
    );

    let current = repository::get(&new_worker, "workspace-a", created.id)
        .unwrap()
        .unwrap();
    drop(new_worker);
    drop(old_worker);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(current.state, TaskState::Running);
    assert_eq!(current.attempt, second_claim.attempt);
    assert_eq!(current.checkpoint_json, second_claim.checkpoint_json);
}

struct TestDatabase {
    path: std::path::PathBuf,
}

impl TestDatabase {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("bloomery-scheduler-{}.sqlite3", Uuid::new_v4()));
        let mut connection = Connection::open(&path).expect("open scheduler database");
        migrate(&mut connection).expect("migrate scheduler database");
        drop(connection);
        Self { path }
    }

    fn connect(&self) -> Connection {
        let connection = Connection::open(&self.path).expect("open scheduler connection");
        connection
            .busy_timeout(Duration::from_secs(2))
            .expect("set busy timeout");
        connection
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match std::fs::remove_file(&self.path) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error)
                    if error.kind() == std::io::ErrorKind::PermissionDenied
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("remove scheduler database: {error}"),
            }
        }
    }
}

struct FakeClock {
    now: Mutex<DateTime<Utc>>,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            now: Mutex::new(Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap()),
        }
    }

    fn advance(&self, duration: chrono::Duration) {
        let mut now = self.now.lock().unwrap();
        *now += duration;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().unwrap()
    }
}

type HandlerFn = dyn Fn(bloomery::tasks::TaskRecord, HandlerContext) -> HandlerFuture + Send + Sync;

struct FakeHandler {
    kind: &'static str,
    resumable: bool,
    run: Arc<HandlerFn>,
}

impl FakeHandler {
    fn new(
        kind: &'static str,
        resumable: bool,
        run: impl Fn(bloomery::tasks::TaskRecord, HandlerContext) -> HandlerFuture
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            kind,
            resumable,
            run: Arc::new(run),
        }
    }
}

impl TaskHandler for FakeHandler {
    fn kind(&self) -> &str {
        self.kind
    }

    fn resumable(&self) -> bool {
        self.resumable
    }

    fn run(&self, task: bloomery::tasks::TaskRecord, context: HandlerContext) -> HandlerFuture {
        (self.run)(task, context)
    }
}

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<SchedulerEvent>>,
}

impl EventSink for RecordingSink {
    fn emit(&self, event: SchedulerEvent) {
        self.events.lock().unwrap().push(event);
    }
}

fn scheduler_config(max_workers: usize) -> SchedulerConfig {
    SchedulerConfig {
        max_workers,
        max_attempts: 4,
        retry_base: Duration::from_secs(10),
        retry_max: Duration::from_secs(40),
        poll_interval: Duration::from_millis(10),
    }
}

fn scheduler_task(kind: &str) -> NewTask {
    NewTask {
        kind: kind.to_string(),
        ..task("workspace-a")
    }
}

fn scheduler(
    database: &TestDatabase,
    clock: Arc<FakeClock>,
    handlers: Vec<Arc<dyn TaskHandler>>,
    sink: Arc<dyn EventSink>,
    config: SchedulerConfig,
) -> Scheduler {
    Scheduler::new(
        database.path.clone(),
        "workspace-a".to_string(),
        config,
        clock,
        handlers,
        sink,
    )
    .expect("create scheduler")
}

fn drive_until(scheduler: &mut Scheduler, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !condition() {
        scheduler.tick().expect("scheduler tick");
        assert!(Instant::now() < deadline, "scheduler test timed out");
        thread::yield_now();
    }
}

fn wait_for_count(signal: &(Mutex<usize>, Condvar), expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut count = signal.0.lock().unwrap();
    while *count < expected {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "handler start timed out");
        let (next, timeout) = signal.1.wait_timeout(count, remaining).unwrap();
        count = next;
        assert!(
            !timeout.timed_out() || *count >= expected,
            "handler start timed out"
        );
    }
}

fn wait_for_release(signal: &(Mutex<bool>, Condvar)) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut released = signal.0.lock().unwrap();
    while !*released {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "handler release timed out");
        let (next, timeout) = signal.1.wait_timeout(released, remaining).unwrap();
        released = next;
        assert!(
            !timeout.timed_out() || *released,
            "handler release timed out"
        );
    }
}

#[test]
fn scheduler_enforces_bounded_concurrency_with_simultaneous_handlers() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    for _ in 0..3 {
        repository::create(&mut connection, scheduler_task("bounded")).unwrap();
    }
    drop(connection);

    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let started = Arc::new((Mutex::new(0usize), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let handler = FakeHandler::new("bounded", true, {
        let active = Arc::clone(&active);
        let maximum = Arc::clone(&maximum);
        let started = Arc::clone(&started);
        let release = Arc::clone(&release);
        move |_, _| {
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            let started = Arc::clone(&started);
            let release = Arc::clone(&release);
            Box::pin(async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                {
                    let mut count = started.0.lock().unwrap();
                    *count += 1;
                    started.1.notify_all();
                }
                wait_for_release(&release);
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(HandlerOutcome::Completed)
            })
        }
    });
    let clock = Arc::new(FakeClock::new());
    let mut scheduler = scheduler(
        &database,
        clock,
        vec![Arc::new(handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(2),
    );

    scheduler.tick().unwrap();
    wait_for_count(&started, 2);
    assert_eq!(maximum.load(Ordering::SeqCst), 2);
    let states = repository::list(&database.connect(), "workspace-a").unwrap();
    assert_eq!(
        states
            .iter()
            .filter(|task| task.state == TaskState::Running)
            .count(),
        2
    );
    assert_eq!(
        states
            .iter()
            .filter(|task| task.state == TaskState::Queued)
            .count(),
        1
    );

    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    drive_until(&mut scheduler, || {
        repository::list(&database.connect(), "workspace-a")
            .unwrap()
            .iter()
            .all(|task| task.state == TaskState::Completed)
    });
    assert_eq!(maximum.load(Ordering::SeqCst), 2);
}

#[test]
fn scheduler_emits_progress_only_after_successful_checkpoint_persistence() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created = repository::create(&mut connection, scheduler_task("checkpoint")).unwrap();
    drop(connection);
    let sink = Arc::new(RecordingSink::default());
    let handler = FakeHandler::new("checkpoint", true, |_, context| {
        Box::pin(async move {
            assert_eq!(
                context
                    .checkpoint(Some("not-json"), 40, None)
                    .unwrap_err()
                    .code(),
                "invalid_task"
            );
            context
                .checkpoint(Some(r#"{"step":"persisted"}"#), 55, None)
                .unwrap();
            Ok(HandlerOutcome::Completed)
        })
    });
    let mut scheduler = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        vec![Arc::new(handler)],
        sink.clone(),
        scheduler_config(1),
    );

    drive_until(&mut scheduler, || {
        repository::get(&database.connect(), "workspace-a", created.id)
            .unwrap()
            .is_some_and(|task| task.state == TaskState::Completed)
    });

    let events = sink.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    let SchedulerEvent::Progress(event) = &events[0];
    assert_eq!(event.progress, 55);
    let stored = repository::get(&database.connect(), "workspace-a", created.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.progress, 100);
}

#[test]
fn scheduler_uses_fake_clock_for_capped_exponential_retry() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created = repository::create(&mut connection, scheduler_task("retry")).unwrap();
    drop(connection);
    let handler = FakeHandler::new("retry", true, |task, _| {
        Box::pin(async move {
            if task.attempt < 4 {
                Err(HandlerError::retryable("upstream_timeout"))
            } else {
                Ok(HandlerOutcome::Completed)
            }
        })
    });
    let clock = Arc::new(FakeClock::new());
    let mut scheduler = scheduler(
        &database,
        Arc::clone(&clock),
        vec![Arc::new(handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    );

    for (attempt, delay) in [(1, 10), (2, 20), (3, 40)] {
        scheduler.tick().unwrap();
        drive_until(&mut scheduler, || {
            repository::get(&database.connect(), "workspace-a", created.id)
                .unwrap()
                .is_some_and(|task| task.state == TaskState::Queued && task.attempt == attempt)
        });
        let record = repository::get(&database.connect(), "workspace-a", created.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            DateTime::parse_from_rfc3339(record.next_run_at.as_deref().unwrap())
                .unwrap()
                .with_timezone(&Utc),
            clock.now() + chrono::Duration::seconds(delay)
        );
        scheduler.tick().unwrap();
        assert_eq!(
            repository::get(&database.connect(), "workspace-a", created.id)
                .unwrap()
                .unwrap()
                .attempt,
            attempt
        );
        clock.advance(chrono::Duration::seconds(delay));
    }

    drive_until(&mut scheduler, || {
        repository::get(&database.connect(), "workspace-a", created.id)
            .unwrap()
            .is_some_and(|task| task.state == TaskState::Completed)
    });
    assert_eq!(
        repository::get(&database.connect(), "workspace-a", created.id)
            .unwrap()
            .unwrap()
            .attempt,
        4
    );
}

#[test]
fn scheduler_never_retries_permanent_or_unknown_task_errors() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let permanent = repository::create(&mut connection, scheduler_task("permanent")).unwrap();
    let unknown = repository::create(&mut connection, scheduler_task("unknown")).unwrap();
    drop(connection);
    let handler = FakeHandler::new("permanent", true, |_, _| {
        Box::pin(async { Err(HandlerError::permanent("invalid_input")) })
    });
    let clock = Arc::new(FakeClock::new());
    let mut scheduler = scheduler(
        &database,
        Arc::clone(&clock),
        vec![Arc::new(handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(2),
    );

    drive_until(&mut scheduler, || {
        repository::list(&database.connect(), "workspace-a")
            .unwrap()
            .iter()
            .all(|task| task.state == TaskState::Failed)
    });
    clock.advance(chrono::Duration::days(1));
    scheduler.tick().unwrap();

    let permanent = repository::get(&database.connect(), "workspace-a", permanent.id)
        .unwrap()
        .unwrap();
    assert_eq!(permanent.attempt, 1);
    assert_eq!(permanent.error_code.as_deref(), Some("invalid_input"));
    let unknown = repository::get(&database.connect(), "workspace-a", unknown.id)
        .unwrap()
        .unwrap();
    assert_eq!(unknown.attempt, 1);
    assert_eq!(unknown.error_code.as_deref(), Some("unknown_task_kind"));
}

#[test]
fn scheduler_with_no_handlers_leaves_queued_tasks_unclaimed() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created = repository::create(&mut connection, scheduler_task("future-handler")).unwrap();
    drop(connection);

    let mut scheduler = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        Vec::new(),
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    );
    scheduler.tick().unwrap();

    let persisted = repository::get(&database.connect(), "workspace-a", created.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.state, TaskState::Queued);
    assert_eq!(persisted.attempt, 0);
    assert_eq!(persisted.error_code, None);
}

#[test]
fn scheduler_cooperatively_cancels_active_work_without_failure() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created = repository::create(&mut connection, scheduler_task("cancel")).unwrap();
    drop(connection);
    let started = Arc::new((Mutex::new(0usize), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let returned = Arc::new(AtomicUsize::new(0));
    let handler = FakeHandler::new("cancel", true, {
        let started = Arc::clone(&started);
        let release = Arc::clone(&release);
        let returned = Arc::clone(&returned);
        move |_, context| {
            let started = Arc::clone(&started);
            let release = Arc::clone(&release);
            let returned = Arc::clone(&returned);
            Box::pin(async move {
                {
                    let mut count = started.0.lock().unwrap();
                    *count += 1;
                    started.1.notify_all();
                }
                wait_for_release(&release);
                assert!(context.cancellation_requested().unwrap());
                returned.fetch_add(1, Ordering::SeqCst);
                Ok(HandlerOutcome::Cancelled)
            })
        }
    });
    let mut scheduler = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        vec![Arc::new(handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    );

    scheduler.tick().unwrap();
    wait_for_count(&started, 1);
    repository::request_cancellation(&mut database.connect(), "workspace-a", created.id).unwrap();
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    drive_until(&mut scheduler, || {
        repository::get(&database.connect(), "workspace-a", created.id)
            .unwrap()
            .is_some_and(|task| task.state == TaskState::Cancelled)
    });
    let cancelled = repository::get(&database.connect(), "workspace-a", created.id)
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.error_code, None);
    assert_eq!(cancelled.attempt, 1);
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    drive_until(&mut scheduler, || returned.load(Ordering::SeqCst) == 1);
}

#[test]
fn scheduler_shutdown_interrupts_active_work_and_fences_stale_handler() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let active_task = repository::create(&mut connection, scheduler_task("shutdown")).unwrap();
    let queued_task = repository::create(
        &mut connection,
        NewTask {
            next_run_at: Some("2099-01-01T00:00:00Z".into()),
            ..scheduler_task("shutdown")
        },
    )
    .unwrap();
    drop(connection);
    let started = Arc::new((Mutex::new(0usize), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let finished = Arc::new(AtomicUsize::new(0));
    let handler = FakeHandler::new("shutdown", true, {
        let started = Arc::clone(&started);
        let release = Arc::clone(&release);
        let finished = Arc::clone(&finished);
        move |_, context| {
            let started = Arc::clone(&started);
            let release = Arc::clone(&release);
            let finished = Arc::clone(&finished);
            Box::pin(async move {
                {
                    let mut count = started.0.lock().unwrap();
                    *count += 1;
                    started.1.notify_all();
                }
                wait_for_release(&release);
                assert!(context.shutdown_requested());
                finished.fetch_add(1, Ordering::SeqCst);
                Ok(HandlerOutcome::Completed)
            })
        }
    });
    let mut scheduler = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        vec![Arc::new(handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    );

    scheduler.tick().unwrap();
    wait_for_count(&started, 1);
    let control = scheduler.control();
    control.request_shutdown();
    scheduler.tick().unwrap();
    let interrupted = repository::get(&database.connect(), "workspace-a", active_task.id)
        .unwrap()
        .unwrap();
    assert_eq!(interrupted.state, TaskState::Interrupted);
    assert_eq!(
        repository::get(&database.connect(), "workspace-a", queued_task.id)
            .unwrap()
            .unwrap()
            .state,
        TaskState::Queued
    );

    let retried = repository::transition(
        &mut database.connect(),
        "workspace-a",
        active_task.id,
        interrupted.attempt,
        TaskState::Interrupted,
        TaskState::Queued,
        None,
    )
    .unwrap();
    let reclaimed = repository::claim_next(
        &mut database.connect(),
        "workspace-a",
        &Utc::now().to_rfc3339(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(reclaimed.id, retried.id);
    assert_eq!(reclaimed.attempt, interrupted.attempt + 1);

    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    drive_until(&mut scheduler, || finished.load(Ordering::SeqCst) == 1);
    scheduler.tick().unwrap();
    let current = repository::get(&database.connect(), "workspace-a", active_task.id)
        .unwrap()
        .unwrap();
    assert_eq!(current.state, TaskState::Running);
    assert_eq!(current.attempt, reclaimed.attempt);
}

#[test]
fn scheduler_tick_error_durably_interrupts_the_finished_claim() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created = repository::create(&mut connection, scheduler_task("tick-error")).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER abort_scheduler_completion
             BEFORE UPDATE OF state ON background_tasks
             WHEN NEW.state = 'completed'
             BEGIN
               SELECT RAISE(ABORT, 'completion write failed');
             END;",
        )
        .unwrap();
    drop(connection);

    let handler = FakeHandler::new("tick-error", true, |_, _| {
        Box::pin(async { Ok(HandlerOutcome::Completed) })
    });
    let scheduler = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        vec![Arc::new(handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    );
    let handle = scheduler.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_stopped() {
        assert!(Instant::now() < deadline, "scheduler shutdown timed out");
        thread::yield_now();
    }
    assert!(handle.shutdown(Duration::from_secs(1)));

    let persisted = repository::get(&database.connect(), "workspace-a", created.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.state, TaskState::Interrupted);
}

#[test]
fn scheduler_unknown_claim_is_durably_interrupted_when_failure_write_errors() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created = repository::create(&mut connection, scheduler_task("unknown-durable")).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE scheduler_test_claims (task_id TEXT PRIMARY KEY);
             CREATE TRIGGER record_unknown_claim
             AFTER UPDATE OF state ON background_tasks
             WHEN OLD.state = 'queued' AND NEW.state = 'running'
               AND NEW.kind = 'unknown-durable'
             BEGIN
               INSERT INTO scheduler_test_claims (task_id) VALUES (NEW.id);
             END;
             CREATE TRIGGER abort_unknown_failure
             BEFORE UPDATE OF state ON background_tasks
             WHEN OLD.state = 'running' AND NEW.state = 'failed'
               AND NEW.error_code = 'unknown_task_kind'
             BEGIN
               SELECT RAISE(ABORT, 'unknown failure write failed');
             END;",
        )
        .unwrap();
    drop(connection);

    let known_handler = FakeHandler::new("known", true, |_, _| {
        Box::pin(async { panic!("known handler must not execute") })
    });
    let handle = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        vec![Arc::new(known_handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    )
    .start()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let claimed = database
            .connect()
            .query_row(
                "SELECT COUNT(*) FROM scheduler_test_claims WHERE task_id = ?1",
                [created.id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            == 1;
        if claimed {
            break;
        }
        assert!(Instant::now() < deadline, "unknown task claim timed out");
        thread::yield_now();
    }

    assert!(handle.shutdown(Duration::from_secs(2)));
    let persisted = repository::get(&database.connect(), "workspace-a", created.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.attempt, 1);
    assert_eq!(persisted.state, TaskState::Interrupted);
}

#[test]
fn scheduler_shutdown_reports_false_until_interrupt_is_durable() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created =
        repository::create(&mut connection, scheduler_task("interrupt-write-failure")).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER abort_scheduler_finish
             BEFORE UPDATE OF state ON background_tasks
             WHEN OLD.state = 'running'
               AND NEW.state IN ('completed', 'interrupted', 'cancelled')
             BEGIN
               SELECT RAISE(ABORT, 'finish write failed');
             END;",
        )
        .unwrap();
    drop(connection);

    let handler = FakeHandler::new("interrupt-write-failure", true, |_, _| {
        Box::pin(async { Ok(HandlerOutcome::Completed) })
    });
    let handle = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        vec![Arc::new(handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    )
    .start()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let running = repository::get(&database.connect(), "workspace-a", created.id)
            .unwrap()
            .is_some_and(|task| task.state == TaskState::Running);
        if running {
            break;
        }
        assert!(Instant::now() < deadline, "task claim timed out");
        thread::yield_now();
    }

    assert!(!handle.shutdown(Duration::from_millis(100)));
    database
        .connect()
        .execute_batch("DROP TRIGGER abort_scheduler_finish")
        .unwrap();
    assert!(handle.shutdown(Duration::from_secs(2)));

    let persisted = repository::get(&database.connect(), "workspace-a", created.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.state, TaskState::Interrupted);
}

#[test]
fn scheduler_state_starts_once_and_shutdown_wait_is_bounded_and_durable() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created = repository::create(&mut connection, scheduler_task("background")).unwrap();
    drop(connection);
    let started = Arc::new((Mutex::new(0usize), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let handler = FakeHandler::new("background", true, {
        let started = Arc::clone(&started);
        let release = Arc::clone(&release);
        move |_, context| {
            let started = Arc::clone(&started);
            let release = Arc::clone(&release);
            Box::pin(async move {
                {
                    let mut count = started.0.lock().unwrap();
                    *count += 1;
                    started.1.notify_all();
                }
                wait_for_release(&release);
                assert!(context.shutdown_requested());
                Ok(HandlerOutcome::Interrupted)
            })
        }
    });
    let state = SchedulerState::default();
    assert!(state
        .start(scheduler(
            &database,
            Arc::new(FakeClock::new()),
            vec![Arc::new(handler)],
            Arc::new(RecordingSink::default()),
            scheduler_config(1),
        ))
        .unwrap());
    assert!(!state
        .start(scheduler(
            &database,
            Arc::new(FakeClock::new()),
            Vec::new(),
            Arc::new(RecordingSink::default()),
            scheduler_config(1),
        ))
        .unwrap());
    wait_for_count(&started, 1);

    assert!(state.shutdown(Duration::from_secs(2)));
    let interrupted = repository::get(&database.connect(), "workspace-a", created.id)
        .unwrap()
        .unwrap();
    assert_eq!(interrupted.state, TaskState::Interrupted);

    *release.0.lock().unwrap() = true;
    release.1.notify_all();

    assert!(state
        .start(scheduler(
            &database,
            Arc::new(FakeClock::new()),
            Vec::new(),
            Arc::new(RecordingSink::default()),
            scheduler_config(1),
        ))
        .unwrap());
    assert!(state.shutdown(Duration::from_secs(2)));
}

#[test]
fn scheduler_restart_recovers_only_resumable_interrupted_tasks() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let resumable = repository::create(
        &mut connection,
        NewTask {
            next_run_at: Some("2000-01-01T00:00:00Z".into()),
            ..scheduler_task("resumable")
        },
    )
    .unwrap();
    let non_resumable = repository::create(
        &mut connection,
        NewTask {
            next_run_at: Some("2000-01-02T00:00:00Z".into()),
            ..scheduler_task("non_resumable")
        },
    )
    .unwrap();
    let resumable_claim = repository::claim_next(&mut connection, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    let non_resumable_claim = repository::claim_next(&mut connection, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    repository::checkpoint(
        &mut connection,
        "workspace-a",
        resumable.id,
        resumable_claim.attempt,
        Some(r#"{"step":"resume"}"#),
        61,
        None,
    )
    .unwrap();
    repository::checkpoint(
        &mut connection,
        "workspace-a",
        non_resumable.id,
        non_resumable_claim.attempt,
        Some(r#"{"step":"manual"}"#),
        72,
        None,
    )
    .unwrap();
    drop(connection);

    let resumable_handler = FakeHandler::new("resumable", true, |task, _| {
        Box::pin(async move {
            assert_eq!(
                task.checkpoint_json.as_deref(),
                Some(r#"{"step":"resume"}"#)
            );
            assert_eq!(task.progress, 61);
            Ok(HandlerOutcome::Completed)
        })
    });
    let non_resumable_handler = FakeHandler::new("non_resumable", false, |_, _| {
        Box::pin(async { panic!("non-resumable task must not execute") })
    });
    let mut scheduler = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        vec![Arc::new(resumable_handler), Arc::new(non_resumable_handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    );

    scheduler.recover().unwrap();
    let resumable_recovered = repository::get(&database.connect(), "workspace-a", resumable.id)
        .unwrap()
        .unwrap();
    assert_eq!(resumable_recovered.state, TaskState::Queued);
    assert_eq!(resumable_recovered.progress, 61);
    assert_eq!(
        resumable_recovered.checkpoint_json.as_deref(),
        Some(r#"{"step":"resume"}"#)
    );
    let non_resumable_recovered =
        repository::get(&database.connect(), "workspace-a", non_resumable.id)
            .unwrap()
            .unwrap();
    assert_eq!(non_resumable_recovered.state, TaskState::Interrupted);
    assert_eq!(non_resumable_recovered.progress, 72);

    drive_until(&mut scheduler, || {
        repository::get(&database.connect(), "workspace-a", resumable.id)
            .unwrap()
            .is_some_and(|task| task.state == TaskState::Completed)
    });
    assert_eq!(
        repository::get(&database.connect(), "workspace-a", resumable.id)
            .unwrap()
            .unwrap()
            .attempt,
        resumable_claim.attempt + 1
    );
}

#[test]
fn scheduler_reclaims_due_waiting_external_only_through_atomic_claim() {
    let database = TestDatabase::new();
    let mut connection = database.connect();
    let created = repository::create(&mut connection, scheduler_task("external")).unwrap();
    let running = repository::claim_next(&mut connection, "workspace-a", NOW)
        .unwrap()
        .unwrap();
    repository::checkpoint(
        &mut connection,
        "workspace-a",
        created.id,
        running.attempt,
        Some(r#"{"remote_id":"job-1"}"#),
        45,
        Some(NOW),
    )
    .unwrap();
    let waiting = repository::transition(
        &mut connection,
        "workspace-a",
        created.id,
        running.attempt,
        TaskState::Running,
        TaskState::WaitingExternal,
        None,
    )
    .unwrap();
    assert_eq!(
        repository::transition(
            &mut connection,
            "workspace-a",
            created.id,
            waiting.attempt,
            TaskState::WaitingExternal,
            TaskState::Running,
            None,
        )
        .unwrap_err()
        .code(),
        "claim_required"
    );
    drop(connection);

    let handler = FakeHandler::new("external", true, |task, _| {
        Box::pin(async move {
            assert_eq!(
                task.checkpoint_json.as_deref(),
                Some(r#"{"remote_id":"job-1"}"#)
            );
            Ok(HandlerOutcome::Completed)
        })
    });
    let mut scheduler = scheduler(
        &database,
        Arc::new(FakeClock::new()),
        vec![Arc::new(handler)],
        Arc::new(RecordingSink::default()),
        scheduler_config(1),
    );
    drive_until(&mut scheduler, || {
        repository::get(&database.connect(), "workspace-a", created.id)
            .unwrap()
            .is_some_and(|task| task.state == TaskState::Completed)
    });
    assert_eq!(
        repository::get(&database.connect(), "workspace-a", created.id)
            .unwrap()
            .unwrap()
            .attempt,
        running.attempt + 1
    );
}
