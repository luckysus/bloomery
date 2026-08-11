use bloomery::storage::database;
use bloomery::storage::migrations::{latest_version, migrate};
use rusqlite::{params, Connection, OptionalExtension};

const LEGACY_SCHEMA: &str = include_str!("../src/storage/migrations/0001_initial.sql");

fn user_version(conn: &Connection) -> u32 {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read user_version")
}

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("prepare table_info");
    statement
        .query_map([], |row| row.get(1))
        .expect("query table_info")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect table columns")
}

fn index_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA index_list({table})"))
        .expect("prepare index_list");
    statement
        .query_map([], |row| row.get(1))
        .expect("query index_list")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect index names")
}

fn seed_database_at_version(connection: &mut Connection, version: u32) {
    let migrations = [
        (
            1,
            include_str!("../src/storage/migrations/0001_initial.sql"),
        ),
        (
            2,
            include_str!("../src/storage/migrations/0002_local_workspace.sql"),
        ),
        (
            3,
            include_str!("../src/storage/migrations/0003_provider_profiles.sql"),
        ),
        (
            4,
            include_str!("../src/storage/migrations/0004_background_tasks.sql"),
        ),
        (
            5,
            include_str!("../src/storage/migrations/0005_knowledge.sql"),
        ),
        (
            6,
            include_str!("../src/storage/migrations/0006_embedding_vectors.sql"),
        ),
        (
            7,
            include_str!("../src/storage/migrations/0007_pending_document_manifest.sql"),
        ),
        (
            8,
            include_str!("../src/storage/migrations/0008_provider_profile_revisions.sql"),
        ),
        (
            9,
            include_str!("../src/storage/migrations/0009_knowledge_fts.sql"),
        ),
        (
            10,
            include_str!("../src/storage/migrations/0010_retrieval_audits.sql"),
        ),
        (
            11,
            include_str!("../src/storage/migrations/0011_agent_runs.sql"),
        ),
        (
            12,
            include_str!("../src/storage/migrations/0012_agent_memory.sql"),
        ),
        (
            13,
            include_str!("../src/storage/migrations/0013_backfill_summary_source.sql"),
        ),
        (
            14,
            include_str!("../src/storage/migrations/0014_permission_rules.sql"),
        ),
        (
            15,
            include_str!("../src/storage/migrations/0015_domain_packages.sql"),
        ),
        (
            16,
            include_str!("../src/storage/migrations/0016_steel_datasets.sql"),
        ),
        (
            17,
            include_str!("../src/storage/migrations/0017_mcp_servers.sql"),
        ),
        (
            18,
            include_str!("../src/storage/migrations/0018_mcp_legacy_sse.sql"),
        ),
        (
            19,
            include_str!("../src/storage/migrations/0019_steel_models.sql"),
        ),
        (
            20,
            include_str!("../src/storage/migrations/0020_sklearn_model_kind.sql"),
        ),
    ];

    for (migration_version, sql) in migrations.into_iter().take(version as usize) {
        connection
            .execute_batch(sql)
            .expect("seed migration fixture");
        connection
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'fixture')",
                params![migration_version],
            )
            .expect("record migration fixture");
        connection
            .pragma_update(None, "user_version", migration_version)
            .expect("set migration fixture version");
    }
}

fn foreign_key_columns(conn: &Connection, table: &str) -> usize {
    let mut statement = conn
        .prepare(&format!("PRAGMA foreign_key_list({table})"))
        .expect("prepare foreign_key_list");
    statement
        .query_map([], |_| Ok(()))
        .expect("query foreign_key_list")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect foreign keys")
        .len()
}

#[test]
fn migrates_empty_database_to_latest_schema() {
    let mut conn = Connection::open_in_memory().expect("open memory database");

    let report = migrate(&mut conn).expect("migrate empty database");

    assert_eq!(
        report.applied_versions,
        (1..=latest_version()).collect::<Vec<_>>()
    );
    assert_eq!(user_version(&conn), latest_version());
    assert!(columns(&conn, "conversations").contains(&"workspace_id".to_string()));
    assert!(!columns(&conn, "conversations").contains(&"user_id".to_string()));
    assert!(columns(&conn, "provider_profiles").contains(&"secret_ref".to_string()));
    assert!(columns(&conn, "provider_profiles").contains(&"revision".to_string()));
    for column in [
        "source_message_id",
        "source_run_id",
        "confidence",
        "status",
        "dedup_key",
    ] {
        assert!(columns(&conn, "memories").contains(&column.to_string()));
    }
    assert!(
        columns(&conn, "conversation_summaries").contains(&"source_message_ids_json".to_string())
    );
    assert!(columns(&conn, "provider_profiles").contains(&"secret_generation".to_string()));
    assert!(columns(&conn, "provider_defaults").contains(&"capability".to_string()));
    assert!(columns(&conn, "background_tasks").contains(&"checkpoint_json".to_string()));
    assert!(columns(&conn, "knowledge_bases").contains(&"workspace_id".to_string()));
    assert!(
        columns(&conn, "knowledge_document_versions").contains(&"expected_chunk_count".to_string())
    );
    assert!(columns(&conn, "knowledge_document_versions").contains(&"manifest_sealed".to_string()));
    assert!(columns(&conn, "knowledge_chunks").contains(&"source_location_json".to_string()));
    assert!(columns(&conn, "knowledge_chunk_embeddings").contains(&"vector_key".to_string()));
    assert!(columns(&conn, "knowledge_vectors").contains(&"vector_blob".to_string()));
    assert!(columns(&conn, "retrieval_audits").contains(&"evidence_json".to_string()));
    assert!(columns(&conn, "agent_runs").contains(&"next_sequence".to_string()));
    assert!(columns(&conn, "agent_run_events").contains(&"event_json".to_string()));
    assert!(
        index_names(&conn, "agent_run_events").contains(&"idx_agent_run_events_replay".to_string())
    );
    assert_eq!(foreign_key_columns(&conn, "agent_runs"), 5);
    assert_eq!(foreign_key_columns(&conn, "agent_run_events"), 3);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row
            .get::<_, i64>(0))
            .expect("count schema migrations"),
        i64::from(latest_version())
    );
    assert!(columns(&conn, "permission_rules").contains(&"source_json".to_string()));
    assert!(columns(&conn, "permission_rules").contains(&"revoked_at".to_string()));
    assert!(columns(&conn, "domain_packages").contains(&"package_sha256".to_string()));
    assert!(columns(&conn, "domain_packages").contains(&"active".to_string()));
    assert!(mcp_transport_accepts_legacy_sse(&conn));
}

#[test]
fn every_supported_database_version_upgrades_to_latest() {
    for version in 0..=latest_version() {
        let mut conn = Connection::open_in_memory().expect("open migration fixture");
        seed_database_at_version(&mut conn, version);

        let report = migrate(&mut conn).expect("upgrade migration fixture");

        assert_eq!(user_version(&conn), latest_version());
        assert_eq!(
            report.applied_versions,
            ((version + 1)..=latest_version()).collect::<Vec<_>>()
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row
                .get::<_, i64>(0))
                .expect("count upgraded migrations"),
            i64::from(latest_version())
        );
    }
}

#[test]
fn background_task_schema_enforces_state_invariants() {
    let mut conn = Connection::open_in_memory().expect("open memory database");
    migrate(&mut conn).expect("migrate schema");
    let invalid = [
        ("failed", 50, None, None),
        ("queued", 50, None, Some("unexpected")),
        ("completed", 99, None, None),
        ("completed", 100, Some("2099-01-01T00:00:00Z"), None),
    ];

    for (index, (state, progress, next_run_at, error_code)) in invalid.into_iter().enumerate() {
        let result = conn.execute(
            "INSERT INTO background_tasks
             (id, workspace_id, kind, state, payload_json, attempt, next_run_at, progress,
              error_code, cancel_requested, created_at, updated_at)
             VALUES (?1, 'workspace-a', 'demo', ?2, '{}', 0, ?3, ?4, ?5, 0,
                     '2026-07-31T00:00:00Z', '2026-07-31T00:00:00Z')",
            params![
                format!("invalid-{index}"),
                state,
                next_run_at,
                progress,
                error_code
            ],
        );
        assert!(result.is_err(), "schema accepted invalid {state} task");
    }
}

#[test]
fn migrates_legacy_schema_without_losing_local_data() {
    let mut conn = Connection::open_in_memory().expect("open memory database");
    conn.execute_batch(LEGACY_SCHEMA)
        .expect("create legacy schema");
    conn.execute_batch(
        r#"
        CREATE TABLE cloud_jobs (id TEXT PRIMARY KEY);
        INSERT INTO cloud_jobs (id) VALUES ('job-1');
        INSERT INTO conversations
          (id, user_id, title, created_at, updated_at, pinned, archived)
          VALUES ('c1', 'old-user', 'legacy chat', '2024-01-01T00:00:00Z', '2024-01-02T00:00:00Z', 1, 0);
        INSERT INTO messages
          (id, user_id, conversation_id, role, content, response_json, created_at)
          VALUES ('m0', 'old-user', 'c1', 'agent', 'earlier', NULL, '2023-12-31T00:00:00Z');
        INSERT INTO messages
          (id, user_id, conversation_id, role, content, response_json, created_at)
          VALUES ('m1', 'old-user', 'c1', 'user', 'hello', NULL, '2024-01-01T00:00:00Z');
        INSERT INTO conversation_drafts
          (user_id, conversation_id, content, updated_at)
          VALUES ('old-user', 'c1', 'draft', '2024-01-03T00:00:00Z');
        INSERT INTO memories
          (id, user_id, scope, type, title, description, body, tags_json, enabled, archived_at, created_at, updated_at)
          VALUES ('memory-1', 'old-user', 'global', 'fact', 'title', '', 'body', '[]', 1, NULL,
                  '2024-01-01T00:00:00Z', '2024-01-02T00:00:00Z');
        INSERT INTO conversation_summaries
          (id, user_id, conversation_id, summary, covered_message_id, created_at, updated_at)
          VALUES ('summary-1', 'old-user', 'c1', 'summary', 'm1',
                  '2024-01-02T00:00:00Z', '2024-01-02T00:00:00Z');
        INSERT INTO settings (user_id, key, value_json, updated_at)
          VALUES ('old-user', 'theme', '"light"', '2024-01-01T00:00:00Z');
        INSERT INTO settings (user_id, key, value_json, updated_at)
          VALUES ('newer-user', 'theme', '"dark"', '2024-02-01T00:00:00Z');
        INSERT INTO settings (user_id, key, value_json, updated_at)
          VALUES ('old-user', 'cloud_api_base', '"https://private.invalid"', '2024-03-01T00:00:00Z');
        "#,
    )
    .expect("seed legacy data");

    let report = migrate(&mut conn).expect("migrate legacy database");

    assert_eq!(report.legacy_cloud_jobs, 1);
    assert_eq!(report.legacy_cloud_settings, 1);
    for table in [
        "conversations",
        "messages",
        "conversation_drafts",
        "memories",
        "conversation_summaries",
        "settings",
    ] {
        assert!(columns(&conn, table).contains(&"workspace_id".to_string()));
        assert!(!columns(&conn, table).contains(&"user_id".to_string()));
    }
    assert_eq!(
        conn.query_row(
            "SELECT workspace_id FROM conversations WHERE id = 'c1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read migrated conversation"),
        "local"
    );
    assert_eq!(
        conn.query_row("SELECT content FROM messages WHERE id = 'm1'", [], |row| {
            row.get::<_, String>(0)
        })
        .expect("read migrated message"),
        "hello"
    );
    assert_eq!(
        conn.query_row(
            "SELECT content FROM conversation_drafts WHERE conversation_id = 'c1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read migrated draft"),
        "draft"
    );
    assert_eq!(
        conn.query_row(
            "SELECT body FROM memories WHERE id = 'memory-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read migrated memory"),
        "body"
    );
    assert_eq!(
        conn.query_row(
            "SELECT summary FROM conversation_summaries WHERE id = 'summary-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read migrated summary"),
        "summary"
    );
    assert_eq!(
        conn.query_row(
            "SELECT source_message_ids_json FROM conversation_summaries WHERE id = 'summary-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read migrated summary sources"),
        "[\"m0\",\"m1\"]"
    );
    assert_eq!(
        conn.query_row(
            "SELECT value_json FROM settings WHERE workspace_id = 'local' AND key = 'theme'",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("read newest migrated setting"),
        "\"dark\""
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM legacy_settings_archive", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("count archived settings"),
        3
    );
    assert!(conn
        .query_row(
            "SELECT value_json FROM settings WHERE key = 'cloud_api_base'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .expect("query cloud setting")
        .is_none());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM cloud_jobs", [], |row| row
            .get::<_, i64>(0))
            .expect("count retained cloud jobs"),
        1
    );
}

#[test]
fn current_database_migration_is_idempotent() {
    let mut conn = Connection::open_in_memory().expect("open memory database");
    migrate(&mut conn).expect("first migration");

    let report = migrate(&mut conn).expect("second migration");

    assert!(report.applied_versions.is_empty());
    assert_eq!(user_version(&conn), latest_version());
}

#[test]
fn version_twelve_database_receives_summary_source_backfill() {
    let mut conn = Connection::open_in_memory().expect("open memory database");
    migrate(&mut conn).expect("create current schema");
    conn.execute_batch(
        r#"
        INSERT INTO conversations
          (id, workspace_id, title, created_at, updated_at, pinned, archived)
          VALUES ('c-v12', 'local', 'v12 chat', 't0', 't2', 0, 0);
        INSERT INTO messages
          (id, workspace_id, conversation_id, role, content, response_json, created_at)
          VALUES ('m-v12-0', 'local', 'c-v12', 'user', 'first', NULL, 't0');
        INSERT INTO messages
          (id, workspace_id, conversation_id, role, content, response_json, created_at)
          VALUES ('m-v12-1', 'local', 'c-v12', 'agent', 'second', NULL, 't1');
        INSERT INTO conversation_summaries
          (id, workspace_id, conversation_id, summary, covered_message_id,
           source_message_ids_json, created_at, updated_at)
          VALUES ('s-v12', 'local', 'c-v12', 'summary', 'm-v12-1', '[]', 't2', 't2');
        DROP TABLE steel_dataset_columns;
        DROP TABLE steel_datasets;
        DROP TABLE steel_models;
        DROP TABLE mcp_servers;
        DELETE FROM schema_migrations WHERE version > 12;
        PRAGMA user_version = 12;
        "#,
    )
    .expect("simulate version twelve database");

    let report = migrate(&mut conn).expect("apply post-v12 migrations");

    assert_eq!(
        report.applied_versions,
        vec![13, 14, 15, 16, 17, 18, 19, 20]
    );
    assert_eq!(
        conn.query_row(
            "SELECT source_message_ids_json FROM conversation_summaries WHERE id = 's-v12'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "[\"m-v12-0\",\"m-v12-1\"]"
    );
}

#[test]
fn version_nineteen_database_requires_v20_for_sklearn_model_kind() {
    let mut conn = Connection::open_in_memory().expect("open memory database");
    seed_database_at_version(&mut conn, 19);

    let legacy_insert = conn.execute(
        "INSERT INTO steel_models
         (workspace_id, id, lineage_id, kind, version, source_task_id,
          model_sha256, manifest_json, artifact_json, model_base64, is_active, created_at)
         VALUES ('local', 'legacy-sklearn', 'sklearn:dataset-1', 'sklearn_artifact', 1, NULL,
                 ?1, '{}', '{}', NULL, 1, 't')",
        params!["a".repeat(64)],
    );
    assert!(
        legacy_insert.is_err(),
        "v19 schema must not accept sklearn artifacts before migration 20"
    );

    let report = migrate(&mut conn).expect("apply v20 model-kind migration");
    assert_eq!(report.applied_versions, vec![20]);

    conn.execute(
        "INSERT INTO steel_models
         (workspace_id, id, lineage_id, kind, version, source_task_id,
          model_sha256, manifest_json, artifact_json, model_base64, is_active, created_at)
         VALUES ('local', 'current-sklearn', 'sklearn:dataset-1', 'sklearn_artifact', 1, NULL,
                 ?1, '{}', '{}', NULL, 1, 't')",
        params!["b".repeat(64)],
    )
    .expect("v20 schema accepts sklearn artifacts");
}

#[test]
fn file_database_uses_wal_and_ordered_migrations() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-migration-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let (conn, report) = database::open(&path).expect("open migrated file database");
    let journal_mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("read journal mode");
    let version = user_version(&conn);
    drop(conn);
    std::fs::remove_file(&path).expect("remove test database");

    assert_eq!(journal_mode, "wal");
    assert_eq!(version, latest_version());
    assert_eq!(
        report.applied_versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]
    );
}

fn mcp_transport_accepts_legacy_sse(conn: &Connection) -> bool {
    conn.execute(
        "INSERT INTO mcp_servers
         (id, workspace_id, display_name, server_id, transport, url, args_json,
          inherited_env_json, env_names_json, timeout_ms, enabled, created_at, updated_at)
         VALUES ('mcp-sse-fixture', 'local', 'SSE fixture', 'sse-fixture', 'sse',
                 'http://127.0.0.1:1/sse', '[]', '[]', '[]', 1000, 1, 't', 't')",
        [],
    )
    .is_ok()
}
#[test]
fn rejects_database_from_newer_bloomery() {
    let mut conn = Connection::open_in_memory().expect("open memory database");
    conn.pragma_update(None, "user_version", 999_u32)
        .expect("set future version");

    let error = migrate(&mut conn).expect_err("future database must be rejected");

    assert_eq!(error.code(), "database_too_new");
    assert_eq!(user_version(&conn), 999);
}

#[test]
fn failed_migration_rolls_back_schema_and_version() {
    let mut conn = Connection::open_in_memory().expect("open memory database");
    conn.execute_batch(LEGACY_SCHEMA)
        .expect("create version-one schema");
    conn.execute(
        "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
         VALUES ('rollback-conversation', 'legacy-user', 'keep me', 't1', 't1')",
        [],
    )
    .expect("seed rollback conversation");
    conn.execute_batch("DROP TABLE messages; PRAGMA user_version = 1;")
        .expect("make version-one schema incomplete");

    let error = migrate(&mut conn).expect_err("incomplete schema must fail");

    assert_eq!(error.code(), "migration_failed");
    assert_eq!(user_version(&conn), 1);
    assert!(columns(&conn, "conversations").contains(&"user_id".to_string()));
    assert!(!columns(&conn, "conversations").contains(&"workspace_id".to_string()));
    assert_eq!(
        conn.query_row(
            "SELECT title FROM conversations WHERE id = 'rollback-conversation'",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("read rollback conversation"),
        "keep me"
    );
}
#[test]
fn globally_unique_content_ids_survive_workspace_mapping() {
    let mut conn = Connection::open_in_memory().expect("open memory database");
    conn.execute_batch(LEGACY_SCHEMA)
        .expect("create legacy schema");
    conn.execute(
        "INSERT INTO conversations (id, user_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["same-id", "user-a", "a", "2024-01-01T00:00:00Z", "2024-01-01T00:00:00Z"],
    )
    .expect("insert first conversation");
    conn.execute(
        "INSERT OR REPLACE INTO conversations (id, user_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["same-id", "user-b", "b", "2024-01-01T00:00:00Z", "2024-01-01T00:00:00Z"],
    )
    .expect("replace conversation under globally unique id");

    migrate(&mut conn).expect("globally unique legacy primary keys migrate");

    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM conversations WHERE id = 'same-id'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .expect("count migrated conversation"),
        1
    );
}
