use bloomery::tasks::mailbox::{MailboxMessage, MailboxMessageKind, MailboxStore};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("bloomery-mailbox-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    root
}

fn message(id: Uuid) -> MailboxMessage {
    MailboxMessage {
        id,
        sender: "lead".to_string(),
        recipient: "alice".to_string(),
        kind: MailboxMessageKind::Message,
        content: "inspect the task".to_string(),
    }
}

#[test]
fn mailbox_claim_ack_release_and_recovery_are_durable() {
    let root = temp_root();
    let store = MailboxStore::new(&root).unwrap();
    store.ensure_recipient("alice").unwrap();
    let original = message(Uuid::new_v4());
    store.send(&original).unwrap();
    let claimed = store.claim("alice").unwrap().unwrap();
    assert_eq!(claimed, original);
    store.release(&claimed).unwrap();
    let claimed_again = store.claim("alice").unwrap().unwrap();
    assert!(store.ack(&claimed_again).unwrap());
    assert!(store.ack(&claimed_again).unwrap());
    assert!(store.claim("alice").unwrap().is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn processing_messages_return_to_ready_after_restart() {
    let root = temp_root();
    let store = MailboxStore::new(&root).unwrap();
    store.ensure_recipient("alice").unwrap();
    let original = message(Uuid::new_v4());
    store.send(&original).unwrap();
    assert_eq!(store.claim("alice").unwrap().unwrap(), original);
    assert_eq!(store.recover_processing("alice").unwrap(), 1);
    assert_eq!(store.claim("alice").unwrap().unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}
