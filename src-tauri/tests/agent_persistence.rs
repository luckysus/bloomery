use bloomery::agent::protocol::{
    AgentEventData, AgentMessageRole, AgentRunState, MessageDelta, RunCompleted, RunOutcome,
    UsageUpdated,
};
use bloomery::storage::database;
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::{conversations, events, runs};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread;
use uuid::Uuid;

const WORKSPACE: &str = "local";
const CONVERSATION_ID: &str = "11111111-1111-4111-8111-111111111111";
const USER_MESSAGE_ID: &str = "22222222-2222-4222-8222-222222222222";
const ASSISTANT_MESSAGE_ID: &str = "33333333-3333-4333-8333-333333333333";
const RUN_ID: &str = "44444444-4444-4444-8444-444444444444";
const CREATED_EVENT_ID: &str = "55555555-5555-4555-8555-555555555555";
const DELTA_EVENT_ID: &str = "66666666-6666-4666-8666-666666666666";
const USAGE_EVENT_ID: &str = "77777777-7777-4777-8777-777777777777";
const COMPLETED_EVENT_ID: &str = "88888888-8888-4888-8888-888888888888";
const CREATED_AT: &str = "2026-08-03T09:00:00Z";
const COMPLETED_AT: &str = "2026-08-03T09:01:00Z";

#[test]
fn run_creation_and_first_event_are_atomic() {
    let mut connection = setup();

    let created = runs::create(
        &mut connection,
        new_run(RUN_ID, CREATED_EVENT_ID, CREATED_AT),
    )
    .unwrap();

    assert_eq!(created.run.id, id(RUN_ID));
    assert_eq!(created.run.workspace_id, WORKSPACE);
    assert_eq!(created.run.conversation_id, id(CONVERSATION_ID));
    assert_eq!(created.run.state, AgentRunState::Created);
    assert_eq!(created.run.next_sequence, 2);
    assert_eq!(created.event.sequence, 1);
    assert!(matches!(created.event.data, AgentEventData::RunCreated(_)));

    let second_run_id = "99999999-9999-4999-8999-999999999999";
    let error = runs::create(
        &mut connection,
        new_run(second_run_id, CREATED_EVENT_ID, CREATED_AT),
    )
    .unwrap_err();

    assert_eq!(error.code(), "agent_event_duplicate");
    assert!(runs::get(&connection, WORKSPACE, id(second_run_id))
        .unwrap()
        .is_none());
    assert_eq!(count(&connection, "agent_runs"), 1);
    assert_eq!(count(&connection, "agent_run_events"), 1);
}

#[test]
fn run_creation_can_join_the_callers_message_transaction() {
    let mut connection = setup();
    connection.execute("DELETE FROM messages", []).unwrap();
    let transaction = connection.transaction().unwrap();
    transaction
        .execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, 'user', 'Explain controlled cooling', NULL, ?4)",
            params![USER_MESSAGE_ID, WORKSPACE, CONVERSATION_ID, CREATED_AT],
        )
        .unwrap();

    let created =
        runs::create_in_transaction(&transaction, new_run(RUN_ID, CREATED_EVENT_ID, CREATED_AT))
            .unwrap();
    assert_eq!(created.event.sequence, 1);
    transaction.rollback().unwrap();

    assert_eq!(count(&connection, "messages"), 0);
    assert_eq!(count(&connection, "agent_runs"), 0);
    assert_eq!(count(&connection, "agent_run_events"), 0);
}

#[test]
fn event_allocation_is_monotonic_and_duplicate_ids_do_not_consume_sequences() {
    let mut connection = setup_with_run();

    let delta = events::append(
        &mut connection,
        WORKSPACE,
        id(RUN_ID),
        id(DELTA_EVENT_ID),
        timestamp("2026-08-03T09:00:01Z"),
        AgentEventData::MessageDelta(MessageDelta {
            message_id: id(ASSISTANT_MESSAGE_ID),
            role: AgentMessageRole::Assistant,
            delta: "Q355".to_string(),
        }),
    )
    .unwrap();
    assert_eq!(delta.sequence, 2);

    let duplicate = events::append(
        &mut connection,
        WORKSPACE,
        id(RUN_ID),
        id(DELTA_EVENT_ID),
        timestamp("2026-08-03T09:00:02Z"),
        AgentEventData::UsageUpdated(UsageUpdated {
            prompt_tokens: 10,
            completion_tokens: 2,
            total_tokens: 12,
        }),
    )
    .unwrap_err();
    assert_eq!(duplicate.code(), "agent_event_duplicate");

    let usage = events::append(
        &mut connection,
        WORKSPACE,
        id(RUN_ID),
        id(USAGE_EVENT_ID),
        timestamp("2026-08-03T09:00:03Z"),
        AgentEventData::UsageUpdated(UsageUpdated {
            prompt_tokens: 10,
            completion_tokens: 2,
            total_tokens: 12,
        }),
    )
    .unwrap();
    assert_eq!(usage.sequence, 3);
    assert_eq!(
        runs::get(&connection, WORKSPACE, id(RUN_ID))
            .unwrap()
            .unwrap()
            .next_sequence,
        4
    );
}

#[test]
fn generic_event_append_rejects_run_state_events_without_consuming_a_sequence() {
    let mut connection = setup_with_run();

    let error = events::append(
        &mut connection,
        WORKSPACE,
        id(RUN_ID),
        id(COMPLETED_EVENT_ID),
        timestamp(COMPLETED_AT),
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Completed,
            assistant_message_id: Some(id(ASSISTANT_MESSAGE_ID)),
        }),
    )
    .unwrap_err();
    assert_eq!(error.code(), "agent_state_event_requires_run_repository");

    let usage = events::append(
        &mut connection,
        WORKSPACE,
        id(RUN_ID),
        id(COMPLETED_EVENT_ID),
        timestamp(COMPLETED_AT),
        AgentEventData::UsageUpdated(UsageUpdated {
            prompt_tokens: 10,
            completion_tokens: 2,
            total_tokens: 12,
        }),
    )
    .unwrap();
    assert_eq!(usage.sequence, 2);
}

#[test]
fn replay_is_ordered_incremental_and_workspace_scoped() {
    let mut connection = setup_with_run();
    append_delta(&mut connection);
    events::append(
        &mut connection,
        WORKSPACE,
        id(RUN_ID),
        id(USAGE_EVENT_ID),
        timestamp("2026-08-03T09:00:03Z"),
        AgentEventData::UsageUpdated(UsageUpdated {
            prompt_tokens: 10,
            completion_tokens: 2,
            total_tokens: 12,
        }),
    )
    .unwrap();

    let all = events::replay(&connection, WORKSPACE, id(RUN_ID), 0).unwrap();
    assert_eq!(
        all.iter().map(|event| event.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let incremental = events::replay(&connection, WORKSPACE, id(RUN_ID), 1).unwrap();
    assert_eq!(
        incremental
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(
        events::replay(&connection, "other-workspace", id(RUN_ID), 0)
            .unwrap_err()
            .code(),
        "agent_run_not_found"
    );
}

#[test]
fn submillisecond_timestamps_survive_persistence_and_replay() {
    let mut connection = setup();
    let precise = "2026-08-03T09:00:00.123456789Z";
    let created =
        runs::create(&mut connection, new_run(RUN_ID, CREATED_EVENT_ID, precise)).unwrap();

    let replayed = events::replay(&connection, WORKSPACE, id(RUN_ID), 0).unwrap();

    assert_eq!(replayed, vec![created.event]);
    assert_eq!(replayed[0].timestamp, timestamp(precise));
}

#[test]
fn completion_updates_run_and_appends_terminal_event_atomically() {
    let mut connection = setup_with_run();
    set_completing(&connection);

    let completed = runs::complete(
        &mut connection,
        WORKSPACE,
        id(RUN_ID),
        id(COMPLETED_EVENT_ID),
        timestamp(COMPLETED_AT),
        RunOutcome::Completed,
        Some(id(ASSISTANT_MESSAGE_ID)),
    )
    .unwrap();

    assert_eq!(completed.run.state, AgentRunState::Completed);
    assert_eq!(completed.run.completed_at, Some(timestamp(COMPLETED_AT)));
    assert_eq!(completed.run.next_sequence, 3);
    assert_eq!(completed.event.sequence, 2);
    assert!(matches!(
        completed.event.data,
        AgentEventData::RunCompleted(_)
    ));
    assert_eq!(
        runs::complete(
            &mut connection,
            WORKSPACE,
            id(RUN_ID),
            Uuid::new_v4(),
            timestamp(COMPLETED_AT),
            RunOutcome::Completed,
            Some(id(ASSISTANT_MESSAGE_ID)),
        )
        .unwrap_err()
        .code(),
        "agent_run_not_completing"
    );
}

#[test]
fn completion_rolls_back_state_and_sequence_when_event_insert_fails() {
    let mut connection = setup_with_run();
    set_completing(&connection);
    connection
        .execute_batch(
            "CREATE TRIGGER reject_terminal_event
             BEFORE INSERT ON agent_run_events
             WHEN NEW.sequence = 2
             BEGIN
               SELECT RAISE(ABORT, 'injected event failure');
             END;",
        )
        .unwrap();

    let error = runs::complete(
        &mut connection,
        WORKSPACE,
        id(RUN_ID),
        id(COMPLETED_EVENT_ID),
        timestamp(COMPLETED_AT),
        RunOutcome::Completed,
        Some(id(ASSISTANT_MESSAGE_ID)),
    )
    .unwrap_err();

    assert_eq!(error.code(), "agent_event_storage_failed");
    let run = runs::get(&connection, WORKSPACE, id(RUN_ID))
        .unwrap()
        .unwrap();
    assert_eq!(run.state, AgentRunState::Completing);
    assert_eq!(run.next_sequence, 2);
    assert_eq!(run.completed_at, None);
    assert_eq!(count(&connection, "agent_run_events"), 1);
}

#[test]
fn deleting_a_conversation_cascades_its_runs_and_events() {
    let mut connection = setup_with_run();

    conversations::delete(&mut connection, WORKSPACE, CONVERSATION_ID).unwrap();

    assert_eq!(count(&connection, "agent_runs"), 0);
    assert_eq!(count(&connection, "agent_run_events"), 0);
}

#[test]
fn schema_rejects_cross_workspace_runs_and_mismatched_event_conversations() {
    let mut connection = setup();
    let cross_workspace = connection.execute(
        "INSERT INTO agent_runs
         (id, workspace_id, conversation_id, user_message_id, state,
          next_sequence, created_at, updated_at, completed_at)
         VALUES (?1, 'other-workspace', ?2, ?3, 'created', 1, ?4, ?4, NULL)",
        params![RUN_ID, CONVERSATION_ID, USER_MESSAGE_ID, CREATED_AT],
    );
    assert!(cross_workspace.is_err());

    runs::create(
        &mut connection,
        new_run(RUN_ID, CREATED_EVENT_ID, CREATED_AT),
    )
    .unwrap();
    let other_conversation = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    connection
        .execute(
            "INSERT INTO conversations
             (id, workspace_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, 'Other', ?3, ?3, 0, 0)",
            params![other_conversation, WORKSPACE, CREATED_AT],
        )
        .unwrap();
    let mismatched_conversation = connection.execute(
        "INSERT INTO agent_run_events
         (event_id, workspace_id, run_id, conversation_id, sequence,
          protocol_version, timestamp, event_json)
         VALUES (?1, ?2, ?3, ?4, 2, 1, ?5, '{}')",
        params![
            DELTA_EVENT_ID,
            WORKSPACE,
            RUN_ID,
            other_conversation,
            CREATED_AT
        ],
    );
    assert!(mismatched_conversation.is_err());
}

#[test]
fn concurrent_connections_allocate_distinct_monotonic_sequences() {
    let path =
        std::env::temp_dir().join(format!("bloomery-agent-events-{}.sqlite3", Uuid::new_v4()));
    let (mut first, _) = database::open(&path).unwrap();
    seed_conversation(&first);
    runs::create(&mut first, new_run(RUN_ID, CREATED_EVENT_ID, CREATED_AT)).unwrap();
    let (mut second, _) = database::open(&path).unwrap();
    let barrier = Arc::new(Barrier::new(3));

    let first_barrier = Arc::clone(&barrier);
    let first_append = thread::spawn(move || {
        first_barrier.wait();
        events::append(
            &mut first,
            WORKSPACE,
            id(RUN_ID),
            id(DELTA_EVENT_ID),
            timestamp("2026-08-03T09:00:01.000000001Z"),
            AgentEventData::UsageUpdated(UsageUpdated {
                prompt_tokens: 10,
                completion_tokens: 2,
                total_tokens: 12,
            }),
        )
        .unwrap()
        .sequence
    });
    let second_barrier = Arc::clone(&barrier);
    let second_append = thread::spawn(move || {
        second_barrier.wait();
        events::append(
            &mut second,
            WORKSPACE,
            id(RUN_ID),
            id(USAGE_EVENT_ID),
            timestamp("2026-08-03T09:00:01.000000002Z"),
            AgentEventData::UsageUpdated(UsageUpdated {
                prompt_tokens: 20,
                completion_tokens: 4,
                total_tokens: 24,
            }),
        )
        .unwrap()
        .sequence
    });
    barrier.wait();
    let mut sequences = vec![first_append.join().unwrap(), second_append.join().unwrap()];
    sequences.sort_unstable();

    let (verification, _) = database::open(&path).unwrap();
    assert_eq!(sequences, vec![2, 3]);
    assert_eq!(
        runs::get(&verification, WORKSPACE, id(RUN_ID))
            .unwrap()
            .unwrap()
            .next_sequence,
        4
    );
    drop(verification);
    remove_sqlite_files(&path);
}

fn setup() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    migrate(&mut connection).unwrap();
    seed_conversation(&connection);
    connection
}

fn seed_conversation(connection: &Connection) {
    connection
        .execute(
            "INSERT INTO conversations
             (id, workspace_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, 'Agent persistence', ?3, ?3, 0, 0)",
            params![CONVERSATION_ID, WORKSPACE, CREATED_AT],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, 'user', 'Explain controlled cooling', NULL, ?4)",
            params![USER_MESSAGE_ID, WORKSPACE, CONVERSATION_ID, CREATED_AT],
        )
        .unwrap();
}

fn setup_with_run() -> Connection {
    let mut connection = setup();
    runs::create(
        &mut connection,
        new_run(RUN_ID, CREATED_EVENT_ID, CREATED_AT),
    )
    .unwrap();
    connection
}

fn new_run(run_id: &str, event_id: &str, created_at: &str) -> runs::NewAgentRun {
    runs::NewAgentRun {
        id: id(run_id),
        workspace_id: WORKSPACE.to_string(),
        conversation_id: id(CONVERSATION_ID),
        user_message_id: id(USER_MESSAGE_ID),
        event_id: id(event_id),
        timestamp: timestamp(created_at),
    }
}

fn append_delta(connection: &mut Connection) {
    events::append(
        connection,
        WORKSPACE,
        id(RUN_ID),
        id(DELTA_EVENT_ID),
        timestamp("2026-08-03T09:00:01Z"),
        AgentEventData::MessageDelta(MessageDelta {
            message_id: id(ASSISTANT_MESSAGE_ID),
            role: AgentMessageRole::Assistant,
            delta: "Q355".to_string(),
        }),
    )
    .unwrap();
}

fn set_completing(connection: &Connection) {
    connection
        .execute(
            "UPDATE agent_runs SET state = 'completing' WHERE id = ?1",
            [RUN_ID],
        )
        .unwrap();
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

fn timestamp(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn remove_sqlite_files(path: &Path) {
    for candidate in [
        path.to_path_buf(),
        format!("{}-wal", path.display()).into(),
        format!("{}-shm", path.display()).into(),
    ] {
        if candidate.exists() {
            std::fs::remove_file(candidate).unwrap();
        }
    }
}
