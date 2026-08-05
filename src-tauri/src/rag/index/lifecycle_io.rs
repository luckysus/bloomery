use super::vector::{IndexError, IndexWatermark};
use crate::rag::model::{ChunkId, DocumentVersionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub(super) const HNSW_BASENAME: &str = "index";
const MANIFEST_FILE: &str = "manifest.json";
const RECORDS_FILE: &str = "records.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RecordMetadata {
    pub version_id: DocumentVersionId,
    pub chunk_id: ChunkId,
}

#[derive(Debug, Serialize, Deserialize)]
struct SidecarManifest {
    generation_id: String,
    watermark: IndexWatermark,
    graph_sha256: String,
    data_sha256: String,
    records_sha256: String,
}

pub(super) fn prepare_root(root: &Path) -> Result<(), IndexError> {
    fs::create_dir_all(root).map_err(io_error)?;
    for entry in fs::read_dir(root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(".tmp-") {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(io_error)?;
        } else {
            fs::remove_file(path).map_err(io_error)?;
        }
    }
    Ok(())
}

pub(super) fn temp_generation(root: &Path, generation_id: &str) -> PathBuf {
    root.join(format!(".tmp-{generation_id}"))
}

pub(super) fn generation(root: &Path, generation_id: &str) -> PathBuf {
    root.join(format!("generation-{generation_id}"))
}

pub(super) fn write_metadata(
    directory: &Path,
    generation_id: &str,
    watermark: &IndexWatermark,
    records: &[RecordMetadata],
) -> Result<(), IndexError> {
    let records_path = directory.join(RECORDS_FILE);
    write_synced(
        &records_path,
        &serde_json::to_vec(records).map_err(json_error)?,
    )?;
    let manifest = SidecarManifest {
        generation_id: generation_id.to_string(),
        watermark: watermark.clone(),
        graph_sha256: file_sha256(&directory.join(graph_file()))?,
        data_sha256: file_sha256(&directory.join(data_file()))?,
        records_sha256: file_sha256(&records_path)?,
    };
    write_synced(
        &directory.join(MANIFEST_FILE),
        &serde_json::to_vec(&manifest).map_err(json_error)?,
    )
}

pub(super) fn validate_generation(
    directory: &Path,
    expected: &IndexWatermark,
) -> Result<Vec<RecordMetadata>, IndexError> {
    let manifest: SidecarManifest = read_json(&directory.join(MANIFEST_FILE))?;
    validate_generation_id(&manifest.generation_id)?;
    if manifest.watermark.format_version != expected.format_version
        || manifest.watermark.workspace_id != expected.workspace_id
        || manifest.watermark.provider_profile_id != expected.provider_profile_id
        || manifest.watermark.model_id != expected.model_id
        || manifest.watermark.dimension != expected.dimension
    {
        return Err(IndexError::new(
            "index_identity_mismatch",
            "sidecar identity does not match the requested index",
        ));
    }
    if manifest.watermark.chunk_count != expected.chunk_count
        || manifest.watermark.sqlite_watermark != expected.sqlite_watermark
    {
        return Err(IndexError::new(
            "index_watermark_mismatch",
            "sidecar watermark does not match SQLite",
        ));
    }
    for (path, expected_hash) in [
        (directory.join(graph_file()), manifest.graph_sha256),
        (directory.join(data_file()), manifest.data_sha256),
        (directory.join(RECORDS_FILE), manifest.records_sha256),
    ] {
        if file_sha256(&path)? != expected_hash {
            return Err(IndexError::new(
                "index_checksum_mismatch",
                "sidecar checksum does not match its manifest",
            ));
        }
    }
    let records: Vec<RecordMetadata> = read_json(&directory.join(RECORDS_FILE))?;
    if records.len() != expected.chunk_count as usize {
        return Err(IndexError::new(
            "index_watermark_mismatch",
            "sidecar record count does not match its watermark",
        ));
    }
    let mut identities = HashSet::new();
    for record in &records {
        if !identities.insert((record.version_id, record.chunk_id.clone())) {
            return Err(IndexError::new(
                "index_records_invalid",
                "sidecar contains duplicate chunk identities",
            ));
        }
    }
    Ok(records)
}

pub(super) fn read_current(root: &Path) -> Result<(String, PathBuf), IndexError> {
    let generation_id = fs::read_to_string(root.join("CURRENT"))
        .map_err(|error| IndexError::new("index_unavailable", error.to_string()))?;
    let generation_id = generation_id.trim();
    validate_generation_id(generation_id)?;
    Ok((generation_id.to_string(), generation(root, generation_id)))
}

pub(super) fn activate(root: &Path, generation_id: &str) -> Result<(), IndexError> {
    validate_generation_id(generation_id)?;
    let temporary = root.join(format!(".current-{generation_id}"));
    write_synced(&temporary, generation_id.as_bytes())?;
    fs::rename(&temporary, root.join("CURRENT")).map_err(io_error)
}

pub(super) fn validate_record_ids(
    version_id: DocumentVersionId,
    chunk_id: &ChunkId,
) -> Result<(), IndexError> {
    DocumentVersionId::from_str(&version_id.to_string())
        .map_err(|error| IndexError::new("index_records_invalid", error.to_string()))?;
    ChunkId::new(chunk_id.as_str())
        .map_err(|error| IndexError::new("index_records_invalid", error))?;
    Ok(())
}

fn validate_generation_id(value: &str) -> Result<(), IndexError> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| IndexError::new("index_manifest_invalid", "generation ID is invalid"))
}

fn graph_file() -> String {
    format!("{HNSW_BASENAME}.hnsw.graph")
}

fn data_file() -> String {
    format!("{HNSW_BASENAME}.hnsw.data")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, IndexError> {
    serde_json::from_slice(&fs::read(path).map_err(io_error)?).map_err(json_error)
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), IndexError> {
    let mut file = File::create(path).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn file_sha256(path: &Path) -> Result<String, IndexError> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn io_error(error: std::io::Error) -> IndexError {
    IndexError::new("index_io_failed", error.to_string())
}

fn json_error(error: serde_json::Error) -> IndexError {
    IndexError::new("index_manifest_invalid", error.to_string())
}
