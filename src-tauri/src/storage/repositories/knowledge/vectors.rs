use super::support::{now, scope, version_identity};
use crate::rag::model::{
    ChunkEmbeddingSource, ChunkId, DocumentVersionId, EmbeddingIdentity, EmbeddingVectorBatch,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

pub fn list_chunks_for_embedding(
    connection: &Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
) -> Result<Vec<ChunkEmbeddingSource>, String> {
    scope(workspace_id)?;
    version_identity(connection, workspace_id, version_id)?;
    let mut statement = connection
        .prepare(
            "SELECT id, ordinal, text FROM knowledge_chunks
             WHERE workspace_id = ?1 AND version_id = ?2 ORDER BY ordinal, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id, version_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let (id, ordinal, text) = row.map_err(|error| error.to_string())?;
        Ok(ChunkEmbeddingSource {
            id: id.parse()?,
            ordinal,
            text,
        })
    })
    .collect()
}

pub fn linked_embedding_chunk_ids(
    connection: &Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
    identity: &EmbeddingIdentity,
) -> Result<Vec<ChunkId>, String> {
    scope(workspace_id)?;
    let mut statement = connection
        .prepare(
            "SELECT chunk_id FROM knowledge_chunk_embeddings
             WHERE workspace_id = ?1 AND version_id = ?2 AND provider_profile_id = ?3
               AND model_id = ?4 AND dimension = ?5 AND policy_version = ?6
             ORDER BY chunk_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![
                workspace_id,
                version_id.to_string(),
                identity.provider_profile_id,
                identity.model_id,
                identity.dimension,
                identity.policy_version
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string())?.parse())
        .collect()
}

pub fn find_reusable_vector(
    connection: &Connection,
    workspace_id: &str,
    identity: &EmbeddingIdentity,
) -> Result<Option<String>, String> {
    scope(workspace_id)?;
    connection
        .query_row(
            "SELECT id FROM knowledge_vectors
             WHERE workspace_id = ?1 AND provider_profile_id = ?2 AND model_id = ?3
               AND dimension = ?4 AND normalized_text_sha256 = ?5 AND policy_version = ?6",
            params![
                workspace_id,
                identity.provider_profile_id,
                identity.model_id,
                identity.dimension,
                identity.normalized_text_sha256,
                identity.policy_version
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub fn persist_embedding_batch(
    connection: &mut Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
    vectors: &[EmbeddingVectorBatch],
) -> Result<(), String> {
    scope(workspace_id)?;
    let expected = version_identity(connection, workspace_id, version_id)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    for vector in vectors {
        validate_identity(&vector.identity, &expected)?;
        if vector.vector_blob.len() != vector.identity.dimension as usize * 4
            || vector.chunk_ids.is_empty()
        {
            return Err("invalid embedding vector batch".to_string());
        }
        transaction
            .execute(
                "INSERT INTO knowledge_vectors
                 (id, workspace_id, provider_profile_id, model_id, dimension,
                  normalized_text_sha256, policy_version, vector_blob, vector_sha256, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(workspace_id, provider_profile_id, model_id, dimension,
                             normalized_text_sha256, policy_version) DO NOTHING",
                params![
                    vector.vector_key,
                    workspace_id,
                    vector.identity.provider_profile_id,
                    vector.identity.model_id,
                    vector.identity.dimension,
                    vector.identity.normalized_text_sha256,
                    vector.identity.policy_version,
                    vector.vector_blob,
                    vector.vector_sha256,
                    now()
                ],
            )
            .map_err(|error| error.to_string())?;
        let key = exact_vector_key(&transaction, workspace_id, &vector.identity)?;
        link_chunks(
            &transaction,
            workspace_id,
            version_id,
            &vector.identity,
            &key,
            &vector.chunk_ids,
        )?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub fn persist_reused_embedding_links(
    connection: &mut Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
    identity: &EmbeddingIdentity,
    vector_key: &str,
    chunk_ids: &[ChunkId],
) -> Result<(), String> {
    scope(workspace_id)?;
    let expected = version_identity(connection, workspace_id, version_id)?;
    validate_identity(identity, &expected)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let exact = exact_vector_key(&transaction, workspace_id, identity)?;
    if exact != vector_key {
        return Err("reusable vector identity mismatch".to_string());
    }
    link_chunks(
        &transaction,
        workspace_id,
        version_id,
        identity,
        vector_key,
        chunk_ids,
    )?;
    transaction.commit().map_err(|error| error.to_string())
}

fn link_chunks(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    version_id: DocumentVersionId,
    identity: &EmbeddingIdentity,
    vector_key: &str,
    chunk_ids: &[ChunkId],
) -> Result<(), String> {
    for chunk_id in chunk_ids {
        let exists = transaction
            .query_row(
                "SELECT 1 FROM knowledge_chunks
                 WHERE workspace_id = ?1 AND version_id = ?2 AND id = ?3",
                params![workspace_id, version_id.to_string(), chunk_id.to_string()],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !exists {
            return Err("knowledge chunk not found".to_string());
        }
        transaction
            .execute(
                "INSERT INTO knowledge_chunk_embeddings
                 (workspace_id, version_id, chunk_id, provider_profile_id, model_id, dimension,
                  normalized_text_sha256, policy_version, vector_key, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(version_id, chunk_id) DO NOTHING",
                params![
                    workspace_id,
                    version_id.to_string(),
                    chunk_id.to_string(),
                    identity.provider_profile_id,
                    identity.model_id,
                    identity.dimension,
                    identity.normalized_text_sha256,
                    identity.policy_version,
                    vector_key,
                    now()
                ],
            )
            .map_err(|error| error.to_string())?;
        let linked: (String, String, u32, String, String) = transaction
            .query_row(
                "SELECT provider_profile_id, model_id, dimension, policy_version, vector_key
                 FROM knowledge_chunk_embeddings WHERE version_id = ?1 AND chunk_id = ?2",
                params![version_id.to_string(), chunk_id.to_string()],
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
            .map_err(|error| error.to_string())?;
        if linked
            != (
                identity.provider_profile_id.clone(),
                identity.model_id.clone(),
                identity.dimension,
                identity.policy_version.clone(),
                vector_key.to_string(),
            )
        {
            return Err("existing chunk embedding identity mismatch".to_string());
        }
    }
    Ok(())
}

fn exact_vector_key(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    identity: &EmbeddingIdentity,
) -> Result<String, String> {
    transaction
        .query_row(
            "SELECT id FROM knowledge_vectors
             WHERE workspace_id = ?1 AND provider_profile_id = ?2 AND model_id = ?3
               AND dimension = ?4 AND normalized_text_sha256 = ?5 AND policy_version = ?6",
            params![
                workspace_id,
                identity.provider_profile_id,
                identity.model_id,
                identity.dimension,
                identity.normalized_text_sha256,
                identity.policy_version
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn validate_identity(
    identity: &EmbeddingIdentity,
    expected: &(String, String, u32, String, u32),
) -> Result<(), String> {
    if (
        identity.provider_profile_id.as_str(),
        identity.model_id.as_str(),
        identity.dimension,
        identity.policy_version.as_str(),
    ) != (
        expected.0.as_str(),
        expected.1.as_str(),
        expected.2,
        expected.3.as_str(),
    ) {
        Err("embedding identity does not match document version".to_string())
    } else {
        Ok(())
    }
}
