use super::catalog::{
    list_document_versions, list_source_documents, KnowledgeBaseMergeMode,
    KnowledgeBaseMergeRequest,
};
use super::records::{KnowledgeBaseRecord, SourceDocumentRecord};
use super::support::scope;
use super::{create_knowledge_base, get_knowledge_base};
use crate::rag::model::{DocumentVersionId, KnowledgeBaseId, SourceDocumentId};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

pub fn merge_knowledge_bases(
    connection: &mut Connection,
    workspace_id: &str,
    request: KnowledgeBaseMergeRequest,
) -> Result<KnowledgeBaseRecord, String> {
    scope(workspace_id)?;
    if request.source_id == request.target_id {
        return Err("source and target knowledge bases must differ".to_string());
    }
    let source = get_knowledge_base(connection, workspace_id, request.source_id)?
        .ok_or_else(|| "source knowledge base not found".to_string())?;
    let target = get_knowledge_base(connection, workspace_id, request.target_id)?
        .ok_or_else(|| "target knowledge base not found".to_string())?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let destination = match request.mode {
        KnowledgeBaseMergeMode::Existing => target,
        KnowledgeBaseMergeMode::New => {
            let name = request
                .destination_name
                .as_deref()
                .ok_or_else(|| "destination knowledge base name is required".to_string())?;
            create_knowledge_base(&transaction, workspace_id, name)?
        }
    };
    let bases = match request.mode {
        KnowledgeBaseMergeMode::Existing => vec![source.id],
        KnowledgeBaseMergeMode::New => vec![source.id, request.target_id],
    };
    for base_id in bases {
        let documents = list_source_documents(&transaction, workspace_id, base_id)?;
        for document in documents {
            copy_document(&transaction, workspace_id, &document, destination.id)?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    get_knowledge_base(connection, workspace_id, destination.id)?
        .ok_or_else(|| "merged knowledge base not found".to_string())
}

fn copy_document(
    connection: &Connection,
    workspace_id: &str,
    source: &SourceDocumentRecord,
    destination_id: KnowledgeBaseId,
) -> Result<(), String> {
    let new_document_id = SourceDocumentId::new();
    connection
        .execute(
            "INSERT INTO knowledge_source_documents
             (id, workspace_id, knowledge_base_id, display_name, source_kind,
              active_version_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7)",
            params![
                new_document_id.to_string(),
                workspace_id,
                destination_id.to_string(),
                source.display_name,
                source.source_kind,
                source.created_at,
                source.updated_at
            ],
        )
        .map_err(|error| error.to_string())?;

    let versions = list_document_versions(connection, workspace_id, source.id)?;
    let mut active_version_id = None;
    for version in versions {
        let new_version_id = DocumentVersionId::new();
        connection
            .execute(
                "INSERT INTO knowledge_document_versions
                 (id, workspace_id, document_id, content_sha256, mime_type, parser,
                  parser_version, chunk_policy_version, embedding_profile_id,
                  embedding_model_id, embedding_dimension, expected_asset_count,
                  expected_chunk_count, manifest_sealed, created_at, activated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                         ?14, ?15, ?16)",
                params![
                    new_version_id.to_string(),
                    workspace_id,
                    new_document_id.to_string(),
                    version.content_sha256,
                    version.mime_type,
                    version.parser,
                    version.parser_version,
                    version.chunk_policy_version,
                    version.embedding_profile_id,
                    version.embedding_model_id,
                    version.embedding_dimension,
                    version.expected_asset_count,
                    version.expected_chunk_count,
                    i64::from(version.manifest_sealed),
                    version.created_at,
                    version.activated_at
                ],
            )
            .map_err(|error| error.to_string())?;
        copy_version_content(
            connection,
            workspace_id,
            source.id,
            version.id,
            new_document_id,
            new_version_id,
            destination_id,
        )?;
        if source.active_version_id == Some(version.id) {
            active_version_id = Some(new_version_id);
        }
    }
    connection
        .execute(
            "UPDATE knowledge_source_documents SET active_version_id = ?1
             WHERE workspace_id = ?2 AND id = ?3",
            params![
                active_version_id.map(|id| id.to_string()),
                workspace_id,
                new_document_id.to_string()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn copy_version_content(
    connection: &Connection,
    workspace_id: &str,
    source_document_id: SourceDocumentId,
    source_version_id: DocumentVersionId,
    destination_document_id: SourceDocumentId,
    destination_version_id: DocumentVersionId,
    destination_base_id: KnowledgeBaseId,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, storage_key, sha256, media_type, source_location_json, created_at
             FROM knowledge_assets
             WHERE workspace_id = ?1 AND version_id = ?2 ORDER BY id",
        )
        .map_err(|error| error.to_string())?;
    let assets = statement
        .query_map(
            params![workspace_id, source_version_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for (kind, storage_key, sha256, media_type, location, created_at) in assets {
        connection
            .execute(
                "INSERT INTO knowledge_assets
                 (id, workspace_id, version_id, kind, storage_key, sha256, media_type,
                  source_location_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    workspace_id,
                    destination_version_id.to_string(),
                    kind,
                    storage_key,
                    sha256,
                    media_type,
                    location,
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    let mut statement = connection
        .prepare(
            "SELECT id, ordinal, text, source_location_json, content_sha256, policy_version, created_at
             FROM knowledge_chunks
             WHERE workspace_id = ?1 AND version_id = ?2 ORDER BY ordinal, id",
        )
        .map_err(|error| error.to_string())?;
    let chunks = statement
        .query_map(
            params![workspace_id, source_version_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for (chunk_id, ordinal, text, location, content_sha256, policy_version, created_at) in chunks {
        connection
            .execute(
                "INSERT INTO knowledge_chunks
                 (id, workspace_id, version_id, ordinal, text, source_location_json,
                  content_sha256, policy_version, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    chunk_id,
                    workspace_id,
                    destination_version_id.to_string(),
                    ordinal,
                    text,
                    location,
                    content_sha256,
                    policy_version,
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    let mut statement = connection
        .prepare(
            "SELECT chunk_id, provider_profile_id, model_id, dimension,
                    normalized_text_sha256, policy_version, vector_key, created_at
             FROM knowledge_chunk_embeddings
             WHERE workspace_id = ?1 AND version_id = ?2 ORDER BY chunk_id",
        )
        .map_err(|error| error.to_string())?;
    let embeddings = statement
        .query_map(
            params![workspace_id, source_version_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for (
        chunk_id,
        provider_profile_id,
        model_id,
        dimension,
        normalized_text_sha256,
        policy_version,
        vector_key,
        created_at,
    ) in embeddings
    {
        connection
            .execute(
                "INSERT INTO knowledge_chunk_embeddings
                 (workspace_id, version_id, chunk_id, provider_profile_id, model_id,
                  dimension, normalized_text_sha256, policy_version, vector_key, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    workspace_id,
                    destination_version_id.to_string(),
                    chunk_id,
                    provider_profile_id,
                    model_id,
                    dimension,
                    normalized_text_sha256,
                    policy_version,
                    vector_key,
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    connection
        .execute(
            "INSERT INTO knowledge_chunks_fts
             (workspace_id, knowledge_base_id, document_id, version_id, chunk_id,
              title_path, source_name, grade_aliases, text)
             SELECT ?1, ?2, ?3, ?4, chunk_id, title_path, source_name, grade_aliases, text
             FROM knowledge_chunks_fts
             WHERE workspace_id = ?1 AND document_id = ?5 AND version_id = ?6",
            params![
                workspace_id,
                destination_base_id.to_string(),
                destination_document_id.to_string(),
                destination_version_id.to_string(),
                source_document_id.to_string(),
                source_version_id.to_string()
            ],
        )
        .map_err(|error| error.to_string())?;

    let watermark = connection
        .query_row(
            "SELECT provider_profile_id, model_id, dimension, expected_count,
                    indexed_count, index_version, updated_at
             FROM knowledge_vector_watermarks
             WHERE workspace_id = ?1 AND version_id = ?2",
            params![workspace_id, source_version_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((
        provider_profile_id,
        model_id,
        dimension,
        expected_count,
        indexed_count,
        index_version,
        updated_at,
    )) = watermark
    {
        connection
            .execute(
                "INSERT INTO knowledge_vector_watermarks
                 (workspace_id, version_id, provider_profile_id, model_id, dimension,
                  expected_count, indexed_count, index_version, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    workspace_id,
                    destination_version_id.to_string(),
                    provider_profile_id,
                    model_id,
                    dimension,
                    expected_count,
                    indexed_count,
                    index_version,
                    updated_at
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
