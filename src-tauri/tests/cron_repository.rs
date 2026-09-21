use bloomery::storage::migrations::migrate;
use bloomery::tasks::cron_repository::{acknowledge, pending, schedule, tick};
use chrono::{TimeZone, Utc};
use rusqlite::Connection;

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
