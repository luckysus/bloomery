use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use suna::storage::migrations::migrate;
use suna::tasks::cron_repository::{self, acknowledge, pending, schedule, tick, SaveAgentSchedule};
use uuid::Uuid;

#[test]
fn tick_persists_one_shot_event_and_acknowledges_it() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap();
    let job = schedule(
        &connection,
        "local",
        "1 9 * * *",
        "Asia/Shanghai",
        "run tests",
        "lead",
        false,
        true,
        now,
    )
    .unwrap();
    let events = tick(
        &mut connection,
        "local",
        now + chrono::Duration::hours(2),
        10,
    )
    .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].job_id, job.id);
    assert_eq!(pending(&connection, "local").unwrap().len(), 1);
    assert!(acknowledge(&connection, "local", events[0].event_id).unwrap());
    assert!(!acknowledge(&connection, "local", events[0].event_id).unwrap());
    assert!(pending(&connection, "local").unwrap().is_empty());
}

#[test]
fn recurring_tick_advances_and_capacity_is_bounded() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap();
    schedule(
        &connection,
        "local",
        "* * * * *",
        "UTC",
        "check",
        "lead",
        true,
        true,
        now,
    )
    .unwrap();
    assert_eq!(
        tick(
            &mut connection,
            "local",
            now + chrono::Duration::minutes(2),
            1
        )
        .unwrap()
        .len(),
        1
    );
    assert_eq!(pending(&connection, "local").unwrap().len(), 1);
    assert_eq!(
        tick(
            &mut connection,
            "local",
            now + chrono::Duration::minutes(3),
            1
        )
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn agent_schedule_persists_slot_identity_and_controls_unstarted_work() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 10, 9, 0, 0, 0).unwrap();
    let conversation = Uuid::new_v4();
    connection
        .execute(
            "INSERT INTO conversations (id, workspace_id, title, created_at, updated_at)
        VALUES (?1, 'local', 'scheduled', ?2, ?2)",
            rusqlite::params![conversation.to_string(), now.to_rfc3339()],
        )
        .unwrap();
    let request = SaveAgentSchedule {
        id: None,
        conversation_id: conversation,
        agent_id: "knowledge".into(),
        expression: "* * * * *".into(),
        timezone: "Asia/Shanghai".into(),
        prompt: "summarize".into(),
        enabled: true,
        recurring: true,
    };
    assert!(cron_repository::save(&mut connection, "other", request.clone(), now).is_err());
    let job = cron_repository::save(&mut connection, "local", request, now).unwrap();
    let due = now + chrono::Duration::minutes(1);
    let events = tick(&mut connection, "local", due, 10).unwrap();
    assert_eq!(events[0].conversation_id, Some(conversation));
    assert_eq!(events[0].agent_id, "knowledge");
    assert!(tick(&mut connection, "local", due, 10).unwrap().is_empty());
    let event = events[0].event_id;
    let run = cron_repository::reserve_run(&connection, "local", event).unwrap();
    assert_eq!(run, event);
    assert_eq!(
        cron_repository::reserve_run(&connection, "local", event).unwrap(),
        run
    );
    assert_eq!(pending(&connection, "local").unwrap()[0].run_id, Some(run));
    let message = Uuid::new_v4();
    connection
        .execute(
            "INSERT INTO messages (id, workspace_id, conversation_id, role, content, created_at)
        VALUES (?1, 'local', ?2, 'user', 'scheduled', ?3)",
            rusqlite::params![
                message.to_string(),
                conversation.to_string(),
                now.to_rfc3339()
            ],
        )
        .unwrap();
    connection.execute("INSERT INTO agent_runs (id, workspace_id, conversation_id, user_message_id, state, created_at, updated_at)
        VALUES (?1, 'local', ?2, ?3, 'generating', ?4, ?4)", rusqlite::params![run.to_string(),conversation.to_string(),message.to_string(),now.to_rfc3339()]).unwrap();
    cron_repository::set_enabled(&mut connection, "local", job.id, false, due).unwrap();
    assert_eq!(
        pending(&connection, "local").unwrap().len(),
        1,
        "disabling must preserve already reserved runs"
    );
    assert!(tick(
        &mut connection,
        "local",
        due + chrono::Duration::hours(1),
        10
    )
    .unwrap()
    .is_empty());
    cron_repository::record_error(&connection, "local", event, "provider unavailable").unwrap();
    assert_eq!(
        cron_repository::list(&connection, "local").unwrap()[0]
            .last_error
            .as_deref(),
        Some("provider unavailable")
    );
    cron_repository::set_enabled(&mut connection, "local", job.id, true, due).unwrap();
    tick(
        &mut connection,
        "local",
        due + chrono::Duration::minutes(1),
        10,
    )
    .unwrap();
    assert_eq!(pending(&connection, "local").unwrap().len(), 1);
    let next_event = pending(&connection, "local").unwrap()[0].event_id;
    cron_repository::reserve_run(&connection, "local", next_event).unwrap();
    cron_repository::delete(&mut connection, "local", job.id).unwrap();
    assert!(pending(&connection, "local").unwrap().is_empty());
    assert!(cron_repository::list(&connection, "local")
        .unwrap()
        .is_empty());
}
