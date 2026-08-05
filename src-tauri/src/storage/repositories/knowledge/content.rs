use super::support::{now, scope, version_identity};
use crate::rag::model::{
    required, sha256, AssetId, ChunkId, DocumentVersionId, NewAsset, NewChunk, NewChunkEmbedding,
    SourceLocation, VectorWatermark,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeSet;

pub fn add_asset(
    connection: &Connection,
    workspace_id: &str,
    input: NewAsset,
) -> Result<AssetId, String> {
    scope(workspace_id)?;
    version_identity(connection, workspace_id, input.version_id)?;
    for (name, value) in [
        ("asset kind", input.kind.as_str()),
        ("storage key", input.storage_key.as_str()),
        ("media type", input.media_type.as_str()),
    ] {
        required(name, value)?;
    }
    sha256("asset sha256", &input.sha256)?;
    if let Some(location) = &input.source_location {
        location.validate()?;
    }
    let id = AssetId::new();
    connection
        .execute(
            "INSERT INTO knowledge_assets
             (id, workspace_id, version_id, kind, storage_key, sha256, media_type,
              source_location_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id.to_string(),
                workspace_id,
                input.version_id.to_string(),
                input.kind,
                input.storage_key,
                input.sha256,
                input.media_type,
                input
                    .source_location
                    .map(|value| serde_json::to_string(&value))
                    .transpose()
                    .map_err(|error| error.to_string())?,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(id)
}

pub fn add_chunk(
    connection: &Connection,
    workspace_id: &str,
    input: NewChunk,
) -> Result<(), String> {
    scope(workspace_id)?;
    let (_, _, _, policy, _) = version_identity(connection, workspace_id, input.version_id)?;
    if input.policy_version != policy {
        return Err("chunk policy does not match document version".to_string());
    }
    if input.text.trim().is_empty() {
        return Err("chunk text is required".to_string());
    }
    sha256("chunk content_sha256", &input.content_sha256)?;
    input.source_location.validate()?;
    connection
        .execute(
            "INSERT INTO knowledge_chunks
             (id, workspace_id, version_id, ordinal, text, source_location_json,
              content_sha256, policy_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                input.id.to_string(),
                workspace_id,
                input.version_id.to_string(),
                input.ordinal,
                input.text,
                serde_json::to_string(&input.source_location).map_err(|error| error.to_string())?,
                input.content_sha256,
                input.policy_version,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn record_chunk_embedding(
    connection: &mut Connection,
    workspace_id: &str,
    input: NewChunkEmbedding,
) -> Result<(), String> {
    scope(workspace_id)?;
    let (profile, model, dimension, policy, _) =
        version_identity(connection, workspace_id, input.version_id)?;
    if (
        input.provider_profile_id.as_str(),
        input.model_id.as_str(),
        input.dimension,
        input.policy_version.as_str(),
    ) != (profile.as_str(), model.as_str(), dimension, policy.as_str())
    {
        return Err("embedding identity does not match document version".to_string());
    }
    sha256("normalized_text_sha256", &input.normalized_text_sha256)?;
    required("vector key", &input.vector_key)?;
    connection
        .execute(
            "INSERT INTO knowledge_chunk_embeddings
             (workspace_id, version_id, chunk_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_key, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                workspace_id,
                input.version_id.to_string(),
                input.chunk_id.to_string(),
                input.provider_profile_id,
                input.model_id,
                input.dimension,
                input.normalized_text_sha256,
                input.policy_version,
                input.vector_key,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn index_chunk_fts(
    connection: &mut Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
    chunk_id: &ChunkId,
) -> Result<(), String> {
    scope(workspace_id)?;
    let (text, source_location, knowledge_base_id, document_id, source_name) = connection
        .query_row(
            "SELECT chunks.text, chunks.source_location_json, documents.knowledge_base_id,
                    documents.id, documents.display_name
             FROM knowledge_chunks AS chunks
             JOIN knowledge_document_versions AS versions
               ON versions.workspace_id = chunks.workspace_id AND versions.id = chunks.version_id
             JOIN knowledge_source_documents AS documents
               ON documents.workspace_id = versions.workspace_id
              AND documents.id = versions.document_id
             WHERE chunks.workspace_id = ?1 AND chunks.version_id = ?2 AND chunks.id = ?3",
            params![workspace_id, version_id.to_string(), chunk_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "knowledge chunk not found".to_string())?;
    let title_path = match serde_json::from_str::<SourceLocation>(&source_location)
        .map_err(|error| error.to_string())?
    {
        SourceLocation::Heading { path } => path.join(" > "),
        _ => String::new(),
    };
    let grade_aliases = grade_aliases(&text);
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM knowledge_chunks_fts
             WHERE workspace_id = ?1 AND version_id = ?2 AND chunk_id = ?3",
            params![workspace_id, version_id.to_string(), chunk_id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO knowledge_chunks_fts
             (workspace_id, knowledge_base_id, document_id, version_id, chunk_id,
              title_path, source_name, grade_aliases, text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                workspace_id,
                knowledge_base_id,
                document_id,
                version_id.to_string(),
                chunk_id.to_string(),
                title_path,
                source_name,
                grade_aliases,
                text
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

pub(super) fn grade_aliases(text: &str) -> String {
    let mut aliases = BTreeSet::new();
    for token in text
        .split(|character: char| !(character.is_alphanumeric() || matches!(character, '-' | '_')))
    {
        let has_letter = token
            .chars()
            .any(|character| character.is_ascii_alphabetic());
        let has_digit = token.chars().any(|character| character.is_ascii_digit());
        if !has_letter || !has_digit {
            continue;
        }
        let normalized = token
            .chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>();
        if !normalized.is_empty() {
            aliases.insert(normalized);
        }
    }
    aliases.into_iter().collect::<Vec<_>>().join(" ")
}

pub fn set_vector_watermark(
    connection: &mut Connection,
    workspace_id: &str,
    watermark: VectorWatermark,
) -> Result<(), String> {
    scope(workspace_id)?;
    let (profile, model, dimension, _, expected) =
        version_identity(connection, workspace_id, watermark.version_id)?;
    if (
        watermark.provider_profile_id.as_str(),
        watermark.model_id.as_str(),
        watermark.dimension,
        watermark.expected_count,
    ) != (profile.as_str(), model.as_str(), dimension, expected)
        || watermark.indexed_count > watermark.expected_count
    {
        return Err("vector watermark does not match document version".to_string());
    }
    required("index version", &watermark.index_version)?;
    connection
        .execute(
            "INSERT INTO knowledge_vector_watermarks
             (workspace_id, version_id, provider_profile_id, model_id, dimension,
              expected_count, indexed_count, index_version, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(version_id) DO UPDATE SET indexed_count = excluded.indexed_count,
               index_version = excluded.index_version, updated_at = excluded.updated_at",
            params![
                workspace_id,
                watermark.version_id.to_string(),
                watermark.provider_profile_id,
                watermark.model_id,
                watermark.dimension,
                watermark.expected_count,
                watermark.indexed_count,
                watermark.index_version,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
