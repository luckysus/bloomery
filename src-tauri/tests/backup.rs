use bloomery::storage::backup::{create_backup, preview_backup, restore_backup};
use bloomery::storage::migrations::migrate;
use rusqlite::Connection;
use serde_json::json;
use std::fs;
use std::io::Write;
use uuid::Uuid;
use zip::write::SimpleFileOptions;

fn fixture_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("bloomery-backup-{label}-{}", Uuid::new_v4()))
}

#[test]
fn backup_round_trip_restores_database_and_content_without_staging_files() {
    let root = fixture_root("round-trip");
    let source_root = root.join("source-content");
    let target_root = root.join("target-content");
    fs::create_dir_all(source_root.join("objects/sha256/ab")).expect("create source root");
    fs::create_dir_all(source_root.join("indexes/current")).expect("create index root");
    fs::create_dir_all(source_root.join(".staging")).expect("create staging root");
    fs::write(
        source_root.join("objects/sha256/ab/document"),
        "local document",
    )
    .expect("write content");
    fs::write(source_root.join("indexes/current/index.bin"), "local index").expect("write index");
    fs::write(source_root.join("unrelated.log"), "must not be backed up")
        .expect("write unrelated file");
    fs::write(source_root.join(".staging/partial"), "temporary").expect("write staging");

    let source_database = root.join("source.sqlite3");
    let mut connection = Connection::open(&source_database).expect("open database");
    migrate(&mut connection).expect("migrate database");
    connection
        .execute(
            "INSERT INTO settings (workspace_id, key, value_json, updated_at)
             VALUES ('local', 'backup.test', '{\"ok\":true}', 'now')",
            [],
        )
        .expect("write setting");

    let archive = root.join("bloomery.bloomery-backup");
    let summary = create_backup(&connection, &source_database, &source_root, &archive)
        .expect("create backup");
    assert_eq!(summary.content_file_count, 2);
    assert!(summary.database_bytes > 0);

    let restored_database = root.join("target.sqlite3");
    restore_backup(&archive, &restored_database, &target_root).expect("restore backup");
    let restored = Connection::open(restored_database).expect("open restored database");
    let value: String = restored
        .query_row(
            "SELECT value_json FROM settings WHERE workspace_id = 'local' AND key = 'backup.test'",
            [],
            |row| row.get(0),
        )
        .expect("read restored setting");
    assert_eq!(value, "{\"ok\":true}");
    assert_eq!(
        fs::read_to_string(target_root.join("objects/sha256/ab/document")).expect("read content"),
        "local document"
    );
    assert_eq!(
        fs::read_to_string(target_root.join("indexes/current/index.bin")).expect("read index"),
        "local index"
    );
    assert!(!target_root.join("unrelated.log").exists());
    assert!(!target_root.join(".staging/partial").exists());

    drop(restored);
    drop(connection);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn backup_preview_validates_archive_without_mutating_restore_targets() {
    let root = fixture_root("preview");
    fs::create_dir_all(&root).expect("create root");
    let database = root.join("source.sqlite3");
    let mut connection = Connection::open(&database).expect("open database");
    migrate(&mut connection).expect("migrate database");
    let archive = root.join("preview.bloomery-backup");
    create_backup(&connection, &database, &root.join("content"), &archive).expect("create backup");

    let preview = preview_backup(&archive).expect("preview backup");
    assert_eq!(preview.archive_path, archive.to_string_lossy());
    assert!(preview.database_bytes > 0);
    assert_eq!(preview.content_file_count, 0);
    assert_eq!(preview.content_bytes, 0);
    assert!(!root.join("target.sqlite3").exists());
    assert!(!root.join("target-content").exists());
    drop(connection);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn restore_replaces_existing_database_and_content() {
    let root = fixture_root("replace");
    let source_content = root.join("source-content");
    let target_content = root.join("target-content");
    fs::create_dir_all(source_content.join("objects")).expect("create source content");
    fs::write(source_content.join("objects/new"), "new content").expect("write new content");

    let source_database = root.join("source.sqlite3");
    let mut source = Connection::open(&source_database).expect("open source database");
    migrate(&mut source).expect("migrate source database");
    source
        .execute(
            "INSERT INTO settings (workspace_id, key, value_json, updated_at)
             VALUES ('local', 'backup.replace', '{\"value\":\"new\"}', 'now')",
            [],
        )
        .expect("write source setting");
    let archive = root.join("replace.bloomery-backup");
    create_backup(&source, &source_database, &source_content, &archive).expect("create backup");

    fs::create_dir_all(target_content.join("objects")).expect("create target content");
    fs::write(target_content.join("objects/old"), "old content").expect("write old content");
    let target_database = root.join("target.sqlite3");
    let mut target = Connection::open(&target_database).expect("open target database");
    migrate(&mut target).expect("migrate target database");
    drop(target);

    restore_backup(&archive, &target_database, &target_content).expect("replace backup");
    let restored = Connection::open(target_database).expect("open replaced database");
    let value: String = restored
        .query_row(
            "SELECT value_json FROM settings WHERE workspace_id = 'local' AND key = 'backup.replace'",
            [],
            |row| row.get(0),
        )
        .expect("read replaced setting");
    assert_eq!(value, "{\"value\":\"new\"}");
    assert_eq!(
        fs::read_to_string(target_content.join("objects/new")).expect("read new content"),
        "new content"
    );
    assert!(!target_content.join("objects/old").exists());

    drop(restored);
    drop(source);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn restore_rejects_archive_path_traversal_before_writing_files() {
    let root = fixture_root("unsafe");
    fs::create_dir_all(&root).expect("create root");
    let archive = root.join("unsafe.zip");
    let file = fs::File::create(&archive).expect("create archive");
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file("manifest.json", SimpleFileOptions::default())
        .expect("start manifest");
    let manifest = serde_json::to_vec(&json!({
        "formatVersion": 1,
        "createdAt": "2026-08-07T00:00:00Z",
        "databaseEntry": "bloomery.sqlite3",
        "contentPrefix": "content/",
        "contentFileCount": 0
    }))
    .expect("encode manifest");
    writer.write_all(&manifest).expect("write manifest");
    writer
        .start_file("content/../escaped", SimpleFileOptions::default())
        .expect("start unsafe entry");
    writer
        .write_all(b"must not extract")
        .expect("write unsafe entry");
    writer.finish().expect("finish archive");

    let error = restore_backup(
        &archive,
        &root.join("restored.sqlite3"),
        &root.join("restored-content"),
    )
    .expect_err("path traversal must be rejected");
    assert!(error.contains("unsafe backup entry"));
    assert!(!root.join("escaped").exists());
    fs::remove_dir_all(root).expect("remove fixture");
}

fn manifest_bytes(format_version: u32, content_file_count: usize) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "formatVersion": format_version,
        "createdAt": "2026-08-08T00:00:00Z",
        "databaseEntry": "bloomery.sqlite3",
        "contentPrefix": "content/",
        "contentFileCount": content_file_count
    }))
    .expect("encode manifest")
}

fn build_archive(
    path: &std::path::Path,
    manifest: &[u8],
    database: &[u8],
    content: &[(&str, &[u8])],
) {
    let file = fs::File::create(path).expect("create archive");
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    writer
        .start_file("manifest.json", options)
        .expect("start manifest");
    writer.write_all(manifest).expect("write manifest");
    writer
        .start_file("bloomery.sqlite3", options)
        .expect("start database");
    writer.write_all(database).expect("write database");
    for (name, bytes) in content {
        writer.start_file(*name, options).expect("start content");
        writer.write_all(bytes).expect("write content");
    }
    writer.finish().expect("finish archive");
}

fn seed_target(database: &std::path::Path, content: &std::path::Path) {
    let mut connection = Connection::open(database).expect("open target database");
    migrate(&mut connection).expect("migrate target database");
    connection
        .execute(
            "INSERT INTO settings (workspace_id, key, value_json, updated_at)
             VALUES ('local', 'sentinel', '{\"keep\":true}', 'now')",
            [],
        )
        .expect("seed sentinel row");
    drop(connection);
    fs::create_dir_all(content.join("objects")).expect("create target content");
    fs::write(content.join("objects/keep"), "keep").expect("write keep file");
}

fn assert_target_intact(database: &std::path::Path, content: &std::path::Path) {
    let connection = Connection::open(database).expect("reopen target database");
    let value: String = connection
        .query_row(
            "SELECT value_json FROM settings WHERE workspace_id = 'local' AND key = 'sentinel'",
            [],
            |row| row.get(0),
        )
        .expect("sentinel row must survive a rejected restore");
    assert_eq!(value, "{\"keep\":true}");
    assert_eq!(
        fs::read_to_string(content.join("objects/keep")).expect("keep file must survive"),
        "keep"
    );
}

fn assert_no_staging_leftovers(root: &std::path::Path) {
    for entry in fs::read_dir(root).expect("read fixture root").flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(
            !name.starts_with(".bloomery-restore-")
                && !name.starts_with(".bloomery-content-restore-")
                && !name.starts_with(".bloomery-rollback-"),
            "restore left staging/rollback artifact behind: {name}"
        );
    }
}

#[test]
fn restore_rejects_truncated_archive_without_touching_target() {
    let root = fixture_root("truncated");
    fs::create_dir_all(&root).expect("create root");
    let source_database = root.join("source.sqlite3");
    let mut source = Connection::open(&source_database).expect("open source database");
    migrate(&mut source).expect("migrate source database");
    source
        .execute(
            "INSERT INTO settings (workspace_id, key, value_json, updated_at)
             VALUES ('local', 'origin', '{\"v\":1}', 'now')",
            [],
        )
        .expect("seed source row");
    let full = root.join("full.bloomery-backup");
    create_backup(
        &source,
        &source_database,
        &root.join("source-content"),
        &full,
    )
    .expect("create backup");
    drop(source);

    // 头部完整、内容截断：仅保留前半段字节，破坏 ZIP 中央目录。
    let bytes = fs::read(&full).expect("read full backup");
    let archive = root.join("truncated.bloomery-backup");
    fs::write(&archive, &bytes[..bytes.len() / 2]).expect("write truncated backup");

    let target_database = root.join("target.sqlite3");
    let target_content = root.join("target-content");
    seed_target(&target_database, &target_content);

    let error = restore_backup(&archive, &target_database, &target_content)
        .expect_err("truncated archive must be rejected");
    assert!(!error.is_empty());
    assert_target_intact(&target_database, &target_content);
    assert_no_staging_leftovers(&root);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn restore_rejects_content_count_mismatch_without_touching_target() {
    let root = fixture_root("count-mismatch");
    fs::create_dir_all(&root).expect("create root");
    let archive = root.join("mismatch.bloomery-backup");
    // manifest 声明 3 个内容文件，归档内实际为 0 个 —— 条目一致性校验必须拒绝。
    build_archive(
        &archive,
        &manifest_bytes(1, 3),
        b"unused database bytes",
        &[],
    );

    let target_database = root.join("target.sqlite3");
    let target_content = root.join("target-content");
    seed_target(&target_database, &target_content);

    let error = restore_backup(&archive, &target_database, &target_content)
        .expect_err("content count mismatch must be rejected");
    assert!(error.contains("content count does not match"));
    assert_target_intact(&target_database, &target_content);
    assert_no_staging_leftovers(&root);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn restore_rejects_incompatible_manifest_version_without_touching_target() {
    let root = fixture_root("bad-version");
    fs::create_dir_all(&root).expect("create root");
    let archive = root.join("version.bloomery-backup");
    build_archive(
        &archive,
        &manifest_bytes(999, 0),
        b"unused database bytes",
        &[],
    );

    let target_database = root.join("target.sqlite3");
    let target_content = root.join("target-content");
    seed_target(&target_database, &target_content);

    let error = restore_backup(&archive, &target_database, &target_content)
        .expect_err("incompatible manifest version must be rejected");
    assert!(error.contains("unsupported backup format"));
    assert_target_intact(&target_database, &target_content);
    assert_no_staging_leftovers(&root);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn restore_rejects_corrupt_database_header_without_touching_target() {
    let root = fixture_root("bad-db");
    fs::create_dir_all(&root).expect("create root");
    let archive = root.join("corrupt.bloomery-backup");
    // manifest 合法、0 个内容文件，但数据库条目不是合法 SQLite 文件。
    build_archive(
        &archive,
        &manifest_bytes(1, 0),
        b"this is definitely not a valid sqlite database header",
        &[],
    );

    let target_database = root.join("target.sqlite3");
    let target_content = root.join("target-content");
    seed_target(&target_database, &target_content);

    let error = restore_backup(&archive, &target_database, &target_content)
        .expect_err("corrupt database must be rejected");
    assert!(error.contains("database"), "unexpected error: {error}");
    assert_target_intact(&target_database, &target_content);
    assert_no_staging_leftovers(&root);
    fs::remove_dir_all(root).expect("remove fixture");
}
