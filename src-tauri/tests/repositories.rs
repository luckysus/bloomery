use bloomery::models::MemoryInput;
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::{conversations, memories, settings};
use rusqlite::Connection;

const WORKSPACE: &str = "local";

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open memory database");
    migrate(&mut connection).expect("migrate database");
    connection
}

#[test]
fn conversation_lifecycle_is_workspace_scoped() {
    let mut connection = database();

    let conversation = conversations::create(&mut connection, WORKSPACE, "  first chat  ")
        .expect("create conversation");
    assert_eq!(conversation.title, "first chat");
    assert_eq!(
        conversations::list(&connection, WORKSPACE, false)
            .unwrap()
            .len(),
        1
    );
    assert!(conversations::list(&connection, "other", false)
        .unwrap()
        .is_empty());

    conversations::update_title(&mut connection, WORKSPACE, &conversation.id, "renamed")
        .expect("rename conversation");
    conversations::set_pinned(&mut connection, WORKSPACE, &conversation.id, true)
        .expect("pin conversation");
    conversations::set_archived(&mut connection, WORKSPACE, &conversation.id, true)
        .expect("archive conversation");

    assert!(conversations::list(&connection, WORKSPACE, false)
        .unwrap()
        .is_empty());
    let archived = conversations::list(&connection, WORKSPACE, true).unwrap();
    assert_eq!(archived[0].title, "renamed");
    assert!(archived[0].pinned);

    conversations::set_archived(&mut connection, WORKSPACE, &conversation.id, false)
        .expect("restore conversation");
    conversations::delete(&mut connection, WORKSPACE, &conversation.id)
        .expect("delete conversation");
    assert!(conversations::list(&connection, WORKSPACE, false)
        .unwrap()
        .is_empty());
}

#[test]
fn message_edit_truncate_and_fork_preserve_expected_history() {
    let mut connection = database();
    let conversation =
        conversations::create(&mut connection, WORKSPACE, "source").expect("create source");
    connection
        .execute_batch(&format!(
            r#"
            INSERT INTO messages
              (id, workspace_id, conversation_id, role, content, response_json, created_at)
              VALUES ('m1', '{WORKSPACE}', '{}', 'user', 'one', NULL, '2024-01-01T00:00:01Z');
            INSERT INTO messages
              (id, workspace_id, conversation_id, role, content, response_json, created_at)
              VALUES ('m2', '{WORKSPACE}', '{}', 'agent', 'two', NULL, '2024-01-01T00:00:02Z');
            INSERT INTO messages
              (id, workspace_id, conversation_id, role, content, response_json, created_at)
              VALUES ('m3', '{WORKSPACE}', '{}', 'user', 'three', NULL, '2024-01-01T00:00:03Z');
            "#,
            conversation.id, conversation.id, conversation.id
        ))
        .expect("seed ordered messages");

    conversations::replace_after_edit(&mut connection, WORKSPACE, "m1", "one edited")
        .expect("edit first message");
    let edited = conversations::list_messages(&connection, WORKSPACE, &conversation.id)
        .expect("list edited messages");
    assert_eq!(edited.len(), 1);
    assert_eq!(edited[0].content, "one edited");

    connection
        .execute_batch(&format!(
            r#"
            INSERT INTO messages
              (id, workspace_id, conversation_id, role, content, response_json, created_at)
              VALUES ('m2b', '{WORKSPACE}', '{}', 'agent', 'two again', NULL, '2024-01-01T00:00:04Z');
            INSERT INTO messages
              (id, workspace_id, conversation_id, role, content, response_json, created_at)
              VALUES ('m3b', '{WORKSPACE}', '{}', 'user', 'three again', NULL, '2024-01-01T00:00:05Z');
            "#,
            conversation.id, conversation.id
        ))
        .expect("seed second tail");
    conversations::truncate_after_message(&mut connection, WORKSPACE, "m2b")
        .expect("truncate tail");
    let truncated = conversations::list_messages(&connection, WORKSPACE, &conversation.id)
        .expect("list truncated messages");
    assert_eq!(truncated.len(), 2);

    let fork = conversations::fork_from_message(
        &mut connection,
        WORKSPACE,
        &conversation.id,
        "m2b",
        "fork",
    )
    .expect("fork conversation");
    let fork_messages =
        conversations::list_messages(&connection, WORKSPACE, &fork.id).expect("list fork messages");
    assert_eq!(fork_messages.len(), 2);
    assert_eq!(fork_messages[1].content, "two again");

    let appended = conversations::append_message(
        &mut connection,
        WORKSPACE,
        &fork.id,
        "assistant",
        "answer",
        None,
    )
    .expect("append normalized message");
    assert_eq!(appended.role, "agent");
}

#[test]
fn summaries_and_drafts_require_owned_conversations() {
    let mut connection = database();
    let conversation =
        conversations::create(&mut connection, WORKSPACE, "owned").expect("create conversation");

    conversations::save_summary(
        &mut connection,
        WORKSPACE,
        &conversation.id,
        "summary",
        None,
    )
    .expect("save summary");
    assert_eq!(
        conversations::get_summary(&connection, WORKSPACE, &conversation.id).unwrap(),
        "summary"
    );
    assert!(conversations::save_summary(
        &mut connection,
        "other",
        &conversation.id,
        "forbidden",
        None,
    )
    .is_err());

    conversations::save_draft(&mut connection, WORKSPACE, &conversation.id, "draft")
        .expect("save draft");
    assert_eq!(
        conversations::get_draft(&connection, WORKSPACE, &conversation.id).unwrap(),
        "draft"
    );
    conversations::clear_draft(&mut connection, WORKSPACE, &conversation.id).expect("clear draft");
    assert!(
        conversations::get_draft(&connection, WORKSPACE, &conversation.id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn conversation_delete_rolls_back_all_dependent_rows_on_failure() {
    let mut connection = database();
    let conversation =
        conversations::create(&mut connection, WORKSPACE, "rollback").expect("create conversation");
    conversations::append_message(
        &mut connection,
        WORKSPACE,
        &conversation.id,
        "user",
        "message",
        None,
    )
    .expect("append message");
    conversations::save_summary(
        &mut connection,
        WORKSPACE,
        &conversation.id,
        "summary",
        None,
    )
    .expect("save summary");
    conversations::save_draft(&mut connection, WORKSPACE, &conversation.id, "draft")
        .expect("save draft");
    connection
        .execute_batch(
            "CREATE TRIGGER block_conversation_delete
             BEFORE DELETE ON conversations
             BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
        )
        .expect("create failure trigger");

    assert!(conversations::delete(&mut connection, WORKSPACE, &conversation.id).is_err());

    for table in [
        "conversations",
        "messages",
        "conversation_summaries",
        "conversation_drafts",
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count retained rows");
        assert_eq!(count, 1, "{table} must roll back");
    }
}

#[test]
fn memory_crud_is_workspace_scoped() {
    let mut connection = database();
    let input = MemoryInput {
        id: None,
        scope: "global".to_string(),
        r#type: "fact".to_string(),
        title: "grade".to_string(),
        description: "".to_string(),
        body: "Q355B".to_string(),
        tags_json: "[]".to_string(),
        enabled: true,
    };

    let saved = memories::save(&mut connection, WORKSPACE, input).expect("save memory");
    assert_eq!(
        memories::get(&connection, WORKSPACE, &saved.id)
            .unwrap()
            .unwrap()
            .body,
        "Q355B"
    );
    assert!(memories::get(&connection, "other", &saved.id)
        .unwrap()
        .is_none());
    assert_eq!(
        memories::list(&connection, WORKSPACE, false, "")
            .unwrap()
            .len(),
        1
    );

    memories::archive(&mut connection, WORKSPACE, &saved.id).expect("archive memory");
    assert_eq!(
        memories::list(&connection, WORKSPACE, true, "")
            .unwrap()
            .len(),
        1
    );
    memories::restore(&mut connection, WORKSPACE, &saved.id).expect("restore memory");
    assert_eq!(
        memories::list(&connection, WORKSPACE, false, "Q355")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn settings_round_trip_without_tauri_state() {
    let mut connection = database();

    assert!(settings::get(&connection, WORKSPACE, "theme")
        .unwrap()
        .is_none());
    settings::set(&mut connection, WORKSPACE, "theme", "\"dark\"").expect("set setting");

    assert_eq!(
        settings::get(&connection, WORKSPACE, "theme")
            .unwrap()
            .unwrap(),
        "\"dark\""
    );
    assert!(settings::get(&connection, "other", "theme")
        .unwrap()
        .is_none());
}
