use crate::rag::model::DocumentVersionId;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::str::FromStr;

pub(super) fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(super) fn scope(workspace_id: &str) -> Result<(), String> {
    if workspace_id.is_empty()
        || workspace_id.len() > 128
        || !workspace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Err("invalid workspace ID".to_string())
    } else {
        Ok(())
    }
}

pub(super) fn parse<T: FromStr>(value: String, name: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| format!("invalid stored {name}: {error}"))
}

pub(super) fn ensure_owner(
    connection: &Connection,
    table: &str,
    workspace_id: &str,
    id: &str,
) -> Result<(), String> {
    let sql = format!("SELECT 1 FROM {table} WHERE workspace_id = ?1 AND id = ?2");
    if connection
        .query_row(&sql, params![workspace_id, id], |_| Ok(()))
        .optional()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        Ok(())
    } else {
        Err("knowledge record not found".to_string())
    }
}

pub(super) fn version_identity(
    connection: &Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
) -> Result<(String, String, u32, String, u32), String> {
    connection
        .query_row(
            "SELECT embedding_profile_id, embedding_model_id, embedding_dimension,
                    chunk_policy_version, expected_chunk_count
             FROM knowledge_document_versions WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, version_id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "knowledge record not found".to_string())
}
