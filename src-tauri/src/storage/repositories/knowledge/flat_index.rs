use super::content::grade_aliases;
use super::support::{now, scope, version_identity};
use crate::rag::model::{ChunkId, DocumentVersionId, SourceLocation};
use rusqlite::{params, Connection};

pub fn finalize_flat_index(
    connection: &mut Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
) -> Result<Vec<(ChunkId, String)>, String> {
    scope(workspace_id)?;
    let (profile, model, dimension, policy, expected) =
        version_identity(connection, workspace_id, version_id)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let embedded: u32 = transaction
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk_embeddings e
             JOIN knowledge_vectors v ON v.id = e.vector_key AND v.workspace_id = e.workspace_id
             WHERE e.workspace_id = ?1 AND e.version_id = ?2
               AND e.provider_profile_id = ?3 AND e.model_id = ?4
               AND e.dimension = ?5 AND e.policy_version = ?6",
            params![
                workspace_id,
                version_id.to_string(),
                profile,
                model,
                dimension,
                policy
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if embedded != expected {
        return Err("cannot index an incomplete embedding set".to_string());
    }
    transaction
        .execute(
            "DELETE FROM knowledge_chunks_fts WHERE workspace_id = ?1 AND version_id = ?2",
            params![workspace_id, version_id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    let fts_rows = {
        let mut statement = transaction
            .prepare(
                "SELECT chunks.id, chunks.text, chunks.source_location_json,
                        documents.knowledge_base_id, documents.id, documents.display_name
                 FROM knowledge_chunks AS chunks
                 JOIN knowledge_document_versions AS versions
                   ON versions.workspace_id = chunks.workspace_id
                  AND versions.id = chunks.version_id
                 JOIN knowledge_source_documents AS documents
                   ON documents.workspace_id = versions.workspace_id
                  AND documents.id = versions.document_id
                 WHERE chunks.workspace_id = ?1 AND chunks.version_id = ?2
                 ORDER BY chunks.ordinal, chunks.id",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![workspace_id, version_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    for (chunk_id, text, location, base_id, document_id, source_name) in fts_rows {
        let title_path = match serde_json::from_str::<SourceLocation>(&location)
            .map_err(|error| error.to_string())?
        {
            SourceLocation::Heading { path } => path.join(" > "),
            _ => String::new(),
        };
        transaction
            .execute(
                "INSERT INTO knowledge_chunks_fts
                 (workspace_id, knowledge_base_id, document_id, version_id, chunk_id,
                  title_path, source_name, grade_aliases, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    workspace_id,
                    base_id,
                    document_id,
                    version_id.to_string(),
                    chunk_id,
                    title_path,
                    source_name,
                    grade_aliases(&text),
                    text
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction
        .execute(
            "INSERT INTO knowledge_vector_watermarks
             (workspace_id, version_id, provider_profile_id, model_id, dimension,
              expected_count, indexed_count, index_version, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 'sqlite-flat-v1', ?7)
             ON CONFLICT(version_id) DO UPDATE SET indexed_count = excluded.indexed_count,
               index_version = excluded.index_version, updated_at = excluded.updated_at",
            params![
                workspace_id,
                version_id.to_string(),
                profile,
                model,
                dimension,
                expected,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    let manifest = {
        let mut statement = transaction
            .prepare(
                "SELECT c.id, e.vector_key FROM knowledge_chunks c
                 JOIN knowledge_chunk_embeddings e
                   ON e.version_id = c.version_id AND e.chunk_id = c.id
                 WHERE c.workspace_id = ?1 AND c.version_id = ?2 ORDER BY c.ordinal, c.id",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![workspace_id, version_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let (id, key) = row.map_err(|error| error.to_string())?;
            Ok((id.parse()?, key))
        })
        .collect::<Result<Vec<_>, String>>()?
    };
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(manifest)
}
