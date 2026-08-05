use super::content::{add_asset, add_chunk};
use super::support::{scope, version_identity};
use crate::rag::model::{DocumentVersionId, NewAsset, NewChunk};
use rusqlite::{params, Connection, OptionalExtension};

pub fn persist_parsed_content(
    connection: &mut Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
    assets: &[NewAsset],
    chunks: &[NewChunk],
) -> Result<(), String> {
    scope(workspace_id)?;
    let manifest_sealed: bool = connection
        .query_row(
            "SELECT manifest_sealed FROM knowledge_document_versions
             WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, version_id.to_string()],
            |row| row.get::<_, i64>(0).map(|value| value != 0),
        )
        .map_err(|error| error.to_string())?;
    if !manifest_sealed {
        return Err("document_manifest_pending: parsed counts are not sealed".to_string());
    }
    let (_, _, _, policy, expected_chunks) =
        version_identity(connection, workspace_id, version_id)?;
    let expected_assets: u32 = connection
        .query_row(
            "SELECT expected_asset_count FROM knowledge_document_versions
             WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, version_id.to_string()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if assets.len() != expected_assets as usize || chunks.len() != expected_chunks as usize {
        return Err("parsed content count does not match document version".to_string());
    }
    if chunks
        .iter()
        .any(|chunk| chunk.version_id != version_id || chunk.policy_version != policy)
        || assets.iter().any(|asset| asset.version_id != version_id)
    {
        return Err("parsed content identity does not match document version".to_string());
    }

    let stored = stored_counts(connection, workspace_id, version_id)?;
    if stored == (expected_assets, expected_chunks) {
        return verify_existing(connection, workspace_id, version_id, assets, chunks);
    }
    if stored != (0, 0) {
        return Err("partial parsed content requires repair".to_string());
    }

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    for asset in assets.iter().cloned() {
        add_asset(&transaction, workspace_id, asset)?;
    }
    for chunk in chunks.iter().cloned() {
        add_chunk(&transaction, workspace_id, chunk)?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn stored_counts(
    connection: &Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
) -> Result<(u32, u32), String> {
    let assets = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_assets WHERE workspace_id = ?1 AND version_id = ?2",
            params![workspace_id, version_id.to_string()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let chunks = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunks WHERE workspace_id = ?1 AND version_id = ?2",
            params![workspace_id, version_id.to_string()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok((assets, chunks))
}

fn verify_existing(
    connection: &Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
    assets: &[NewAsset],
    chunks: &[NewChunk],
) -> Result<(), String> {
    for asset in assets {
        let location = asset
            .source_location
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| error.to_string())?;
        let found = connection
            .query_row(
                "SELECT 1 FROM knowledge_assets
                 WHERE workspace_id = ?1 AND version_id = ?2 AND kind = ?3
                   AND storage_key = ?4 AND sha256 = ?5 AND media_type = ?6
                   AND source_location_json IS ?7",
                params![
                    workspace_id,
                    version_id.to_string(),
                    asset.kind,
                    asset.storage_key,
                    asset.sha256,
                    asset.media_type,
                    location
                ],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !found {
            return Err("stored asset does not match parsed manifest".to_string());
        }
    }
    for chunk in chunks {
        let location =
            serde_json::to_string(&chunk.source_location).map_err(|error| error.to_string())?;
        let found = connection
            .query_row(
                "SELECT 1 FROM knowledge_chunks
                 WHERE workspace_id = ?1 AND version_id = ?2 AND id = ?3 AND ordinal = ?4
                   AND text = ?5 AND source_location_json = ?6 AND content_sha256 = ?7
                   AND policy_version = ?8",
                params![
                    workspace_id,
                    version_id.to_string(),
                    chunk.id.to_string(),
                    chunk.ordinal,
                    chunk.text,
                    location,
                    chunk.content_sha256,
                    chunk.policy_version
                ],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !found {
            return Err("stored chunk does not match parsed manifest".to_string());
        }
    }
    Ok(())
}
