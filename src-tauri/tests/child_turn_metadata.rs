use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use suna::agent::protocol::{
    AgentEventData, AgentEventEnvelope, AgentRunState, RunStateChanged, PROTOCOL_VERSION,
};
use suna::agent::runtime::{AgentLoopLimits, ChildTurnStore, SqliteChildTurnStore, TurnSnapshot};
use suna::agent::session::{SessionService, StartRunRequest};
use suna::storage::{database, repositories::child_turns};
use uuid::Uuid;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn child_records_keep_actual_snapshot_metadata_and_publish_only_persisted_events() {
    let fixture = Fixture(std::env::temp_dir().join(format!("suna-child-meta-{}", Uuid::new_v4())));
    fs::create_dir_all(&fixture.0).unwrap();
    let db = fixture.0.join("data.sqlite3");
    let (mut connection, _) = database::open(&db).unwrap();
    let parent_id = Uuid::new_v4();
    let session_id = {
        let mut session = SessionService::new(&mut connection, "workspace-a").unwrap();
        let conversation = session.create_conversation("expert task").unwrap();
        let conversation_id = Uuid::parse_str(&conversation.id).unwrap();
        session
            .start_run(StartRunRequest {
                conversation_id,
                user_message_id: Uuid::new_v4(),
                run_id: parent_id,
                event_id: Uuid::new_v4(),
                content: "review".to_string(),
                timestamp: Utc::now(),
            })
            .unwrap();
        conversation_id
    };
    let snapshot = TurnSnapshot {
        turn_id: Uuid::new_v4(),
        session_id,
        parent_turn_id: Some(parent_id),
        child_turn_limit: 2,
        provider: "expert-provider".to_string(),
        model: "expert-model".to_string(),
        model_context_window: Some(16384),
        output_reservation: 1024,
        reasoning_reservation: 512,
        limits: AgentLoopLimits::default(),
        tool_ids: Vec::new(),
        tool_snapshot: Vec::new(),
    };
    let published = Arc::new(Mutex::new(Vec::new()));
    let store = SqliteChildTurnStore::new(&db, "workspace-a").with_event_publisher({
        let published = published.clone();
        let db = db.clone();
        move |event| {
            let (connection, _) = database::open(&db).map_err(|error| error.to_string())?;
            let persisted = child_turns::replay(&connection, "workspace-a", event.run_id, 0)
                .map_err(|error| error.to_string())?;
            assert_eq!(persisted.last(), Some(event));
            published.lock().unwrap().push(event.clone());
            Ok(())
        }
    });
    store
        .begin_task(&snapshot, &"任务".repeat(400), Some("steel"))
        .unwrap();
    let records = child_turns::list(&connection, "workspace-a", Some(parent_id)).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].provider, "expert-provider");
    assert_eq!(records[0].model, "expert-model");
    assert_eq!(records[0].agent_id.as_deref(), Some("steel"));
    assert_eq!(records[0].task_summary.chars().count(), 512);
    assert!(child_turns::list(&connection, "workspace-b", None)
        .unwrap()
        .is_empty());
    let event = AgentEventEnvelope {
        protocol_version: PROTOCOL_VERSION,
        event_id: Uuid::new_v4(),
        run_id: snapshot.turn_id,
        conversation_id: session_id,
        sequence: 100,
        timestamp: Utc::now(),
        data: AgentEventData::RunStateChanged(RunStateChanged {
            previous: AgentRunState::Created,
            current: AgentRunState::Preparing,
            reason: None,
        }),
    };
    store.append(&event).unwrap();
    assert_eq!(published.lock().unwrap()[0].sequence, 1);
    let unknown = AgentEventEnvelope {
        run_id: Uuid::new_v4(),
        ..event
    };
    assert!(store.append(&unknown).is_err());
    assert_eq!(published.lock().unwrap().len(), 1);
}
