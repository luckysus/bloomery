use bloomery::agent::protocol::{AgentEventData, AgentRunState};
use bloomery::agent::session::model::{SessionSnapshot, StartRunRequest};
use bloomery::agent::session::service::SessionService;
use bloomery::models::ConversationSnapshotMessage;
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::runs::{self, NewAgentRun};
use chrono::{TimeZone, Utc};
use rusqlite::{params, Connection};
use uuid::Uuid;

const WORKSPACE: &str = "local";
const OTHER_WORKSPACE: &str = "other";

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open in-memory database");
    connection
        .pragma_update(None, "foreign_keys", true)
        .expect("enable foreign keys");
    migrate(&mut connection).expect("migrate database");
    connection
}

fn timestamp(second: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2030, 1, 2, 3, 4, second)
        .single()
        .expect("valid timestamp")
}

fn run_request(conversation_id: &str, content: &str, second: u32) -> StartRunRequest {
    StartRunRequest {
        conversation_id: Uuid::parse_str(conversation_id).expect("UUID conversation"),
        user_message_id: Uuid::new_v4(),
        run_id: Uuid::new_v4(),
        event_id: Uuid::new_v4(),
        content: content.to_string(),
        timestamp: timestamp(second),
    }
}

#[test]
fn conversation_lifecycle_is_owned_by_the_session_service() {
    let mut connection = database();
    let mut session = SessionService::new(&mut connection, WORKSPACE).expect("session service");

    let conversation = session
        .create_conversation("  first conversation  ")
        .expect("create conversation");
    assert_eq!(conversation.title, "first conversation");
    assert_eq!(session.list_conversations(false).unwrap().len(), 1);

    session
        .rename_conversation(&conversation.id, "  renamed  ")
        .expect("rename conversation");
    session
        .set_conversation_pinned(&conversation.id, true)
        .expect("pin conversation");
    session
        .archive_conversation(&conversation.id)
        .expect("archive conversation");
    assert!(session.list_conversations(false).unwrap().is_empty());
    let archived = session.list_conversations(true).unwrap();
    assert_eq!(archived[0].title, "renamed");
    assert!(archived[0].pinned);

    session
        .restore_conversation(&conversation.id)
        .expect("restore conversation");
    session
        .rename_conversation(&conversation.id, "   ")
        .expect("apply fallback title");
    assert_eq!(
        session.list_conversations(false).unwrap()[0].title,
        "New conversation"
    );
}

#[test]
fn messages_normalize_roles_and_keep_stable_equal_timestamp_order() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("ordered")
        .unwrap();

    let appended = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .append_message(&conversation.id, "assistant", "normalized", None)
        .expect("append assistant message");
    assert_eq!(appended.role, "agent");

    connection
        .execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, 'user', ?4, NULL, ?5)",
            params![
                Uuid::new_v4().to_string(),
                WORKSPACE,
                conversation.id,
                "equal-time-one",
                "2040-01-01T00:00:00Z"
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, 'agent', ?4, NULL, ?5)",
            params![
                Uuid::new_v4().to_string(),
                WORKSPACE,
                conversation.id,
                "equal-time-two",
                "2040-01-01T00:00:00Z"
            ],
        )
        .unwrap();

    let messages = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .list_messages(&conversation.id)
        .unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["normalized", "equal-time-one", "equal-time-two"]
    );
}

#[test]
fn edit_and_truncate_invalidates_stale_messages_runs_events_and_summary() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("edit source")
        .unwrap();
    let anchor = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .start_run(run_request(&conversation.id, "original", 1))
        .expect("start anchor run");
    let shared_time = anchor.user_message.created_at.clone();
    let tail_user_id = Uuid::new_v4();

    insert_message(
        &connection,
        Uuid::new_v4(),
        &conversation.id,
        "agent",
        "old answer",
        &shared_time,
    );
    insert_message(
        &connection,
        tail_user_id,
        &conversation.id,
        "user",
        "later question",
        &shared_time,
    );
    runs::create(
        &mut connection,
        NewAgentRun {
            id: Uuid::new_v4(),
            workspace_id: WORKSPACE.to_string(),
            conversation_id: Uuid::parse_str(&conversation.id).unwrap(),
            user_message_id: tail_user_id,
            event_id: Uuid::new_v4(),
            timestamp: timestamp(2),
        },
    )
    .expect("create tail run");
    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .save_summary(
            &conversation.id,
            "stale summary",
            Some(tail_user_id.to_string()),
        )
        .unwrap();

    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .edit_message_and_truncate(&anchor.user_message.id, "edited question")
        .expect("edit and truncate");

    let messages = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .list_messages(&conversation.id)
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "edited question");
    assert_eq!(table_count(&connection, "agent_runs"), 0);
    assert_eq!(table_count(&connection, "agent_run_events"), 0);
    assert_eq!(table_count(&connection, "conversation_summaries"), 0);
}

#[test]
fn truncate_and_fork_use_rowid_to_break_equal_timestamp_ties() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("same-time source")
        .unwrap();
    let ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    for (id, content) in ids.into_iter().zip(["one", "two", "three"]) {
        insert_message(
            &connection,
            id,
            &conversation.id,
            "user",
            content,
            "2040-01-01T00:00:00Z",
        );
    }
    runs::create(
        &mut connection,
        NewAgentRun {
            id: Uuid::new_v4(),
            workspace_id: WORKSPACE.to_string(),
            conversation_id: Uuid::parse_str(&conversation.id).unwrap(),
            user_message_id: ids[1],
            event_id: Uuid::new_v4(),
            timestamp: timestamp(3),
        },
    )
    .expect("create source run and event");

    let fork = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .fork_conversation_from_message(&ids[1].to_string())
        .expect("fork through second message");
    let fork_messages = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .list_messages(&fork.id)
        .unwrap();
    assert_eq!(
        fork_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE conversation_id = ?1",
                params![fork.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_run_events WHERE conversation_id = ?1",
                params![fork.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE conversation_id = ?1",
                params![conversation.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );

    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .truncate_after_message(&ids[1].to_string())
        .expect("truncate after second message");
    let source_messages = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .list_messages(&conversation.id)
        .unwrap();
    assert_eq!(
        source_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE conversation_id = ?1",
                params![conversation.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_run_events WHERE conversation_id = ?1",
                params![conversation.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn draft_and_versioned_export_snapshot_round_trip() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("exported")
        .unwrap();
    let first = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .append_message(&conversation.id, "user", "question", None)
        .unwrap();
    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .append_message(
            &conversation.id,
            "assistant",
            "answer",
            Some("{}".to_string()),
        )
        .unwrap();
    let mut session = SessionService::new(&mut connection, WORKSPACE).unwrap();
    session
        .save_summary(&conversation.id, "summary", Some(first.id.clone()))
        .unwrap();
    session.save_draft(&conversation.id, "unfinished").unwrap();
    assert_eq!(session.load_draft(&conversation.id).unwrap(), "unfinished");

    let snapshot = session.export_snapshot(&conversation.id).unwrap();
    assert_eq!(snapshot.format_version, 1);
    assert_eq!(snapshot.conversation.id, conversation.id);
    assert_eq!(snapshot.messages.len(), 2);
    assert_eq!(snapshot.summary.as_ref().unwrap().text, "summary");
    assert!(serde_json::to_value(&snapshot)
        .unwrap()
        .get("draft")
        .is_none());
    let json = serde_json::to_string(&snapshot).unwrap();
    let decoded: SessionSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.format_version, 1);
    assert_eq!(decoded.messages[1].response_json.as_deref(), Some("{}"));
    let legacy: SessionSnapshot = serde_json::from_value(serde_json::json!({
        "format_version": 1,
        "conversation": snapshot.conversation,
        "messages": snapshot.messages,
        "summary": {
            "text": "legacy summary",
            "covered_message_id": first.id
        }
    }))
    .expect("version-one snapshot without summary sources");
    assert!(legacy.summary.unwrap().source_message_ids.is_empty());

    session.save_draft(&conversation.id, "   ").unwrap();
    assert!(session.load_draft(&conversation.id).unwrap().is_empty());

    for key in ["__new__", "__agent_new__"] {
        session.save_draft(key, "new conversation draft").unwrap();
        assert_eq!(session.load_draft(key).unwrap(), "new conversation draft");
        session.save_draft(key, " ").unwrap();
        assert!(session.load_draft(key).unwrap().is_empty());
    }
}

#[test]
fn snapshot_import_requires_an_owned_conversation_without_disclosing_existence() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("import target")
        .unwrap();
    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .import_snapshot(
            &conversation.id,
            "imported title",
            vec![ConversationSnapshotMessage {
                role: "user".to_string(),
                content: "imported message".to_string(),
                response_json: None,
            }],
        )
        .expect("owned import");

    let owned = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .export_snapshot(&conversation.id)
        .unwrap();
    assert_eq!(owned.conversation.title, "imported title");
    assert_eq!(owned.messages[0].content, "imported message");

    let missing_error = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .import_snapshot(&Uuid::new_v4().to_string(), "missing", Vec::new())
        .expect_err("missing conversation must not be created");
    let foreign_error = SessionService::new(&mut connection, OTHER_WORKSPACE)
        .unwrap()
        .import_snapshot(&conversation.id, "foreign", Vec::new())
        .expect_err("foreign conversation must remain hidden");

    assert_eq!(missing_error, "conversation not found");
    assert_eq!(foreign_error, missing_error);
}

#[test]
fn every_session_operation_is_workspace_scoped() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("private")
        .unwrap();
    let message = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .append_message(&conversation.id, "user", "private message", None)
        .unwrap();
    {
        let mut owner = SessionService::new(&mut connection, WORKSPACE).unwrap();
        owner.save_draft(&conversation.id, "private draft").unwrap();
        owner
            .save_summary(
                &conversation.id,
                "private summary",
                Some(message.id.clone()),
            )
            .unwrap();
    }

    let mut other = SessionService::new(&mut connection, OTHER_WORKSPACE).unwrap();
    assert!(other.list_conversations(false).unwrap().is_empty());
    assert!(other.list_messages(&conversation.id).is_err());
    assert!(other.load_draft(&conversation.id).is_err());
    assert!(other.export_snapshot(&conversation.id).is_err());
    assert!(other
        .rename_conversation(&conversation.id, "stolen")
        .is_err());
    assert!(other
        .set_conversation_pinned(&conversation.id, true)
        .is_err());
    assert!(other.archive_conversation(&conversation.id).is_err());
    assert!(other.restore_conversation(&conversation.id).is_err());
    assert!(other.delete_conversation(&conversation.id).is_err());
    assert_eq!(
        other
            .append_message(&conversation.id, "user", "intrusion", None)
            .unwrap_err(),
        "conversation not found"
    );
    assert_eq!(
        other
            .import_snapshot(&conversation.id, "intrusion", Vec::new())
            .unwrap_err(),
        "conversation not found"
    );
    assert!(other
        .search_history("private", Some(&conversation.id), false, 8)
        .is_err());
    assert!(other
        .edit_message_and_truncate(&message.id, "intrusion")
        .is_err());
    assert!(other.truncate_after_message(&message.id).is_err());
    assert!(other.fork_conversation_from_message(&message.id).is_err());
    assert!(other.save_draft(&conversation.id, "intrusion").is_err());
    assert!(other.clear_draft(&conversation.id).is_err());
    assert!(other.load_summary(&conversation.id).is_err());
    assert_eq!(
        other
            .save_summary(&conversation.id, "intrusion", Some(message.id.clone()))
            .unwrap_err(),
        "conversation not found"
    );
    let mut request = run_request(&conversation.id, "intrusion", 3);
    request.conversation_id = Uuid::parse_str(&conversation.id).unwrap();
    assert!(other.start_run(request).is_err());
    drop(other);

    let owner = SessionService::new(&mut connection, WORKSPACE).unwrap();
    assert_eq!(owner.list_messages(&conversation.id).unwrap().len(), 1);
    assert_eq!(owner.load_draft(&conversation.id).unwrap(), "private draft");
    assert_eq!(
        owner.load_summary(&conversation.id).unwrap(),
        "private summary"
    );
    assert_eq!(
        owner
            .export_snapshot(&conversation.id)
            .unwrap()
            .conversation
            .title,
        "private"
    );
}

#[test]
fn summary_coverage_anchor_must_belong_to_the_same_conversation() {
    let mut connection = database();
    let first = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("first")
        .unwrap();
    let second = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("second")
        .unwrap();
    let second_message = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .append_message(&second.id, "user", "second message", None)
        .unwrap();

    let mut session = SessionService::new(&mut connection, WORKSPACE).unwrap();
    assert!(session
        .save_summary(
            &first.id,
            "invalid foreign coverage",
            Some(second_message.id),
        )
        .is_err());
    assert!(session
        .save_summary(
            &first.id,
            "invalid missing coverage",
            Some(Uuid::new_v4().to_string()),
        )
        .is_err());
    assert!(session.load_summary(&first.id).unwrap().is_empty());
}

#[test]
fn user_message_run_and_first_event_commit_atomically() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("atomic")
        .unwrap();
    let request = run_request(&conversation.id, "start agent", 4);
    let expected_run_id = request.run_id;
    let expected_event_id = request.event_id;

    let started = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .start_run(request)
        .expect("start run atomically");

    assert_eq!(started.user_message.role, "user");
    assert_eq!(started.user_message.content, "start agent");
    assert_eq!(started.run.run.id, expected_run_id);
    assert_eq!(started.run.run.state, AgentRunState::Created);
    assert_eq!(started.run.event.event_id, expected_event_id);
    assert_eq!(started.run.event.sequence, 1);
    assert!(matches!(
        started.run.event.data,
        AgentEventData::RunCreated(_)
    ));
    assert_eq!(table_count(&connection, "messages"), 1);
    assert_eq!(table_count(&connection, "agent_runs"), 1);
    assert_eq!(table_count(&connection, "agent_run_events"), 1);
}

#[test]
fn run_event_failure_rolls_back_message_and_conversation_timestamp() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("rollback")
        .unwrap();
    let original_updated_at = conversation.updated_at.clone();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_session_run_event
             BEFORE INSERT ON agent_run_events
             BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;",
        )
        .unwrap();

    let error = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .start_run(run_request(&conversation.id, "must roll back", 5))
        .expect_err("event failure must roll back outer transaction");
    assert!(error.contains("agent_event_storage_failed"));
    assert_eq!(table_count(&connection, "messages"), 0);
    assert_eq!(table_count(&connection, "agent_runs"), 0);
    assert_eq!(table_count(&connection, "agent_run_events"), 0);
    let updated_at: String = connection
        .query_row(
            "SELECT updated_at FROM conversations WHERE id = ?1",
            params![conversation.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(updated_at, original_updated_at);
}

#[test]
fn duplicate_first_event_rolls_back_only_the_second_user_message_and_run() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("duplicate event")
        .unwrap();
    let first_request = run_request(&conversation.id, "first", 7);
    let duplicate_event_id = first_request.event_id;
    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .start_run(first_request)
        .unwrap();
    let updated_after_first: String = connection
        .query_row(
            "SELECT updated_at FROM conversations WHERE id = ?1",
            params![conversation.id],
            |row| row.get(0),
        )
        .unwrap();
    let mut second_request = run_request(&conversation.id, "second", 8);
    second_request.event_id = duplicate_event_id;

    let error = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .start_run(second_request)
        .expect_err("duplicate first event must roll back second turn");

    assert!(error.contains("agent_event_duplicate"));
    assert_eq!(table_count(&connection, "messages"), 1);
    assert_eq!(table_count(&connection, "agent_runs"), 1);
    assert_eq!(table_count(&connection, "agent_run_events"), 1);
    let updated_after_failure: String = connection
        .query_row(
            "SELECT updated_at FROM conversations WHERE id = ?1",
            params![conversation.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(updated_after_failure, updated_after_first);
}

#[test]
fn deleting_through_session_cascades_messages_runs_and_events() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("delete cascade")
        .unwrap();
    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .start_run(run_request(&conversation.id, "delete me", 9))
        .unwrap();

    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .delete_conversation(&conversation.id)
        .unwrap();

    assert_eq!(table_count(&connection, "conversations"), 0);
    assert_eq!(table_count(&connection, "messages"), 0);
    assert_eq!(table_count(&connection, "agent_runs"), 0);
    assert_eq!(table_count(&connection, "agent_run_events"), 0);
}

#[test]
fn empty_user_message_is_rejected_before_any_run_write() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("empty")
        .unwrap();

    let error = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .start_run(run_request(&conversation.id, "   ", 6))
        .expect_err("empty request must fail");

    assert!(error.contains("user message is required"));
    assert_eq!(table_count(&connection, "messages"), 0);
    assert_eq!(table_count(&connection, "agent_runs"), 0);
    assert_eq!(table_count(&connection, "agent_run_events"), 0);
}

fn insert_message(
    connection: &Connection,
    id: Uuid,
    conversation_id: &str,
    role: &str,
    content: &str,
    created_at: &str,
) {
    connection
        .execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
            params![
                id.to_string(),
                WORKSPACE,
                conversation_id,
                role,
                content,
                created_at
            ],
        )
        .expect("insert message");
}

fn table_count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count rows")
}
