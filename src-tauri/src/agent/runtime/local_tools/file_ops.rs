use super::{
    check_cancelled, display_path, invalid, io_error, LocalScope, DEFAULT_READ_BYTES,
    MAX_DIRECTORY_ENTRIES, MAX_READ_BYTES, MAX_WRITE_BYTES,
};
use super::{CancellationToken, ToolExecutionError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRequest {
    path: String,
    #[serde(default)]
    offset_bytes: u64,
    max_bytes: Option<usize>,
}

pub(super) fn read_file(
    scope: &LocalScope,
    arguments: Value,
    cancellation: &CancellationToken,
) -> Result<Value, ToolExecutionError> {
    let request: ReadRequest = serde_json::from_value(arguments).map_err(invalid)?;
    let max_bytes = request.max_bytes.unwrap_or(DEFAULT_READ_BYTES);
    if !(1..=MAX_READ_BYTES).contains(&max_bytes) {
        return Err(invalid("max_bytes is outside the supported range"));
    }
    let path = scope.resolve(&request.path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    check_cancelled(cancellation)?;
    let mut file = options.open(path.canonical_path()).map_err(io_error)?;
    scope.verify_file(&path, &file)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(invalid("path must refer to a regular UTF-8 file"));
    }
    file.seek(SeekFrom::Start(request.offset_bytes))
        .map_err(io_error)?;
    let mut bytes = Vec::with_capacity(max_bytes + 1);
    file.take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    check_cancelled(cancellation)?;
    let truncated = bytes.len() > max_bytes;
    bytes.truncate(max_bytes);
    let content = match std::str::from_utf8(&bytes) {
        Ok(content) => content.to_string(),
        Err(error) if error.error_len().is_none() && truncated => {
            String::from_utf8(bytes[..error.valid_up_to()].to_vec()).map_err(invalid)?
        }
        Err(_) => {
            return Err(ToolExecutionError::new(
                "invalid_utf8",
                "file chunk is not valid UTF-8; use a character boundary for offset_bytes",
            ))
        }
    };
    Ok(json!({
        "path": display_path(path.canonical_path()), "content": content,
        "offset_bytes": request.offset_bytes, "next_offset_bytes": request.offset_bytes.saturating_add(content.len() as u64),
        "total_bytes": metadata.len(), "truncated": truncated
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListRequest {
    path: String,
    max_entries: Option<usize>,
}

pub(super) fn list_directory(
    scope: &LocalScope,
    arguments: Value,
    cancellation: &CancellationToken,
) -> Result<Value, ToolExecutionError> {
    let request: ListRequest = serde_json::from_value(arguments).map_err(invalid)?;
    let max_entries = request.max_entries.unwrap_or(MAX_DIRECTORY_ENTRIES);
    if !(1..=MAX_DIRECTORY_ENTRIES).contains(&max_entries) {
        return Err(invalid("max_entries is outside the supported range"));
    }
    let path = scope.resolve(&request.path)?;
    let _directory = scope.open_directory(&path)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in fs::read_dir(path.canonical_path()).map_err(io_error)? {
        check_cancelled(cancellation)?;
        let entry = entry.map_err(io_error)?;
        if entries.len() >= max_entries {
            truncated = true;
            break;
        }
        let kind = entry.file_type().map_err(io_error)?;
        entries.push(json!({
            "name": entry.file_name().to_string_lossy(),
            "kind": if kind.is_symlink() { "symlink" } else if kind.is_dir() { "directory" } else if kind.is_file() { "file" } else { "other" }
        }));
    }
    entries.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    check_cancelled(cancellation)?;
    Ok(
        json!({"path": display_path(path.canonical_path()), "entries": entries, "truncated": truncated}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteRequest {
    path: String,
    content: String,
    #[serde(default)]
    overwrite: bool,
}

pub(super) fn write_file(
    scope: &LocalScope,
    arguments: Value,
    cancellation: &CancellationToken,
) -> Result<Value, ToolExecutionError> {
    let request: WriteRequest = serde_json::from_value(arguments).map_err(invalid)?;
    if request.content.len() > MAX_WRITE_BYTES {
        return Err(invalid("content exceeds the supported UTF-8 byte limit"));
    }
    let target = scope.resolve(&request.path)?;
    let target_path = target.canonical_path();
    let parent = target_path
        .parent()
        .ok_or_else(|| invalid("file must have a parent directory"))?;
    let parent = scope.resolve(&display_path(parent))?;
    let _directory = scope.open_directory(&parent)?;
    if target_path.exists() && !request.overwrite {
        return Err(ToolExecutionError::new(
            "file_exists",
            "set overwrite=true to replace an existing file",
        ));
    }
    if target_path.is_dir() {
        return Err(invalid("cannot replace a directory with a file"));
    }
    check_cancelled(cancellation)?;
    let staged_path = parent
        .canonical_path()
        .join(format!(".suna-write-{}.tmp", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_path)
        .map_err(io_error)?;
    let staged = TemporaryFile(staged_path.clone());
    let staged_authorized = scope.resolve(&display_path(&staged_path))?;
    scope.verify_file(&staged_authorized, &file)?;
    file.write_all(request.content.as_bytes())
        .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    drop(file);
    check_cancelled(cancellation)?;
    scope
        .roots
        .verify_target(&target, target_path)
        .map_err(super::path_error)?;
    if request.overwrite {
        fs::rename(&staged_path, target_path).map_err(io_error)?;
    } else {
        fs::hard_link(&staged_path, target_path).map_err(io_error)?;
    }
    drop(staged);
    Ok(json!({"path": display_path(target_path), "bytes_written": request.content.len()}))
}

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
