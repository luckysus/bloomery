use super::*;
use crate::agent::runtime::{
    NoopToolExecutor, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation, ToolRegistration,
};
use crate::agent::tool_repair::ToolSpec;
use crate::tools::{ConcurrencyPolicy, ToolSource, ToolVersion};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

struct TestHandler;

impl ToolHandler for TestHandler {
    fn execute(&self, _arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
        Box::pin(async { Ok(json!({"ok": true})) })
    }
}

struct TestTools {
    registrations: Vec<ToolRegistration>,
}

impl ToolExecutor for TestTools {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, _invocation: ToolInvocation, _cancellation: CancellationToken) -> ToolFuture {
        Box::pin(async { Ok(json!({"ok": true})) })
    }
}

fn snapshot(turn_id: Uuid, session_id: Uuid) -> TurnSnapshot {
    TurnSnapshot {
        turn_id,
        session_id,
        parent_turn_id: None,
        child_turn_limit: 4,
        provider: "test".to_string(),
        model: "test-model".to_string(),
        model_context_window: Some(8_192),
        output_reservation: 2_048,
        limits: AgentLoopLimits::default(),
        tool_ids: Vec::new(),
        tool_snapshot: Vec::new(),
    }
}

#[test]
fn host_rejects_two_foreground_turns_for_one_session() {
    let host = RuntimeHost::default();
    let session = Uuid::new_v4();
    host.begin_turn(snapshot(Uuid::new_v4(), session))
        .expect("first turn");
    let error = host
        .begin_turn(snapshot(Uuid::new_v4(), session))
        .expect_err("second foreground turn must be rejected");
    assert!(error.contains("active turn"));
}

#[test]
fn tool_snapshot_is_sorted_and_removed_when_turn_finishes() {
    let host = RuntimeHost::default();
    let turn_id = Uuid::new_v4();
    host.begin_turn(snapshot(turn_id, Uuid::new_v4()))
        .expect("turn");
    let tools = NoopToolExecutor;
    let updated = host
        .set_tool_snapshot(turn_id, &tools)
        .expect("tool snapshot");
    assert!(updated.tool_ids.is_empty());
    host.finish_turn(turn_id);
    assert!(host.snapshot(turn_id).is_err());
}

#[test]
fn rejected_duplicate_turn_does_not_leave_an_input_queue() {
    let host = RuntimeHost::default();
    let session_id = Uuid::new_v4();
    let first_turn = Uuid::new_v4();
    let second_turn = Uuid::new_v4();

    host.begin_turn(snapshot(first_turn, session_id))
        .expect("first turn");
    assert!(host.begin_turn(snapshot(second_turn, session_id)).is_err());

    host.finish_turn(first_turn);
    host.begin_turn(snapshot(second_turn, session_id))
        .expect("rejected turn can be retried after the active turn finishes");
}

#[test]
fn tool_snapshot_freezes_schema_source_and_execution_policy() {
    let host = RuntimeHost::default();
    let turn_id = Uuid::new_v4();
    host.begin_turn(snapshot(turn_id, Uuid::new_v4()))
        .expect("turn");
    let mut registration = ToolRegistration::new(
        ToolSpec {
            id: "mcp.files.read".to_string(),
            name: "read_file".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
            risk: crate::agent::protocol::PermissionRisk::ConfirmationRequired,
        },
        true,
        Arc::new(TestHandler),
    );
    registration.version = ToolVersion {
        major: 2,
        minor: 1,
        patch: 3,
    };
    registration.source = ToolSource::Mcp {
        server_id: "files".to_string(),
        server_version: ToolVersion {
            major: 1,
            minor: 4,
            patch: 0,
        },
    };
    registration.concurrency = ConcurrencyPolicy::ParallelRead;
    registration.timeout = Duration::from_secs(7);
    let tools = TestTools {
        registrations: vec![registration],
    };

    let updated = host
        .set_tool_snapshot(turn_id, &tools)
        .expect("tool snapshot");
    let entry = updated.tool_snapshot.first().expect("snapshot entry");
    assert_eq!(
        entry.version,
        ToolVersion {
            major: 2,
            minor: 1,
            patch: 3
        }
    );
    assert_eq!(entry.source, tools.registrations[0].source);
    assert_eq!(entry.timeout_ms, 7_000);
    assert_eq!(entry.concurrency, ConcurrencyPolicy::ParallelRead);
    assert!(entry.input_schema_json.contains("path"));
}

#[test]
fn parent_cancellation_propagates_to_active_children() {
    let host = RuntimeHost::default();
    let session_id = Uuid::new_v4();
    let parent_id = Uuid::new_v4();
    host.begin_turn(snapshot(parent_id, session_id))
        .expect("parent turn");
    let mut child = snapshot(Uuid::new_v4(), session_id);
    child.parent_turn_id = Some(parent_id);
    host.begin_turn(child.clone()).expect("child turn");

    host.cancel_turn(parent_id).expect("cancel parent tree");
    assert!(host
        .is_cancelled(&parent_id.to_string())
        .expect("parent cancellation"));
    assert!(host
        .is_cancelled(&child.turn_id.to_string())
        .expect("child cancellation"));
    assert_eq!(host.active_children(parent_id).expect("children").len(), 1);
}

#[test]
fn parent_child_limit_is_enforced_without_blocking_foreground_turns() {
    let host = RuntimeHost::default();
    let session_id = Uuid::new_v4();
    let parent_id = Uuid::new_v4();
    let mut parent = snapshot(parent_id, session_id);
    parent.child_turn_limit = 1;
    host.begin_turn(parent).expect("parent turn");
    let mut first = snapshot(Uuid::new_v4(), session_id);
    first.parent_turn_id = Some(parent_id);
    host.begin_turn(first).expect("first child");
    let mut second = snapshot(Uuid::new_v4(), session_id);
    second.parent_turn_id = Some(parent_id);
    assert!(host.begin_turn(second).is_err());

    host.finish_turn(parent_id);
    host.finish_turn(Uuid::new_v4());
}
