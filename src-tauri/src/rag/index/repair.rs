use super::lifecycle::open_hnsw;
use super::rebuild::{index_root, load_index_snapshot, IndexRebuildRequest, INDEX_REBUILD_KIND};
use super::vector::FlatVectorIndex;
use crate::tasks::{repository, TaskState};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

const REBUILD_OVERHEAD_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexRepairState {
    Healthy,
    DegradedFlat,
    RebuildRequired,
    Rebuilding,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexRepairReason {
    MissingSidecar,
    CorruptSidecar,
    WatermarkDiverged,
    ModelChanged,
    InterruptedBuild,
    LowDisk,
    RebuildFailed,
    SqliteInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexServingMode {
    Hnsw,
    Flat,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexHealthReport {
    pub state: IndexRepairState,
    pub reason: Option<IndexRepairReason>,
    pub serving_mode: IndexServingMode,
    pub chunk_count: u32,
    pub required_rebuild_bytes: u64,
    pub available_disk_bytes: Option<u64>,
    pub stale_temporary_count: u32,
    pub rebuild_task_id: Option<Uuid>,
}

pub fn inspect_index_health(
    connection: &Connection,
    workspace_id: &str,
    content_root: &Path,
    request: &IndexRebuildRequest,
    available_disk_bytes: Option<u64>,
) -> Result<IndexHealthReport, String> {
    let snapshot = match load_index_snapshot(connection, workspace_id, request) {
        Ok(snapshot) => snapshot,
        Err(_) => {
            return Ok(report(
                IndexRepairState::Failed,
                Some(IndexRepairReason::SqliteInvalid),
                IndexServingMode::Unavailable,
                0,
                request.dimension,
                available_disk_bytes,
                0,
                None,
            ))
        }
    };
    inspect_snapshot(
        connection,
        content_root,
        request,
        snapshot,
        available_disk_bytes,
    )
}

fn inspect_snapshot(
    connection: &Connection,
    content_root: &Path,
    request: &IndexRebuildRequest,
    snapshot: super::rebuild::IndexSnapshot,
    available_disk_bytes: Option<u64>,
) -> Result<IndexHealthReport, String> {
    let root = index_root(content_root, &snapshot.watermark);
    let stale_temporary_count = stale_temporary_count(&root)?;
    let hnsw = open_hnsw(&root, &snapshot.watermark);
    let hnsw_valid = hnsw.is_ok();
    let flat_valid = FlatVectorIndex::load(connection, &snapshot.watermark).is_ok();
    let serving_mode = if hnsw_valid {
        IndexServingMode::Hnsw
    } else if flat_valid {
        IndexServingMode::Flat
    } else {
        IndexServingMode::Unavailable
    };
    let tasks = matching_tasks(connection, &snapshot.watermark.workspace_id, request)?;
    if let Some(id) = tasks.active {
        return Ok(report(
            IndexRepairState::Rebuilding,
            None,
            serving_mode,
            snapshot.watermark.chunk_count,
            snapshot.watermark.dimension,
            available_disk_bytes,
            stale_temporary_count,
            Some(id),
        ));
    }
    if hnsw_valid {
        return Ok(report(
            IndexRepairState::Healthy,
            (stale_temporary_count > 0).then_some(IndexRepairReason::InterruptedBuild),
            serving_mode,
            snapshot.watermark.chunk_count,
            snapshot.watermark.dimension,
            available_disk_bytes,
            stale_temporary_count,
            None,
        ));
    }
    if !flat_valid {
        return Ok(report(
            IndexRepairState::Failed,
            Some(IndexRepairReason::SqliteInvalid),
            serving_mode,
            snapshot.watermark.chunk_count,
            snapshot.watermark.dimension,
            available_disk_bytes,
            stale_temporary_count,
            tasks.failed,
        ));
    }
    let required =
        required_rebuild_bytes(snapshot.watermark.chunk_count, snapshot.watermark.dimension);
    if available_disk_bytes.is_some_and(|available| available < required) {
        return Ok(IndexHealthReport {
            state: IndexRepairState::Failed,
            reason: Some(IndexRepairReason::LowDisk),
            serving_mode,
            chunk_count: snapshot.watermark.chunk_count,
            required_rebuild_bytes: required,
            available_disk_bytes,
            stale_temporary_count,
            rebuild_task_id: tasks.failed,
        });
    }
    if let Some(id) = tasks.failed {
        return Ok(report(
            IndexRepairState::Failed,
            Some(IndexRepairReason::RebuildFailed),
            serving_mode,
            snapshot.watermark.chunk_count,
            snapshot.watermark.dimension,
            available_disk_bytes,
            stale_temporary_count,
            Some(id),
        ));
    }
    let error = hnsw.expect_err("invalid HNSW already checked");
    let reason = match error.code() {
        "index_watermark_mismatch" => IndexRepairReason::WatermarkDiverged,
        "index_identity_mismatch" => IndexRepairReason::ModelChanged,
        "index_unavailable" if has_other_index(&root)? => IndexRepairReason::ModelChanged,
        "index_unavailable" if !root.exists() => IndexRepairReason::MissingSidecar,
        _ => IndexRepairReason::CorruptSidecar,
    };
    let state = if matches!(
        reason,
        IndexRepairReason::WatermarkDiverged | IndexRepairReason::ModelChanged
    ) {
        IndexRepairState::RebuildRequired
    } else {
        IndexRepairState::DegradedFlat
    };
    Ok(report(
        state,
        Some(reason),
        serving_mode,
        snapshot.watermark.chunk_count,
        snapshot.watermark.dimension,
        available_disk_bytes,
        stale_temporary_count,
        None,
    ))
}

pub fn cleanup_interrupted_builds(root: &Path) -> Result<u32, String> {
    if !root.exists() {
        return Ok(0);
    }
    let mut removed = 0_u32;
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".tmp-") && !name.starts_with(".current-") {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(|error| error.to_string())?;
        } else {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
        removed = removed.saturating_add(1);
    }
    Ok(removed)
}

struct MatchingTasks {
    active: Option<Uuid>,
    failed: Option<Uuid>,
}

fn matching_tasks(
    connection: &Connection,
    workspace_id: &str,
    request: &IndexRebuildRequest,
) -> Result<MatchingTasks, String> {
    let mut active = None;
    let mut failed = None;
    for task in repository::list(connection, workspace_id).map_err(|error| error.to_string())? {
        if task.kind != INDEX_REBUILD_KIND
            || serde_json::from_str::<IndexRebuildRequest>(&task.payload_json)
                .ok()
                .as_ref()
                != Some(request)
        {
            continue;
        }
        if matches!(
            task.state,
            TaskState::Queued
                | TaskState::Running
                | TaskState::WaitingExternal
                | TaskState::Paused
                | TaskState::Interrupted
        ) {
            active = Some(task.id);
        } else if task.state == TaskState::Failed {
            failed = Some(task.id);
        }
    }
    Ok(MatchingTasks { active, failed })
}

fn stale_temporary_count(root: &Path) -> Result<u32, String> {
    if !root.exists() {
        return Ok(0);
    }
    let mut count = 0_u32;
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let name = entry
            .map_err(|error| error.to_string())?
            .file_name()
            .to_string_lossy()
            .to_string();
        if name.starts_with(".tmp-") || name.starts_with(".current-") {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

fn has_other_index(root: &Path) -> Result<bool, String> {
    let Some(parent) = root.parent() else {
        return Ok(false);
    };
    if !parent.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(parent).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path != root && path.is_dir() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn required_rebuild_bytes(chunk_count: u32, dimension: u32) -> u64 {
    u64::from(chunk_count)
        .saturating_mul(u64::from(dimension))
        .saturating_mul(4)
        .saturating_mul(3)
        .saturating_add(REBUILD_OVERHEAD_BYTES)
}

fn report(
    state: IndexRepairState,
    reason: Option<IndexRepairReason>,
    serving_mode: IndexServingMode,
    chunk_count: u32,
    dimension: u32,
    available_disk_bytes: Option<u64>,
    stale_temporary_count: u32,
    rebuild_task_id: Option<Uuid>,
) -> IndexHealthReport {
    IndexHealthReport {
        state,
        reason,
        serving_mode,
        chunk_count,
        required_rebuild_bytes: required_rebuild_bytes(chunk_count, dimension),
        available_disk_bytes,
        stale_temporary_count,
        rebuild_task_id,
    }
}
