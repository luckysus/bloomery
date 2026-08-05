use bloomery::agent::protocol::{
    AgentError, AgentErrorCategory, AgentEventData, AgentEventEnvelope, AgentMessageRole,
    AgentRunState, ErrorRaised, EvidenceAttached, MessageCompleted, MessageDelta,
    PermissionDecision, PermissionRequested, PermissionResolved, PermissionRisk, RunCompleted,
    RunCreated, RunOutcome, RunStateChanged, TaskProgress, TaskProgressState, ToolCompleted,
    ToolOutcome, ToolProgress, ToolRequested, ToolStarted, UsageUpdated, PROTOCOL_VERSION,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

const EVENT_ID: &str = "11111111-1111-4111-8111-111111111111";
const RUN_ID: &str = "22222222-2222-4222-8222-222222222222";
const CONVERSATION_ID: &str = "33333333-3333-4333-8333-333333333333";
const MESSAGE_ID: &str = "44444444-4444-4444-8444-444444444444";
const TOOL_CALL_ID: &str = "55555555-5555-4555-8555-555555555555";
const PERMISSION_ID: &str = "66666666-6666-4666-8666-666666666666";
const EVIDENCE_PACK_ID: &str = "77777777-7777-4777-8777-777777777777";
const TASK_ID: &str = "88888888-8888-4888-8888-888888888888";
const TIMESTAMP: &str = "2026-08-03T08:00:00Z";

#[test]
fn run_event_snapshots_are_stable() {
    assert_snapshot(
        1,
        AgentEventData::RunCreated(RunCreated {
            state: AgentRunState::Created,
            user_message_id: id(MESSAGE_ID),
        }),
        "run_created",
        json!({
            "state": "created",
            "user_message_id": MESSAGE_ID,
        }),
    );
    assert_snapshot(
        2,
        AgentEventData::RunStateChanged(RunStateChanged {
            previous: AgentRunState::Created,
            current: AgentRunState::Preparing,
            reason: Some("context_ready".to_string()),
        }),
        "run_state_changed",
        json!({
            "previous": "created",
            "current": "preparing",
            "reason": "context_ready",
        }),
    );
}

#[test]
fn message_event_snapshots_are_stable() {
    assert_snapshot(
        3,
        AgentEventData::MessageDelta(MessageDelta {
            message_id: id(MESSAGE_ID),
            role: AgentMessageRole::Assistant,
            delta: "Q355".to_string(),
        }),
        "message_delta",
        json!({
            "message_id": MESSAGE_ID,
            "role": "assistant",
            "delta": "Q355",
        }),
    );
    assert_snapshot(
        4,
        AgentEventData::MessageCompleted(MessageCompleted {
            message_id: id(MESSAGE_ID),
            role: AgentMessageRole::Assistant,
            content: "Q355 has a nominal yield strength of 355 MPa.".to_string(),
            partial: false,
        }),
        "message_completed",
        json!({
            "message_id": MESSAGE_ID,
            "role": "assistant",
            "content": "Q355 has a nominal yield strength of 355 MPa.",
            "partial": false,
        }),
    );
}

#[test]
fn tool_event_snapshots_are_stable() {
    assert_snapshot(
        5,
        AgentEventData::ToolRequested(ToolRequested {
            tool_call_id: id(TOOL_CALL_ID),
            tool_id: "builtin.knowledge.search".to_string(),
            tool_name: "Search local knowledge".to_string(),
            arguments: json!({"query": "Q355 controlled cooling"}),
        }),
        "tool_requested",
        json!({
            "tool_call_id": TOOL_CALL_ID,
            "tool_id": "builtin.knowledge.search",
            "tool_name": "Search local knowledge",
            "arguments": {"query": "Q355 controlled cooling"},
        }),
    );
    assert_snapshot(
        6,
        AgentEventData::ToolStarted(ToolStarted {
            tool_call_id: id(TOOL_CALL_ID),
        }),
        "tool_started",
        json!({"tool_call_id": TOOL_CALL_ID}),
    );
    assert_snapshot(
        7,
        AgentEventData::ToolProgress(ToolProgress {
            tool_call_id: id(TOOL_CALL_ID),
            progress: 45,
            message: Some("reranking evidence".to_string()),
        }),
        "tool_progress",
        json!({
            "tool_call_id": TOOL_CALL_ID,
            "progress": 45,
            "message": "reranking evidence",
        }),
    );
    assert_snapshot(
        8,
        AgentEventData::ToolCompleted(ToolCompleted {
            tool_call_id: id(TOOL_CALL_ID),
            outcome: ToolOutcome::Succeeded,
            output: Some(json!({"evidence_pack_id": EVIDENCE_PACK_ID})),
            error: None,
        }),
        "tool_completed",
        json!({
            "tool_call_id": TOOL_CALL_ID,
            "outcome": "succeeded",
            "output": {"evidence_pack_id": EVIDENCE_PACK_ID},
            "error": null,
        }),
    );
}

#[test]
fn permission_event_snapshots_are_stable() {
    assert_snapshot(
        9,
        AgentEventData::PermissionRequested(PermissionRequested {
            permission_id: id(PERMISSION_ID),
            tool_call_id: id(TOOL_CALL_ID),
            risk: PermissionRisk::ConfirmationRequired,
            reason: "The tool will write an export file.".to_string(),
            summary: "Export this conversation to report.md".to_string(),
        }),
        "permission_requested",
        json!({
            "permission_id": PERMISSION_ID,
            "tool_call_id": TOOL_CALL_ID,
            "risk": "confirmation_required",
            "reason": "The tool will write an export file.",
            "summary": "Export this conversation to report.md",
        }),
    );
    assert_snapshot(
        10,
        AgentEventData::PermissionResolved(PermissionResolved {
            permission_id: id(PERMISSION_ID),
            decision: PermissionDecision::AllowOnce,
        }),
        "permission_resolved",
        json!({
            "permission_id": PERMISSION_ID,
            "decision": "allow_once",
        }),
    );
}

#[test]
fn evidence_usage_task_completion_and_error_snapshots_are_stable() {
    assert_snapshot(
        11,
        AgentEventData::EvidenceAttached(EvidenceAttached {
            evidence_pack_id: id(EVIDENCE_PACK_ID),
            citation_numbers: vec![1, 2],
        }),
        "evidence_attached",
        json!({
            "evidence_pack_id": EVIDENCE_PACK_ID,
            "citation_numbers": [1, 2],
        }),
    );
    assert_snapshot(
        12,
        AgentEventData::UsageUpdated(UsageUpdated {
            prompt_tokens: 1200,
            completion_tokens: 240,
            total_tokens: 1440,
        }),
        "usage_updated",
        json!({
            "prompt_tokens": 1200,
            "completion_tokens": 240,
            "total_tokens": 1440,
        }),
    );
    assert_snapshot(
        13,
        AgentEventData::TaskProgress(TaskProgress {
            task_id: id(TASK_ID),
            kind: "knowledge_import".to_string(),
            state: TaskProgressState::Running,
            progress: 70,
        }),
        "task_progress",
        json!({
            "task_id": TASK_ID,
            "kind": "knowledge_import",
            "state": "running",
            "progress": 70,
        }),
    );
    assert_snapshot(
        14,
        AgentEventData::RunCompleted(RunCompleted {
            outcome: RunOutcome::Completed,
            assistant_message_id: Some(id(MESSAGE_ID)),
        }),
        "run_completed",
        json!({
            "outcome": "completed",
            "assistant_message_id": MESSAGE_ID,
        }),
    );
    assert_snapshot(
        15,
        AgentEventData::ErrorRaised(ErrorRaised {
            error: AgentError {
                code: "provider_quota_exceeded".to_string(),
                category: AgentErrorCategory::Quota,
                message: "The configured provider quota is exhausted.".to_string(),
                retryable: false,
                details: Some(json!({"provider_profile_id": "siliconflow-default"})),
            },
            fatal: true,
        }),
        "error_raised",
        json!({
            "error": {
                "code": "provider_quota_exceeded",
                "category": "quota",
                "message": "The configured provider quota is exhausted.",
                "retryable": false,
                "details": {"provider_profile_id": "siliconflow-default"},
            },
            "fatal": true,
        }),
    );
}

fn assert_snapshot(sequence: u64, data: AgentEventData, event_type: &str, payload: Value) {
    let event = AgentEventEnvelope {
        protocol_version: PROTOCOL_VERSION,
        event_id: id(EVENT_ID),
        run_id: id(RUN_ID),
        conversation_id: id(CONVERSATION_ID),
        sequence,
        timestamp: timestamp(),
        data,
    };
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(
        value,
        json!({
            "protocol_version": 1,
            "event_id": EVENT_ID,
            "run_id": RUN_ID,
            "conversation_id": CONVERSATION_ID,
            "sequence": sequence,
            "timestamp": TIMESTAMP,
            "type": event_type,
            "data": payload,
        })
    );
    assert_eq!(
        serde_json::from_value::<AgentEventEnvelope>(value).unwrap(),
        event
    );
}

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

fn timestamp() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(TIMESTAMP)
        .unwrap()
        .with_timezone(&Utc)
}
