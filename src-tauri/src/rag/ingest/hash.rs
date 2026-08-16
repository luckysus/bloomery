use super::IngestError;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

const BUFFER_SIZE: usize = 64 * 1024;
const SNIFF_SIZE: usize = 8 * 1024;

pub(super) struct StagedDigest {
    pub byte_len: u64,
    pub content_sha256: String,
    pub prefix: Vec<u8>,
}

pub(super) fn stage_and_hash(
    mut input: File,
    staged: &Path,
    max_bytes: u64,
) -> Result<StagedDigest, IngestError> {
    if !input
        .metadata()
        .map_err(|error| IngestError::io("source_unavailable", "inspect source", error))?
        .is_file()
    {
        return Err(IngestError::new(
            "source_not_file",
            "selected source is not a regular file",
        ));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staged)
        .map_err(|error| IngestError::io("storage_io", "create staged source", error))?;
    let mut hasher = Sha256::new();
    let mut prefix = Vec::with_capacity(SNIFF_SIZE);
    let mut byte_len = 0u64;
    let mut buffer = [0u8; BUFFER_SIZE];

    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| IngestError::io("source_unavailable", "read source", error))?;
        if count == 0 {
            break;
        }
        byte_len = byte_len
            .checked_add(count as u64)
            .ok_or_else(|| IngestError::new("file_too_large", "source size overflow"))?;
        if byte_len > max_bytes {
            return Err(IngestError::new(
                "file_too_large",
                format!("source exceeds the {max_bytes}-byte limit"),
            ));
        }
        let sniff_count = (SNIFF_SIZE - prefix.len()).min(count);
        prefix.extend_from_slice(&buffer[..sniff_count]);
        hasher.update(&buffer[..count]);
        output
            .write_all(&buffer[..count])
            .map_err(|error| IngestError::io("storage_io", "write staged source", error))?;
    }
    if byte_len == 0 {
        return Err(IngestError::new("empty_file", "source file is empty"));
    }
    output
        .sync_all()
        .map_err(|error| IngestError::io("storage_io", "sync staged source", error))?;

    Ok(StagedDigest {
        byte_len,
        content_sha256: format!("{:x}", hasher.finalize()),
        prefix,
    })
}

pub(super) fn verify_object(
    path: &Path,
    expected_len: u64,
    expected_sha256: &str,
) -> Result<(), IngestError> {
    let mut file = File::open(path)
        .map_err(|error| IngestError::io("storage_io", "open stored object", error))?;
    let mut hasher = Sha256::new();
    let mut byte_len = 0u64;
    let mut buffer = [0u8; BUFFER_SIZE];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| IngestError::io("storage_io", "read stored object", error))?;
        if count == 0 {
            break;
        }
        byte_len = byte_len.saturating_add(count as u64);
        if byte_len > expected_len {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = format!("{:x}", hasher.finalize());
    if byte_len == expected_len && digest == expected_sha256 {
        Ok(())
    } else {
        Err(IngestError::new(
            "content_store_corrupt",
            "stored content-addressed object does not match its key",
        ))
    }
}
