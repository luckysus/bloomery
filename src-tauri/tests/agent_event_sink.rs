use bloomery::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentMessageRole, AgentRunState, MessageDelta,
    RunCompleted, RunOutcome, RunStateChanged,
};
use bloomery::agent::runtime::{AgentEventPublisher, AgentEventSink, SqliteAgentEventSink};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::{events, runs};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const WORKSPACE: &str = "local";
const CONVERSATION_ID: &str = "11111111-1111-4111-8111-111111111111";
const USER_MESSAGE_ID: &str = "22222222-2222-4222-8222-222222222222";
const ASSISTANT_MESSAGE_ID: &str = "33333333-3333-4333-8333-333333333333";
const RUN_ID: &str = "44444444-4444-4444-8444-444444444444";
const CREATED_EVENT_ID: &str = "55555555-5555-4555-8555-555555555555";

#[derive(Clone)]
struct RecordingPublisher {
    events: Arc<Mutex<Vec<AgentEventEnvelope>>>,
    fail: bool,
}

impl AgentEventPublisher for RecordingPublisher {
    fn publish(&mut self, event: &AgentEventEnvelope) -> Result<(), String> {
        self.events.lock().unwrap().push(event.clone());
        if self.fail {
            Err("publisher failed after receiving persisted event".to_string())
        } else {
            Ok(())
        }
    }
}

#[test]
fn sqlite_sink_persists_events_before_publishing_and_finishes_atomically() {
    let mut connection = setup_with_run();
    let published = Arc::new(Mutex::new(Vec::new()));
    let publisher = RecordingPublisher {
        events: Arc::clone(&published),
        fail: false,
    };
    let mut sink = SqliteAgentEventSink::new(&mut connection, WORKSPACE, id(RUN_ID), publisher);

    sink.transition(RunStateChanged {
        previous: AgentRunState::Created,
        current: AgentRunState::Preparing,
        reason: None,
    })
    .unwrap();
    sink.record(AgentEventData::MessageDelta(MessageDelta {
        message_id: id(ASSISTANT_MESSAGE_ID),
        role: AgentMessageRole::Assistant,
        delta: "Q355".to_string(),
    }))
    .unwrap();
    let terminal = sink
        .finish(
            RunStateChanged {
                previous: AgentRunState::Preparing,
                current: AgentRunState::Cancelled,
                reason: Some("user_cancelled".to_string()),
            },
            RunOutcome::Cancelled,
            Some(id(ASSISTANT_MESSAGE_ID)),
        )
        .unwrap();

    assert_eq!(terminal.len(), 2);
    let replayed = events::replay(&connection, WORKSPACE, id(RUN_ID), 0).unwrap();
    assert_eq!(replayed.len(), 5);
    assert_eq!(
        replayed
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert!(matches!(
        replayed[4].data,
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Cancelled,
            ..
        })
    ));
    assert_eq!(
        runs::get(&connection, WORKSPACE, id(RUN_ID))
            .unwrap()
            .unwrap()
            .state,
        AgentRunState::Cancelled
    );
    assert_eq!(published.lock().unwrap().len(), 4);
    assert_eq!(
        published
            .lock()
            .unwrap()
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
}

#[test]
fn publisher_failure_keeps_the_event_that_was_persisted_first() {
    let mut connection = setup_with_run();
    let published = Arc::new(Mutex::new(Vec::new()));
    let publisher = RecordingPublisher {
        events: Arc::clone(&published),
        fail: true,
    };
    let mut sink = SqliteAgentEventSink::new(&mut connection, WORKSPACE, id(RUN_ID), publisher);

    let error = sink
        .record(AgentEventData::MessageDelta(MessageDelta {
            message_id: id(ASSISTANT_MESSAGE_ID),
            role: AgentMessageRole::Assistant,
            delta: "persisted".to_string(),
        }))
        .unwrap_err();

    assert!(error.contains("publisher failed"));
    assert_eq!(published.lock().unwrap().len(), 1);
    assert_eq!(
        events::replay(&connection, WORKSPACE, id(RUN_ID), 0)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn terminal_run_cannot_be_finished_again() {
    let mut connection = setup_with_run();
    let publisher = RecordingPublisher {
        events: Arc::new(Mutex::new(Vec::new())),
        fail: false,
    };
    let mut sink = SqliteAgentEventSink::new(&mut connection, WORKSPACE, id(RUN_ID), publisher);

    sink.finish(
        RunStateChanged {
            previous: AgentRunState::Created,
            current: AgentRunState::Cancelled,
            reason: None,
        },
        RunOutcome::Cancelled,
        None,
    )
    .unwrap();
    let error = sink
        .finish(
            RunStateChanged {
                previous: AgentRunState::Cancelled,
                current: AgentRunState::Cancelled,
                reason: None,
            },
            RunOutcome::Cancelled,
            None,
        )
        .unwrap_err();

    assert!(error.contains("terminal run cannot be completed again"));
    assert_eq!(
        events::replay(&connection, WORKSPACE, id(RUN_ID), 0)
            .unwrap()
            .len(),
        3
    );
}

fn setup_with_run() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    connection
        .execute(
            "INSERT INTO conversations
             (id, workspace_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, 'Agent sink', '2026-08-05T00:00:00Z',
                     '2026-08-05T00:00:00Z', 0, 0)",
            params![CONVERSATION_ID, WORKSPACE],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, 'user', 'Explain Q355B', NULL, '2026-08-05T00:00:00Z')",
            params![USER_MESSAGE_ID, WORKSPACE, CONVERSATION_ID],
        )
        .unwrap();
    runs::create(
        &mut connection,
        runs::NewAgentRun {
            id: id(RUN_ID),
            workspace_id: WORKSPACE.to_string(),
            conversation_id: id(CONVERSATION_ID),
            user_message_id: id(USER_MESSAGE_ID),
            event_id: id(CREATED_EVENT_ID),
            timestamp: timestamp("2026-08-05T00:00:00Z"),
        },
    )
    .unwrap();
    connection
}

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

fn timestamp(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
