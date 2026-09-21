use bloomery::storage::migrations::migrate;
use bloomery::tasks::model::{NewTask, TaskState};
use bloomery::tasks::repository;
use bloomery::tasks::work_stealing::{claim_next, complete};
use chrono::{Duration, TimeZone, Utc};
use rusqlite::Connection;

#[test]
fn claim_uses_owner_token_and_lease() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let task = repository::create(
        &connection,
        NewTask {
            workspace_id: "local".to_string(),
            kind: "shared".to_string(),
            payload_json: "{}".to_string(),
            checkpoint_json: None,
            next_run_at: None,
            progress: 0,
        },
    )
    .unwrap();
    let now = Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap();
    let claim = claim_next(&mut connection, "local", "alice", now, Duration::minutes(1))
        .unwrap()
        .unwrap();
    assert!(
        claim_next(&mut connection, "local", "bob", now, Duration::minutes(1))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        complete(
            &mut connection,
            "local",
            task.id,
            "alice",
            claim.claim_token,
            now + Duration::seconds(30)
        )
        .unwrap()
        .state,
        TaskState::Completed
    );
}
