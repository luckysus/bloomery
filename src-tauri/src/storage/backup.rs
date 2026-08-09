use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const FORMAT_VERSION: u32 = 1;
const DATABASE_ENTRY: &str = "bloomery.sqlite3";
const CONTENT_PREFIX: &str = "content/";
const CONTENT_DIRECTORIES: [&str; 2] = ["objects", "indexes"];
const MAX_FILES: usize = 100_000;
const MAX_UNCOMPRESSED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupManifest {
    format_version: u32,
    created_at: String,
    database_entry: String,
    content_prefix: String,
    content_file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub format_version: u32,
    pub archive_path: String,
    pub database_bytes: u64,
    pub content_file_count: usize,
    pub content_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArchiveStats {
    database_bytes: u64,
    content_file_count: usize,
    content_bytes: u64,
    total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Manifest,
    Database,
    Content,
}

pub fn create_backup(
    connection: &Connection,
    database_path: &Path,
    content_root: &Path,
    archive_path: &Path,
) -> Result<BackupSummary, String> {
    if !database_path.is_file() {
        return Err("backup database does not exist".to_string());
    }
    if database_path == archive_path {
        return Err("backup destination must differ from the database".to_string());
    }
    if archive_path.exists() {
        return Err("backup destination already exists".to_string());
    }
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create backup directory failed: {error}"))?;
    }

    let files = collect_content_files(content_root, archive_path)?;
    let temporary_database = archive_path.with_extension(format!("sqlite3.tmp-{}", Uuid::new_v4()));
    let result = create_backup_archive(connection, &temporary_database, archive_path, &files);
    let _ = fs::remove_file(&temporary_database);
    result
}

fn create_backup_archive(
    connection: &Connection,
    temporary_database: &Path,
    archive_path: &Path,
    files: &[(PathBuf, String, u64)],
) -> Result<BackupSummary, String> {
    connection
        .execute(
            "VACUUM INTO ?1",
            [temporary_database.to_string_lossy().as_ref()],
        )
        .map_err(|error| format!("snapshot database failed: {error}"))?;
    let database_bytes = fs::metadata(temporary_database)
        .map_err(|error| format!("read database snapshot failed: {error}"))?
        .len();
    let content_bytes = files.iter().map(|(_, _, size)| *size).sum::<u64>();
    let manifest = BackupManifest {
        format_version: FORMAT_VERSION,
        created_at: Utc::now().to_rfc3339(),
        database_entry: DATABASE_ENTRY.to_string(),
        content_prefix: CONTENT_PREFIX.to_string(),
        content_file_count: files.len(),
    };
    let archive_file = File::create(archive_path)
        .map_err(|error| format!("create backup archive failed: {error}"))?;
    let mut writer = ZipWriter::new(archive_file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let manifest_json = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    writer
        .start_file("manifest.json", options)
        .map_err(|error| format!("write backup manifest failed: {error}"))?;
    writer
        .write_all(&manifest_json)
        .map_err(|error| format!("write backup manifest failed: {error}"))?;

    writer
        .start_file(DATABASE_ENTRY, options)
        .map_err(|error| format!("write database snapshot failed: {error}"))?;
    copy_file(
        &mut File::open(temporary_database).map_err(|error| error.to_string())?,
        &mut writer,
    )
    .map_err(|error| format!("write database snapshot failed: {error}"))?;
    for (path, entry, _) in files {
        writer
            .start_file(entry, options)
            .map_err(|error| format!("write backup content failed: {error}"))?;
        copy_file(
            &mut File::open(path).map_err(|error| error.to_string())?,
            &mut writer,
        )
        .map_err(|error| format!("write backup content failed: {error}"))?;
    }
    writer
        .finish()
        .map_err(|error| format!("finish backup archive failed: {error}"))?;

    Ok(BackupSummary {
        format_version: FORMAT_VERSION,
        archive_path: archive_path.to_string_lossy().into_owned(),
        database_bytes,
        content_file_count: files.len(),
        content_bytes,
    })
}

pub fn preview_backup(archive_path: &Path) -> Result<BackupSummary, String> {
    if !archive_path.is_file() {
        return Err("backup archive does not exist".to_string());
    }
    let archive_file =
        File::open(archive_path).map_err(|error| format!("open backup archive failed: {error}"))?;
    let mut archive = ZipArchive::new(archive_file)
        .map_err(|error| format!("read backup archive failed: {error}"))?;
    let manifest = read_manifest(&mut archive)?;
    let stats = validate_archive(&mut archive, &manifest)?;
    if stats.total_bytes > MAX_UNCOMPRESSED_BYTES {
        return Err("backup exceeds the uncompressed size limit".to_string());
    }
    if stats.database_bytes == 0 {
        return Err("backup database is empty".to_string());
    }
    Ok(BackupSummary {
        format_version: manifest.format_version,
        archive_path: archive_path.to_string_lossy().into_owned(),
        database_bytes: stats.database_bytes,
        content_file_count: stats.content_file_count,
        content_bytes: stats.content_bytes,
    })
}

pub fn restore_backup(
    archive_path: &Path,
    database_path: &Path,
    content_root: &Path,
) -> Result<BackupSummary, String> {
    if !archive_path.is_file() {
        return Err("backup archive does not exist".to_string());
    }
    if database_path.is_dir() {
        return Err("restore database destination must be a file".to_string());
    }
    if content_root.is_file() {
        return Err("restore content destination must be a directory".to_string());
    }

    let archive_file =
        File::open(archive_path).map_err(|error| format!("open backup archive failed: {error}"))?;
    let mut archive = ZipArchive::new(archive_file)
        .map_err(|error| format!("read backup archive failed: {error}"))?;
    let manifest = read_manifest(&mut archive)?;
    let stats = validate_archive(&mut archive, &manifest)?;
    if stats.total_bytes > MAX_UNCOMPRESSED_BYTES {
        return Err("backup exceeds the uncompressed size limit".to_string());
    }

    let database_parent = database_path
        .parent()
        .ok_or_else(|| "restore database parent is required".to_string())?;
    let content_parent = content_root
        .parent()
        .ok_or_else(|| "restore content parent is required".to_string())?;
    fs::create_dir_all(database_parent)
        .map_err(|error| format!("create restore database directory failed: {error}"))?;
    fs::create_dir_all(content_parent)
        .map_err(|error| format!("create restore content directory failed: {error}"))?;

    let staging_database =
        database_parent.join(format!(".bloomery-restore-{}.sqlite3", Uuid::new_v4()));
    let staging_content =
        content_parent.join(format!(".bloomery-content-restore-{}", Uuid::new_v4()));
    let result = extract_backup(
        &mut archive,
        &manifest,
        &staging_database,
        &staging_content,
        database_path,
        content_root,
    );
    if result.is_err() {
        let _ = fs::remove_file(&staging_database);
        let _ = fs::remove_dir_all(&staging_content);
    }
    result.map(|(database_bytes, content_bytes)| BackupSummary {
        format_version: manifest.format_version,
        archive_path: archive_path.to_string_lossy().into_owned(),
        database_bytes,
        content_file_count: stats.content_file_count,
        content_bytes,
    })
}

fn validate_archive(
    archive: &mut ZipArchive<File>,
    manifest: &BackupManifest,
) -> Result<ArchiveStats, String> {
    let mut total_bytes = 0_u64;
    let mut content_file_count = 0_usize;
    let mut content_bytes = 0_u64;
    let mut database_bytes = 0_u64;
    let mut names = HashSet::new();
    let mut database_count = 0_usize;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("read backup entry failed: {error}"))?;
        let name = entry.name().to_string();
        if !names.insert(name.clone()) {
            return Err(format!("duplicate backup entry: {name}"));
        }
        let kind = validate_entry_name(&name)?;
        if entry.is_dir() {
            continue;
        }
        if kind == EntryKind::Database {
            database_count += 1;
            database_bytes = entry.size();
        } else if kind == EntryKind::Content {
            content_file_count += 1;
            if content_file_count > MAX_FILES {
                return Err("backup contains too many content files".to_string());
            }
            content_bytes = content_bytes
                .checked_add(entry.size())
                .ok_or_else(|| "backup is too large".to_string())?;
        }
        total_bytes = total_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "backup is too large".to_string())?;
    }
    if database_count != 1 {
        return Err("backup must contain exactly one database".to_string());
    }
    if content_file_count != manifest.content_file_count {
        return Err("backup content count does not match its manifest".to_string());
    }
    Ok(ArchiveStats {
        database_bytes,
        content_file_count,
        content_bytes,
        total_bytes,
    })
}

fn extract_backup(
    archive: &mut ZipArchive<File>,
    manifest: &BackupManifest,
    staging_database: &Path,
    staging_content: &Path,
    database_path: &Path,
    content_root: &Path,
) -> Result<(u64, u64), String> {
    let mut database_bytes = 0_u64;
    let mut content_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("read backup entry failed: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        match validate_entry_name(entry.name())? {
            EntryKind::Manifest => {}
            EntryKind::Database => {
                if entry.name() != manifest.database_entry {
                    return Err("backup database entry does not match its manifest".to_string());
                }
                let mut target = File::create(staging_database)
                    .map_err(|error| format!("create restored database failed: {error}"))?;
                io::copy(&mut entry, &mut target)
                    .map_err(|error| format!("extract restored database failed: {error}"))?;
                database_bytes = fs::metadata(staging_database)
                    .map_err(|error| error.to_string())?
                    .len();
            }
            EntryKind::Content => {
                let relative = entry
                    .name()
                    .strip_prefix(&manifest.content_prefix)
                    .ok_or_else(|| "invalid backup content entry".to_string())?;
                let target =
                    staging_content.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!("create restored content directory failed: {error}")
                    })?;
                }
                let mut output = File::create(&target)
                    .map_err(|error| format!("create restored content failed: {error}"))?;
                content_bytes = content_bytes
                    .checked_add(
                        io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?,
                    )
                    .ok_or_else(|| "restored content is too large".to_string())?;
            }
        }
    }
    if database_bytes == 0 {
        return Err("backup database is empty".to_string());
    }

    let restored = Connection::open(staging_database)
        .map_err(|error| format!("open restored database failed: {error}"))?;
    let quick_check: String = restored
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| format!("restored database check failed: {error}"))?;
    if quick_check != "ok" {
        return Err("restored database failed integrity check".to_string());
    }
    drop(restored);
    install_restored_files(
        staging_database,
        staging_content,
        database_path,
        content_root,
    )?;
    Ok((database_bytes, content_bytes))
}

fn install_restored_files(
    staging_database: &Path,
    staging_content: &Path,
    database_path: &Path,
    content_root: &Path,
) -> Result<(), String> {
    let parent = database_path
        .parent()
        .ok_or_else(|| "restore database parent is required".to_string())?;
    let rollback_root = parent.join(format!(".bloomery-rollback-{}", Uuid::new_v4()));
    let rollback_content = rollback_root.join("content");
    fs::create_dir_all(&rollback_content)
        .map_err(|error| format!("create restore rollback directory failed: {error}"))?;
    fs::create_dir_all(content_root)
        .map_err(|error| format!("create restore content directory failed: {error}"))?;

    let mut moved_files = Vec::new();
    let mut moved_content = Vec::new();
    let mut installed_content = Vec::new();
    let mut installed_database = false;
    let result = (|| {
        for name in database_sidecars(database_path) {
            move_existing_file(&name, &rollback_root, &mut moved_files)?;
        }
        move_existing_file(database_path, &rollback_root, &mut moved_files)?;
        for directory in CONTENT_DIRECTORIES {
            let target = content_root.join(directory);
            if target.exists() {
                let old = rollback_content.join(directory);
                fs::rename(&target, &old)
                    .map_err(|error| format!("stage existing restore content failed: {error}"))?;
                moved_content.push((old, target));
            }
        }
        fs::rename(staging_database, database_path)
            .map_err(|error| format!("install restored database failed: {error}"))?;
        installed_database = true;
        for directory in CONTENT_DIRECTORIES {
            let staged = staging_content.join(directory);
            if staged.exists() {
                let target = content_root.join(directory);
                fs::rename(&staged, &target)
                    .map_err(|error| format!("install restored content failed: {error}"))?;
                installed_content.push(target);
            }
        }
        Ok::<(), String>(())
    })();

    if let Err(error) = result {
        for path in installed_content {
            let _ = fs::remove_dir_all(path);
        }
        if installed_database {
            let _ = fs::remove_file(database_path);
        }
        for (old, target) in moved_content.into_iter().rev() {
            let _ = fs::rename(old, target);
        }
        for (old, target) in moved_files.into_iter().rev() {
            let _ = fs::rename(old, target);
        }
        let _ = fs::remove_dir_all(staging_content);
        let _ = fs::remove_file(staging_database);
        let _ = fs::remove_dir_all(&rollback_root);
        return Err(error);
    }

    let _ = fs::remove_dir_all(staging_content);
    fs::remove_dir_all(&rollback_root)
        .map_err(|error| format!("remove restore rollback directory failed: {error}"))?;
    Ok(())
}

fn move_existing_file(
    target: &Path,
    rollback_root: &Path,
    moved_files: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), String> {
    if !target.exists() {
        return Ok(());
    }
    let name = target
        .file_name()
        .ok_or_else(|| "restore file name is required".to_string())?;
    let old = rollback_root.join(name);
    fs::rename(target, &old)
        .map_err(|error| format!("stage existing restore file failed: {error}"))?;
    moved_files.push((old, target.to_path_buf()));
    Ok(())
}

fn database_sidecars(database_path: &Path) -> [PathBuf; 3] {
    [
        PathBuf::from(format!("{}-wal", database_path.display())),
        PathBuf::from(format!("{}-shm", database_path.display())),
        PathBuf::from(format!("{}-journal", database_path.display())),
    ]
}

fn read_manifest(archive: &mut ZipArchive<File>) -> Result<BackupManifest, String> {
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|_| "backup manifest is missing".to_string())?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read backup manifest failed: {error}"))?;
    let manifest: BackupManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("parse backup manifest failed: {error}"))?;
    if manifest.format_version != FORMAT_VERSION
        || manifest.database_entry != DATABASE_ENTRY
        || manifest.content_prefix != CONTENT_PREFIX
    {
        return Err("unsupported backup format".to_string());
    }
    Ok(manifest)
}

fn validate_entry_name(name: &str) -> Result<EntryKind, String> {
    let is_safe = |value: &str| {
        !value.is_empty()
            && !value.contains('\\')
            && !value.contains(':')
            && !Path::new(value).is_absolute()
            && Path::new(value)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
    };
    if name == "manifest.json" {
        return Ok(EntryKind::Manifest);
    }
    if name == DATABASE_ENTRY {
        return Ok(EntryKind::Database);
    }
    if let Some(relative) = name.strip_prefix(CONTENT_PREFIX) {
        let relative = relative.trim_end_matches('/');
        if is_safe(relative) {
            return Ok(EntryKind::Content);
        }
    }
    Err(format!("unsafe backup entry: {name}"))
}

fn collect_content_files(
    root: &Path,
    archive_path: &Path,
) -> Result<Vec<(PathBuf, String, u64)>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for directory in CONTENT_DIRECTORIES {
        let path = root.join(directory);
        if path.exists() {
            collect_content_files_from(root, &path, archive_path, &mut files)?;
        }
    }
    if files.len() > MAX_FILES {
        return Err("backup contains too many content files".to_string());
    }
    let total = files.iter().map(|(_, _, size)| *size).sum::<u64>();
    if total > MAX_UNCOMPRESSED_BYTES {
        return Err("backup content exceeds the uncompressed size limit".to_string());
    }
    Ok(files)
}

fn collect_content_files_from(
    root: &Path,
    directory: &Path,
    archive_path: &Path,
    files: &mut Vec<(PathBuf, String, u64)>,
) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("read backup content failed: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read backup content entry failed: {error}"))?;
        let path = entry.path();
        if path == archive_path || entry.file_name() == ".staging" {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect backup content failed: {error}"))?;
        if file_type.is_symlink() {
            return Err(format!(
                "backup content cannot contain symlink: {}",
                path.display()
            ));
        }
        if file_type.is_dir() {
            collect_content_files_from(root, &path, archive_path, files)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        let relative = relative
            .to_str()
            .ok_or_else(|| "backup content path must be valid UTF-8".to_string())?
            .replace(std::path::MAIN_SEPARATOR, "/");
        let size = fs::metadata(&path)
            .map_err(|error| format!("read backup content metadata failed: {error}"))?
            .len();
        files.push((path, format!("{CONTENT_PREFIX}{relative}"), size));
    }
    Ok(())
}

fn copy_file(source: &mut File, destination: &mut impl Write) -> io::Result<u64> {
    io::copy(source, destination)
}
