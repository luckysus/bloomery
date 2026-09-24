use bloomery::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentMessageRole, AgentRunState, MessageDelta,
    RunCompleted, RunOutcome, RunStateChanged,
};
use bloomery::agent::runtime::{
    AgentContextCheckpoint, AgentEventPublisher, AgentEventSink, AgentLoopLimits,
    ContextCheckpointReason, SqliteAgentEventSink, TurnSnapshot,
};
use bloomery::providers::capabilities::{ChatImage, ChatMessage};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::{checkpoints, child_turns, events, runs, turn_snapshots};
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

#[test]
fn checkpoint_storage_strips_images_and_round_trips_state() {
    let mut connection = setup_with_run();
    let publisher = RecordingPublisher {
        events: Arc::new(Mutex::new(Vec::new())),
        fail: false,
    };
    let mut sink = SqliteAgentEventSink::new(&mut connection, WORKSPACE, id(RUN_ID), publisher);
    sink.checkpoint(AgentContextCheckpoint {
        reason: ContextCheckpointReason::ModelCall,
        model_call_index: 0,
        model_calls: 1,
        tool_calls: 0,
        tool_round: 0,
        recovery_attempt: 0,
        messages: vec![ChatMessage::with_images(
            "user",
            "keep text",
            vec![ChatImage {
                data: "secret-base64".to_string(),
                mime: "image/png".to_string(),
            }],
        )],
    })
    .unwrap();

    let restored = checkpoints::get(&connection, WORKSPACE, id(RUN_ID))
        .unwrap()
        .expect("checkpoint should be stored");
    assert_eq!(restored.reason, ContextCheckpointReason::ModelCall);
    assert_eq!(restored.messages[0].content, "keep text");
    assert!(restored.messages[0].images.is_empty());
    assert!(events::replay(&connection, WORKSPACE, id(RUN_ID), 0)
        .unwrap()
        .iter()
        .any(|event| matches!(event.data, AgentEventData::CheckpointSaved(_))));
}

#[test]
fn checkpoint_storage_rejects_payloads_over_512_kib() {
    let connection = setup_with_run();
    let checkpoint = AgentContextCheckpoint {
        reason: ContextCheckpointReason::ModelCall,
        model_call_index: 0,
        model_calls: 1,
        tool_calls: 0,
        tool_round: 0,
        recovery_attempt: 0,
        messages: vec![ChatMessage::new("user", "x".repeat(600 * 1024))],
    };

    let error = checkpoints::save(
        &connection,
        WORKSPACE,
        id(RUN_ID),
        &checkpoint,
        timestamp("2026-08-05T00:01:00Z"),
    )
    .expect_err("oversized checkpoint must be rejected");
    assert_eq!(error.code(), "agent_checkpoint_too_large");
}

#[test]
fn turn_snapshot_round_trips_without_credentials() {
    let connection = setup_with_run();
    let snapshot = turn_snapshots::AgentTurnSnapshot {
        turn: TurnSnapshot {
            turn_id: id(RUN_ID),
            session_id: id(CONVERSATION_ID),
            parent_turn_id: None,
            child_turn_limit: 4,
            provider: "open_ai_compatible".to_string(),
            model: "test-model".to_string(),
            model_context_window: Some(8_192),
            output_reservation: 2_048,
            limits: AgentLoopLimits::default(),
            tool_ids: vec!["steel.search_literature".to_string()],
            tool_snapshot: Vec::new(),
        },
        assistant_message_id: id(ASSISTANT_MESSAGE_ID),
        provider_base_url: "https://provider.example/v1".to_string(),
        smart_search_enabled: true,
        evidence_pack_id: None,
    };
    turn_snapshots::save(
        &connection,
        WORKSPACE,
        id(RUN_ID),
        &snapshot,
        timestamp("2026-08-05T00:01:00Z"),
    )
    .unwrap();

    assert_eq!(
        turn_snapshots::get(&connection, WORKSPACE, id(RUN_ID)).unwrap(),
        Some(snapshot)
    );
    let stored: String = connection
        .query_row(
            "SELECT snapshot_json FROM agent_run_turn_snapshots WHERE run_id = ?1",
            [RUN_ID],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!stored.contains("api_key"));
    assert!(!stored.contains("secret"));
}

#[test]
fn child_turn_events_are_durable_and_orphans_are_interrupted() {
    let mut connection = setup_with_run();
    let child_id = id("66666666-6666-4666-8666-666666666666");
    let snapshot = TurnSnapshot {
        turn_id: child_id,
        session_id: id(CONVERSATION_ID),
        parent_turn_id: Some(id(RUN_ID)),
        child_turn_limit: 2,
        provider: "test".to_string(),
        model: "test-model".to_string(),
        model_context_window: Some(8_192),
        output_reservation: 2_048,
        limits: AgentLoopLimits::default(),
        tool_ids: Vec::new(),
        tool_snapshot: Vec::new(),
    };
    child_turns::create(
        &mut connection,
        WORKSPACE,
        &snapshot,
        timestamp("2026-08-05T00:02:00Z"),
    )
    .unwrap();
    let stored_child = child_turns::get(&connection, WORKSPACE, child_id)
        .unwrap()
        .expect("child turn should be queryable");
    assert_eq!(stored_child.parent_turn_id, id(RUN_ID));
    assert_eq!(
        child_turns::list(&connection, WORKSPACE, Some(id(RUN_ID)))
            .unwrap()
            .len(),
        1
    );
    let event = AgentEventEnvelope {
        protocol_version: bloomery::agent::protocol::PROTOCOL_VERSION,
        event_id: id("77777777-7777-4777-8777-777777777777"),
        run_id: child_id,
        conversation_id: id(CONVERSATION_ID),
        sequence: 0,
        timestamp: timestamp("2026-08-05T00:02:01Z"),
        data: AgentEventData::MessageDelta(MessageDelta {
            message_id: id(ASSISTANT_MESSAGE_ID),
            role: AgentMessageRole::Assistant,
            delta: "child".to_string(),
        }),
    };
    let stored = child_turns::append(&mut connection, WORKSPACE, &event).unwrap();
    assert_eq!(stored.sequence, 1);
    assert_eq!(
        child_turns::replay(&connection, WORKSPACE, child_id, 0)
            .unwrap()
            .len(),
        1
    );
    child_turns::finish(
        &connection,
        WORKSPACE,
        child_id,
        RunOutcome::Completed,
        timestamp("2026-08-05T00:02:02Z"),
    )
    .unwrap();

    let orphan_id = id("88888888-8888-4888-8888-888888888888");
    let orphan = TurnSnapshot {
        turn_id: orphan_id,
        ..snapshot
    };
    child_turns::create(
        &mut connection,
        WORKSPACE,
        &orphan,
        timestamp("2026-08-05T00:03:00Z"),
    )
    .unwrap();
    let cancelled = child_turns::cancel(
        &mut connection,
        WORKSPACE,
        orphan_id,
        timestamp("2026-08-05T00:03:30Z"),
    )
    .unwrap();
    assert_eq!(cancelled.child.state, AgentRunState::Cancelled);
    assert!(!cancelled.replay_only);
    assert_eq!(cancelled.events.len(), 2);
    assert_eq!(
        child_turns::interrupt_orphans(
            &mut connection,
            WORKSPACE,
            timestamp("2026-08-05T00:04:00Z")
        )
        .unwrap(),
        0
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
