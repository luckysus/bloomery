use crate::agent::runtime::TurnSnapshot;
use crate::storage::StorageError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable, non-secret metadata needed to rebuild a desktop Agent Turn.
/// Provider credentials are intentionally excluded and are loaded from the
/// configured secret store when recovery starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTurnSnapshot {
    pub turn: TurnSnapshot,
    pub assistant_message_id: Uuid,
    pub provider_base_url: String,
    pub smart_search_enabled: bool,
    pub evidence_pack_id: Option<Uuid>,
}

pub fn save(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
    snapshot: &AgentTurnSnapshot,
    timestamp: DateTime<Utc>,
) -> Result<(), StorageError> {
    if workspace_id.trim().is_empty() || workspace_id.trim() != workspace_id {
        return Err(StorageError::new(
            "agent_workspace_invalid",
            "workspace ID is invalid",
        ));
    }
    if snapshot.turn.turn_id != run_id {
        return Err(StorageError::new(
            "agent_turn_snapshot_mismatch",
            "turn snapshot does not belong to the requested run",
        ));
    }
    if snapshot.provider_base_url.contains('@') {
        return Err(StorageError::new(
            "agent_turn_snapshot_credentials",
            "provider base URL must not contain credentials",
        ));
    }
    let snapshot_json = serde_json::to_string(snapshot).map_err(|error| {
        StorageError::new("agent_turn_snapshot_encode_failed", error.to_string())
    })?;
    if snapshot_json.len() > 64 * 1024 {
        return Err(StorageError::new(
            "agent_turn_snapshot_too_large",
            "agent turn snapshot exceeds 64 KiB",
        ));
    }
    connection
        .execute(
            "INSERT INTO agent_run_turn_snapshots
             (run_id, workspace_id, snapshot_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(run_id) DO UPDATE SET
               workspace_id = excluded.workspace_id,
               snapshot_json = excluded.snapshot_json,
               updated_at = excluded.updated_at",
            params![
                run_id.to_string(),
                workspace_id,
                snapshot_json,
                timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            ],
        )
        .map_err(|error| {
            StorageError::new("agent_turn_snapshot_storage_failed", error.to_string())
        })?;
    Ok(())
}

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Option<AgentTurnSnapshot>, StorageError> {
    connection
        .query_row(
            "SELECT snapshot_json FROM agent_run_turn_snapshots
             WHERE workspace_id = ?1 AND run_id = ?2",
            params![workspace_id, run_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            StorageError::new("agent_turn_snapshot_storage_failed", error.to_string())
        })?
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                StorageError::new("agent_turn_snapshot_decode_failed", error.to_string())
            })
        })
        .transpose()
}
