use bloomery::agent::protocol::{
    AgentEventData, AgentMessageRole, AgentRunState, MessageDelta, PermissionRequested,
    PermissionRisk, RunCompleted, RunOutcome, RunStateChanged, ToolRequested,
};
use bloomery::agent::runtime::{AgentRecoveryService, RecoveryAction};
use bloomery::agent::session::{SessionService, StartRunOutcome, StartRunRequest};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::{events, runs};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::collections::HashSet;
use uuid::Uuid;

const WORKSPACE: &str = "local";
const CONVERSATION_ID: &str = "11111111-1111-4111-8111-111111111111";
const USER_MESSAGE_ID: &str = "22222222-2222-4222-8222-222222222222";
const ASSISTANT_MESSAGE_ID: &str = "33333333-3333-4333-8333-333333333333";

#[test]
fn startup_recovery_interrupts_generation_and_retry_reuses_the_user_message() {
    let mut connection = setup_with_user_message();
    let run_id = create_run(&mut connection, "2026-08-05T08:00:00Z");
    transition(
        &mut connection,
        run_id,
        AgentRunState::Created,
        AgentRunState::Preparing,
        "2026-08-05T08:00:01Z",
    );
    transition(
        &mut connection,
        run_id,
        AgentRunState::Preparing,
        AgentRunState::Generating,
        "2026-08-05T08:00:02Z",
    );

    let mut recovery = AgentRecoveryService::new(&mut connection, WORKSPACE).unwrap();
    let recovered = recovery
        .recover_active(&HashSet::new(), timestamp("2026-08-05T08:01:00Z"))
        .unwrap();

    assert_eq!(recovered.len(), 1);
    assert!(matches!(recovered[0].action, RecoveryAction::Regenerate));
    assert_eq!(recovered[0].run.state, AgentRunState::Interrupted);
    assert!(matches!(
        recovered[0].events.last().unwrap().data,
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Interrupted,
            ..
        })
    ));

    let retry = recovery
        .retry(
            run_id,
            Uuid::new_v4(),
            Uuid::new_v4(),
            timestamp("2026-08-05T08:01:01Z"),
        )
        .unwrap();
    assert_eq!(retry.run.user_message_id, id(USER_MESSAGE_ID));
    assert_eq!(retry.run.state, AgentRunState::Created);
}

#[test]
fn unresolved_permission_is_replayed_without_executing_or_interrupting_it() {
    let mut connection = setup_with_user_message();
    let run_id = create_run(&mut connection, "2026-08-05T08:00:00Z");
    let permission_id = Uuid::new_v4();
    let tool_call_id = Uuid::new_v4();
    transition(
        &mut connection,
        run_id,
        AgentRunState::Created,
        AgentRunState::AwaitingPermission,
        "2026-08-05T08:00:01Z",
    );
    append_event(
        &mut connection,
        run_id,
        "2026-08-05T08:00:02Z",
        AgentEventData::PermissionRequested(PermissionRequested {
            permission_id,
            tool_call_id,
            risk: PermissionRisk::Dangerous,
            reason: "requires user approval".to_string(),
            summary: "Run a write tool".to_string(),
        }),
    );

    let mut recovery = AgentRecoveryService::new(&mut connection, WORKSPACE).unwrap();
    let recovered = recovery
        .recover_active(&HashSet::new(), timestamp("2026-08-05T08:01:00Z"))
        .unwrap();

    assert_eq!(recovered.len(), 1);
    assert!(matches!(
        &recovered[0].action,
        RecoveryAction::AwaitPermissions(permissions)
            if permissions.len() == 1
                && permissions[0].permission_id == permission_id
                && permissions[0].tool_call_id == tool_call_id
    ));
    assert_eq!(recovered[0].run.state, AgentRunState::AwaitingPermission);
    assert!(recovered[0].events.is_empty());
}

#[test]
fn only_declared_idempotent_tool_checkpoints_are_resumable() {
    let mut connection = setup_with_user_message();
    let resumable_run = create_run(&mut connection, "2026-08-05T08:00:00Z");
    let non_resumable_run = create_run(&mut connection, "2026-08-05T08:00:00Z");
    let resumable_call = Uuid::new_v4();
    let non_resumable_call = Uuid::new_v4();
    for run_id in [resumable_run, non_resumable_run] {
        transition(
            &mut connection,
            run_id,
            AgentRunState::Created,
            AgentRunState::ExecutingTools,
            "2026-08-05T08:00:01Z",
        );
    }
    append_event(
        &mut connection,
        resumable_run,
        "2026-08-05T08:00:02Z",
        tool_requested(resumable_call, "search.v1", "search"),
    );
    append_event(
        &mut connection,
        non_resumable_run,
        "2026-08-05T08:00:02Z",
        tool_requested(non_resumable_call, "write.v1", "write"),
    );

    let idempotent = HashSet::from(["search.v1".to_string()]);
    let mut recovery = AgentRecoveryService::new(&mut connection, WORKSPACE).unwrap();
    let recovered = recovery
        .recover_active(&idempotent, timestamp("2026-08-05T08:01:00Z"))
        .unwrap();

    let resumed = recovered
        .iter()
        .find(|item| item.run.id == resumable_run)
        .unwrap();
    assert!(matches!(
        &resumed.action,
        RecoveryAction::ResumeTools(checkpoints)
            if checkpoints.len() == 1
                && checkpoints[0].tool_call_id == resumable_call
                && checkpoints[0].tool_id == "search.v1"
    ));
    assert_eq!(resumed.run.state, AgentRunState::ExecutingTools);

    let interrupted = recovered
        .iter()
        .find(|item| item.run.id == non_resumable_run)
        .unwrap();
    assert!(matches!(interrupted.action, RecoveryAction::Regenerate));
    assert_eq!(interrupted.run.state, AgentRunState::Interrupted);
}

#[test]
fn replay_is_incremental_and_cancel_is_idempotent_for_terminal_runs() {
    let mut connection = setup_with_user_message();
    let run_id = create_run(&mut connection, "2026-08-05T08:00:00Z");
    transition(
        &mut connection,
        run_id,
        AgentRunState::Created,
        AgentRunState::Generating,
        "2026-08-05T08:00:01Z",
    );
    append_event(
        &mut connection,
        run_id,
        "2026-08-05T08:00:02Z",
        AgentEventData::MessageDelta(MessageDelta {
            message_id: id(ASSISTANT_MESSAGE_ID),
            role: AgentMessageRole::Assistant,
            delta: "partial".to_string(),
        }),
    );

    let mut recovery = AgentRecoveryService::new(&mut connection, WORKSPACE).unwrap();
    let incremental = recovery.replay(run_id, 1).unwrap();
    assert_eq!(
        incremental
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );

    let cancelled = recovery
        .cancel(
            run_id,
            Some(id(ASSISTANT_MESSAGE_ID)),
            timestamp("2026-08-05T08:01:00Z"),
        )
        .unwrap();
    assert!(!cancelled.replay_only);
    assert_eq!(cancelled.run.state, AgentRunState::Cancelled);
    assert_eq!(cancelled.events.len(), 2);

    let replay_only = recovery
        .cancel(run_id, None, timestamp("2026-08-05T08:01:01Z"))
        .unwrap();
    assert!(replay_only.replay_only);
    assert_eq!(replay_only.run.state, AgentRunState::Cancelled);
    assert_eq!(replay_only.events.len(), 5);
}

#[test]
fn duplicate_start_submission_replays_the_existing_run_without_duplicate_rows() {
    let mut connection = setup_conversation();
    let request = StartRunRequest {
        conversation_id: id(CONVERSATION_ID),
        user_message_id: id(USER_MESSAGE_ID),
        run_id: Uuid::new_v4(),
        event_id: Uuid::new_v4(),
        content: "Explain Q355B".to_string(),
        timestamp: timestamp("2026-08-05T08:00:00Z"),
    };
    let mut service = SessionService::new(&mut connection, WORKSPACE).unwrap();

    let first = service.start_or_replay(request.clone()).unwrap();
    let second = service.start_or_replay(request).unwrap();

    assert!(matches!(first, StartRunOutcome::Started(_)));
    assert!(matches!(second, StartRunOutcome::Existing { .. }));
    assert_eq!(count(&connection, "messages"), 1);
    assert_eq!(count(&connection, "agent_runs"), 1);
    assert_eq!(count(&connection, "agent_run_events"), 1);
}

#[test]
fn stale_cleanup_only_interrupts_runs_older_than_the_cutoff() {
    let mut connection = setup_with_user_message();
    let stale_run = create_run(&mut connection, "2026-08-05T08:00:00Z");
    let fresh_run = create_run(&mut connection, "2026-08-05T08:00:00Z");
    transition(
        &mut connection,
        stale_run,
        AgentRunState::Created,
        AgentRunState::Generating,
        "2026-08-05T08:01:00Z",
    );
    transition(
        &mut connection,
        fresh_run,
        AgentRunState::Created,
        AgentRunState::Generating,
        "2026-08-05T08:10:00Z",
    );

    let mut recovery = AgentRecoveryService::new(&mut connection, WORKSPACE).unwrap();
    let recovered = recovery
        .recover_stale(
            &HashSet::new(),
            timestamp("2026-08-05T08:05:00Z"),
            timestamp("2026-08-05T08:11:00Z"),
        )
        .unwrap();

    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].run.id, stale_run);
    assert_eq!(recovered[0].run.state, AgentRunState::Interrupted);
    assert_eq!(
        runs::get(&connection, WORKSPACE, fresh_run)
            .unwrap()
            .unwrap()
            .state,
        AgentRunState::Generating
    );
}

fn setup_conversation() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    connection
        .execute(
            "INSERT INTO conversations
             (id, workspace_id, title, created_at, updated_at, pinned, archived)
             VALUES (?1, ?2, 'Agent recovery', '2026-08-05T08:00:00Z',
                     '2026-08-05T08:00:00Z', 0, 0)",
            params![CONVERSATION_ID, WORKSPACE],
        )
        .unwrap();
    connection
}

fn setup_with_user_message() -> Connection {
    let connection = setup_conversation();
    connection
        .execute(
            "INSERT INTO messages
             (id, workspace_id, conversation_id, role, content, response_json, created_at)
             VALUES (?1, ?2, ?3, 'user', 'Explain Q355B', NULL, '2026-08-05T08:00:00Z')",
            params![USER_MESSAGE_ID, WORKSPACE, CONVERSATION_ID],
        )
        .unwrap();
    connection
}

fn create_run(connection: &mut Connection, created_at: &str) -> Uuid {
    let run_id = Uuid::new_v4();
    runs::create(
        connection,
        runs::NewAgentRun {
            id: run_id,
            workspace_id: WORKSPACE.to_string(),
            conversation_id: id(CONVERSATION_ID),
            user_message_id: id(USER_MESSAGE_ID),
            event_id: Uuid::new_v4(),
            timestamp: timestamp(created_at),
        },
    )
    .unwrap();
    run_id
}

fn transition(
    connection: &mut Connection,
    run_id: Uuid,
    previous: AgentRunState,
    current: AgentRunState,
    changed_at: &str,
) {
    runs::transition(
        connection,
        WORKSPACE,
        run_id,
        RunStateChanged {
            previous,
            current,
            reason: None,
        },
        timestamp(changed_at),
    )
    .unwrap();
}

fn append_event(connection: &mut Connection, run_id: Uuid, created_at: &str, data: AgentEventData) {
    events::append(
        connection,
        WORKSPACE,
        run_id,
        Uuid::new_v4(),
        timestamp(created_at),
        data,
    )
    .unwrap();
}

fn tool_requested(tool_call_id: Uuid, tool_id: &str, tool_name: &str) -> AgentEventData {
    AgentEventData::ToolRequested(ToolRequested {
        tool_call_id,
        tool_id: tool_id.to_string(),
        tool_name: tool_name.to_string(),
        arguments: serde_json::json!({"query": "Q355B"}),
    })
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
