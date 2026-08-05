use bloomery::agent::context::{
    extract_memory_candidate, normalize_memory_key, MemoryCandidate, MemoryCandidateError,
    MemoryStatus, AUTO_MEMORY_WRITE_SETTING,
};
use bloomery::agent::session::{SessionService, StartRunRequest};
use bloomery::models::MemoryInput;
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::{memories, settings};
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

const WORKSPACE: &str = "local";

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open in-memory database");
    connection
        .pragma_update(None, "foreign_keys", true)
        .expect("enable foreign keys");
    migrate(&mut connection).expect("migrate database");
    connection
}

fn source(connection: &mut Connection, content: &str) -> (String, String, String) {
    let conversation = SessionService::new(connection, WORKSPACE)
        .unwrap()
        .create_conversation("memory source")
        .unwrap();
    let started = SessionService::new(connection, WORKSPACE)
        .unwrap()
        .start_run(StartRunRequest {
            conversation_id: Uuid::parse_str(&conversation.id).unwrap(),
            user_message_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            event_id: Uuid::new_v4(),
            content: content.to_string(),
            timestamp: Utc::now(),
        })
        .unwrap();
    (
        conversation.id,
        started.user_message.id,
        started.run.run.id.to_string(),
    )
}

fn candidate(message_id: &str, run_id: &str, content: &str) -> MemoryCandidate {
    extract_memory_candidate(content, message_id, Some(run_id.to_string()), 0.9)
        .expect("valid candidate")
        .expect("candidate marker")
}

fn manual_input(id: Option<String>, body: &str, enabled: bool) -> MemoryInput {
    MemoryInput {
        id,
        scope: "global".to_string(),
        r#type: "fact".to_string(),
        title: "steel preference".to_string(),
        description: "user-authored memory".to_string(),
        body: body.to_string(),
        tags_json: "[]".to_string(),
        enabled,
    }
}

#[test]
fn summary_preserves_coverage_sources_and_original_messages() {
    let mut connection = database();
    let conversation = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .create_conversation("summary")
        .unwrap();
    let first = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .append_message(&conversation.id, "user", "Q355B target", None)
        .unwrap();
    let second = SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .append_message(&conversation.id, "agent", "Use normalized rolling", None)
        .unwrap();
    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .append_message(&conversation.id, "user", "Keep this recent", None)
        .unwrap();

    let mut session = SessionService::new(&mut connection, WORKSPACE).unwrap();
    session
        .save_summary(
            &conversation.id,
            "Confirmed Q355B target; rolling remains unfinished.",
            Some(second.id.clone()),
        )
        .unwrap();
    let snapshot = session.export_snapshot(&conversation.id).unwrap();
    let summary = snapshot.summary.expect("stored summary");

    assert_eq!(
        summary.covered_message_id.as_deref(),
        Some(second.id.as_str())
    );
    assert_eq!(summary.source_message_ids, vec![first.id, second.id]);
    assert_eq!(snapshot.messages.len(), 3);
}

#[test]
fn extracts_candidate_with_source_confidence_and_normalized_key() {
    let candidate = extract_memory_candidate(
        "remember: always use Celsius for heat treatment",
        "message-1",
        Some("run-1".to_string()),
        0.85,
    )
    .unwrap()
    .expect("candidate");

    assert_eq!(candidate.body, "always use Celsius for heat treatment");
    assert_eq!(candidate.source_message_id, "message-1");
    assert_eq!(candidate.source_run_id.as_deref(), Some("run-1"));
    assert_eq!(candidate.confidence, 0.85);
    assert_eq!(candidate.dedup_key, "always use celsius for heat treatment");
}

#[test]
fn normalized_key_ignores_embedded_punctuation() {
    assert_eq!(
        normalize_memory_key("Q355-B"),
        normalize_memory_key("Q355B")
    );
    assert_eq!(
        normalize_memory_key("use Q355/B, always"),
        normalize_memory_key("use Q355B always")
    );
}

#[test]
fn candidate_extraction_is_safe_when_unicode_lowercase_changes_length() {
    let candidate = extract_memory_candidate("İ REMEMBER钢", "m1", None, 0.8)
        .unwrap()
        .expect("candidate");

    assert_eq!(candidate.body, "钢");
}

#[test]
fn english_markers_require_word_boundaries_and_prefer_the_full_marker() {
    let candidate = extract_memory_candidate("preference: Celsius", "m1", None, 0.8)
        .unwrap()
        .expect("candidate");

    assert_eq!(candidate.body, "Celsius");
    assert!(extract_memory_candidate("preferred steel", "m1", None, 0.8)
        .unwrap()
        .is_none());
    assert!(
        extract_memory_candidate("whenever possible", "m1", None, 0.8)
            .unwrap()
            .is_none()
    );
}

#[test]
fn candidate_confidence_must_be_finite_and_bounded() {
    assert_eq!(
        extract_memory_candidate("remember: use Celsius", "m1", None, 1.1).unwrap_err(),
        MemoryCandidateError::InvalidConfidence
    );
    assert_eq!(
        extract_memory_candidate("remember: use Celsius", "m1", None, f64::NAN).unwrap_err(),
        MemoryCandidateError::InvalidConfidence
    );
}

#[test]
fn duplicate_candidates_are_rejected_by_normalized_content() {
    let mut connection = database();
    let (_, message_id, run_id) = source(&mut connection, "remember: use Q355B");
    memories::capture_candidate(
        &mut connection,
        WORKSPACE,
        candidate(&message_id, &run_id, "remember: Use Q355B"),
    )
    .unwrap();

    let duplicate = candidate(&message_id, &run_id, "remember: use   q355b!!!");
    assert_eq!(
        memories::capture_candidate(&mut connection, WORKSPACE, duplicate).unwrap_err(),
        "memory duplicate"
    );
}

#[test]
fn migrated_legacy_memory_still_participates_in_candidate_deduplication() {
    let mut connection = database();
    let existing = memories::save(
        &mut connection,
        WORKSPACE,
        manual_input(None, "use Q355B", true),
    )
    .unwrap();
    connection
        .execute(
            "UPDATE memories SET dedup_key = 'legacy:' || id WHERE id = ?1",
            [&existing.id],
        )
        .unwrap();
    let (_, message_id, run_id) = source(&mut connection, "remember: use Q355B");

    assert_eq!(
        memories::capture_candidate(
            &mut connection,
            WORKSPACE,
            candidate(&message_id, &run_id, "remember: use Q355B"),
        )
        .unwrap_err(),
        "memory duplicate"
    );
}

#[test]
fn memory_suggestions_share_candidate_normalization() {
    let mut connection = database();
    memories::save(
        &mut connection,
        WORKSPACE,
        manual_input(None, "use Q355B", true),
    )
    .unwrap();
    source(&mut connection, "remember: use Q355-B");

    assert!(memories::suggest(&connection, WORKSPACE, 6)
        .unwrap()
        .is_empty());
}

#[test]
fn candidate_requires_confirmation_by_default() {
    let mut connection = database();
    let (_, message_id, run_id) = source(&mut connection, "remember: use Celsius");
    let stored = memories::capture_candidate(
        &mut connection,
        WORKSPACE,
        candidate(&message_id, &run_id, "remember: use Celsius"),
    )
    .unwrap();

    assert_eq!(stored.status, MemoryStatus::Pending);
    assert!(!stored.enabled);
    assert_eq!(
        stored.source_message_id.as_deref(),
        Some(message_id.as_str())
    );
    assert_eq!(stored.source_run_id.as_deref(), Some(run_id.as_str()));
    assert!(memories::list_context(&connection, WORKSPACE)
        .unwrap()
        .is_empty());

    memories::confirm_candidate(&mut connection, WORKSPACE, &stored.id).unwrap();
    let confirmed = memories::get(&connection, WORKSPACE, &stored.id)
        .unwrap()
        .unwrap();
    assert_eq!(confirmed.status, MemoryStatus::Confirmed);
    assert!(confirmed.enabled);
    assert_eq!(
        memories::list_context(&connection, WORKSPACE)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn pending_candidate_cannot_be_enabled_before_confirmation() {
    let mut connection = database();
    let (_, message_id, run_id) = source(&mut connection, "remember: use Celsius");
    let stored = memories::capture_candidate(
        &mut connection,
        WORKSPACE,
        candidate(&message_id, &run_id, "remember: use Celsius"),
    )
    .unwrap();

    assert_eq!(
        memories::set_enabled(&mut connection, WORKSPACE, &stored.id, true).unwrap_err(),
        "memory must be confirmed before enabling"
    );
    assert!(
        !memories::get(&connection, WORKSPACE, &stored.id)
            .unwrap()
            .unwrap()
            .enabled
    );
}

#[test]
fn automatic_confirmation_requires_explicit_true_preference() {
    let mut connection = database();
    settings::set(
        &mut connection,
        WORKSPACE,
        AUTO_MEMORY_WRITE_SETTING,
        "true",
    )
    .unwrap();
    let (_, message_id, run_id) = source(&mut connection, "remember: prefer Celsius");

    let stored = memories::capture_candidate(
        &mut connection,
        WORKSPACE,
        candidate(&message_id, &run_id, "remember: prefer Celsius"),
    )
    .unwrap();

    assert_eq!(stored.status, MemoryStatus::Confirmed);
    assert!(stored.enabled);
}

#[test]
fn non_boolean_true_preferences_do_not_confirm_candidates() {
    for value in ["false", "1", "\"true\""] {
        let mut connection = database();
        settings::set(&mut connection, WORKSPACE, AUTO_MEMORY_WRITE_SETTING, value).unwrap();
        let (_, message_id, run_id) = source(&mut connection, "remember: prefer Celsius");
        let stored = memories::capture_candidate(
            &mut connection,
            WORKSPACE,
            candidate(&message_id, &run_id, "remember: prefer Celsius"),
        )
        .unwrap();

        assert_eq!(
            stored.status,
            MemoryStatus::Pending,
            "setting value {value}"
        );
        assert!(!stored.enabled, "setting value {value}");
    }
}

#[test]
fn rejected_candidate_remains_visible_but_disabled() {
    let mut connection = database();
    let (_, message_id, run_id) = source(&mut connection, "remember: prefer local models");
    let stored = memories::capture_candidate(
        &mut connection,
        WORKSPACE,
        candidate(&message_id, &run_id, "remember: prefer local models"),
    )
    .unwrap();

    memories::reject_candidate(&mut connection, WORKSPACE, &stored.id).unwrap();
    let rejected = memories::get(&connection, WORKSPACE, &stored.id)
        .unwrap()
        .unwrap();
    assert_eq!(rejected.status, MemoryStatus::Rejected);
    assert!(!rejected.enabled);
    assert_eq!(
        memories::list(&connection, WORKSPACE, false, "")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn manual_memory_can_be_edited_without_losing_identity() {
    let mut connection = database();
    let original = memories::save(
        &mut connection,
        WORKSPACE,
        manual_input(None, "Use Q355B", true),
    )
    .unwrap();
    let updated = memories::save(
        &mut connection,
        WORKSPACE,
        manual_input(Some(original.id.clone()), "Use Q355C", true),
    )
    .unwrap();

    assert_eq!(updated.id, original.id);
    assert_eq!(updated.body, "Use Q355C");
    assert_ne!(updated.dedup_key, original.dedup_key);
    assert_eq!(updated.status, MemoryStatus::Confirmed);
}

#[test]
fn confirmed_memory_can_be_disabled_and_reenabled() {
    let mut connection = database();
    let memory = memories::save(
        &mut connection,
        WORKSPACE,
        manual_input(None, "Use Q355B", true),
    )
    .unwrap();

    memories::set_enabled(&mut connection, WORKSPACE, &memory.id, false).unwrap();
    assert!(
        !memories::get(&connection, WORKSPACE, &memory.id)
            .unwrap()
            .unwrap()
            .enabled
    );
    memories::set_enabled(&mut connection, WORKSPACE, &memory.id, true).unwrap();
    assert!(
        memories::get(&connection, WORKSPACE, &memory.id)
            .unwrap()
            .unwrap()
            .enabled
    );
}

#[test]
fn memory_can_be_archived_and_restored() {
    let mut connection = database();
    let memory = memories::save(
        &mut connection,
        WORKSPACE,
        manual_input(None, "Use Q355B", true),
    )
    .unwrap();

    memories::archive(&mut connection, WORKSPACE, &memory.id).unwrap();
    assert_eq!(
        memories::list(&connection, WORKSPACE, true, "")
            .unwrap()
            .len(),
        1
    );
    assert!(memories::list_context(&connection, WORKSPACE)
        .unwrap()
        .is_empty());
    memories::restore(&mut connection, WORKSPACE, &memory.id).unwrap();
    assert_eq!(
        memories::list_context(&connection, WORKSPACE)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn memory_can_be_permanently_deleted() {
    let mut connection = database();
    let memory = memories::save(
        &mut connection,
        WORKSPACE,
        manual_input(None, "Use Q355B", true),
    )
    .unwrap();

    memories::delete(&mut connection, WORKSPACE, &memory.id).unwrap();
    assert!(memories::get(&connection, WORKSPACE, &memory.id)
        .unwrap()
        .is_none());
}

#[test]
fn candidate_sources_are_workspace_scoped() {
    let mut connection = database();
    let (_, message_id, run_id) = source(&mut connection, "remember: use Celsius");
    let candidate = candidate(&message_id, &run_id, "remember: use Celsius");

    assert_eq!(
        memories::capture_candidate(&mut connection, "other", candidate).unwrap_err(),
        "memory source message not found"
    );
}

#[test]
fn deleting_candidate_sources_preserves_memory_provenance() {
    let mut connection = database();
    let (conversation_id, message_id, run_id) = source(&mut connection, "remember: use Celsius");
    let stored = memories::capture_candidate(
        &mut connection,
        WORKSPACE,
        candidate(&message_id, &run_id, "remember: use Celsius"),
    )
    .unwrap();

    SessionService::new(&mut connection, WORKSPACE)
        .unwrap()
        .delete_conversation(&conversation_id)
        .unwrap();
    let retained = memories::get(&connection, WORKSPACE, &stored.id)
        .unwrap()
        .expect("memory remains");

    assert_eq!(
        retained.source_message_id.as_deref(),
        Some(message_id.as_str())
    );
    assert_eq!(retained.source_run_id.as_deref(), Some(run_id.as_str()));
}
