use bloomery::tasks::mailbox::{ProtocolMailboxMessage, ProtocolMessageKind};
use bloomery::tasks::protocol::{ProtocolKind, ProtocolStatus, ProtocolStore};
use chrono::{TimeZone, Utc};
use std::fs;
use uuid::Uuid;

#[test]
fn protocol_state_is_persistent_and_response_is_idempotent() {
    let root = std::env::temp_dir().join(format!("bloomery-protocol-{}", Uuid::new_v4()));
    let now = Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap();
    let store = ProtocolStore::new(&root).unwrap();
    let request = store
        .create(
            ProtocolKind::PlanApproval,
            "alice",
            "lead",
            "write code",
            now,
        )
        .unwrap();
    let request_message = ProtocolStore::to_message(&request);
    assert_eq!(
        request_message.kind,
        ProtocolMessageKind::PlanApprovalRequest
    );
    let response = ProtocolMailboxMessage {
        id: Uuid::new_v4(),
        sender: "lead".to_string(),
        recipient: "alice".to_string(),
        kind: ProtocolMessageKind::PlanApprovalResponse,
        content: "approved".to_string(),
        request_id: request.id,
        approved: Some(true),
    };
    let resolved = store.consume_response(&response, now).unwrap();
    assert_eq!(resolved.status, ProtocolStatus::Approved);
    assert_eq!(store.consume_response(&response, now).unwrap(), resolved);
    assert_eq!(
        ProtocolStore::new(&root).unwrap().get(request.id).unwrap(),
        resolved
    );
    fs::remove_dir_all(root).unwrap();
}
