use super::records::{DocumentVersionRecord, KnowledgeBaseRecord, SourceDocumentRecord};
use super::support::{now, parse, scope};
use super::{create_knowledge_base, get_document_version, get_knowledge_base};
use crate::rag::model::{
    required, DocumentVersionId, KnowledgeBaseId, SourceDocumentId,
};
use serde::Serialize;
use serde::Deserialize;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnowledgeBaseDeleteImpact {
    pub knowledge_base_id: KnowledgeBaseId,
    pub name: String,
    pub document_count: u32,
    pub version_count: u32,
    pub chunk_count: u32,
    pub asset_count: u32,
    pub active_task_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnowledgeHealth {
    pub knowledge_base_count: u32,
    pub document_count: u32,
    pub active_document_count: u32,
    pub version_count: u32,
    pub chunk_count: u32,
    pub indexed_chunk_count: u32,
    pub active_task_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeBaseMergeMode {
    New,
    Existing,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct KnowledgeBaseMergeRequest {
    pub source_id: KnowledgeBaseId,
    pub target_id: KnowledgeBaseId,
    pub mode: KnowledgeBaseMergeMode,
    pub destination_name: Option<String>,
}

pub fn rename_knowledge_base(
    connection: &Connection,
    workspace_id: &str,
    id: KnowledgeBaseId,
    name: &str,
) -> Result<KnowledgeBaseRecord, String> {
    scope(workspace_id)?;
    let name = name.trim();
    required("knowledge base name", name)?;
    let changed = connection
        .execute(
            "UPDATE knowledge_bases SET name = ?1, updated_at = ?2
             WHERE workspace_id = ?3 AND id = ?4",
            params![name, now(), workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("knowledge base not found".to_string());
    }
    get_knowledge_base(connection, workspace_id, id)?
        .ok_or_else(|| "knowledge base not found".to_string())
}

pub fn list_source_documents(
    connection: &Connection,
    workspace_id: &str,
    knowledge_base_id: KnowledgeBaseId,
) -> Result<Vec<SourceDocumentRecord>, String> {
    scope(workspace_id)?;
    let mut statement = connection
        .prepare(
            "SELECT id, knowledge_base_id, display_name, source_kind, active_version_id,
                    created_at, updated_at
             FROM knowledge_source_documents
             WHERE workspace_id = ?1 AND knowledge_base_id = ?2
             ORDER BY updated_at DESC, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![workspace_id, knowledge_base_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let (id, base, display_name, source_kind, active, created_at, updated_at) =
            row.map_err(|error| error.to_string())?;
        Ok(SourceDocumentRecord {
            id: parse(id, "source document ID")?,
            knowledge_base_id: parse(base, "knowledge base ID")?,
            display_name,
            source_kind,
            active_version_id: active
                .map(|value| parse(value, "document version ID"))
                .transpose()?,
            created_at,
            updated_at,
        })
    })
    .collect()
}

pub fn list_document_versions(
    connection: &Connection,
    workspace_id: &str,
    document_id: SourceDocumentId,
) -> Result<Vec<DocumentVersionRecord>, String> {
    scope(workspace_id)?;
    let mut statement = connection
        .prepare(
            "SELECT id FROM knowledge_document_versions
             WHERE workspace_id = ?1 AND document_id = ?2
             ORDER BY created_at DESC, id DESC",
        )
        .map_err(|error| error.to_string())?;
    let ids = statement
        .query_map(params![workspace_id, document_id.to_string()], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    ids.into_iter()
        .map(|id| {
            let id: DocumentVersionId = parse(id, "document version ID")?;
            get_document_version(connection, workspace_id, id)?
                .ok_or_else(|| "document version not found".to_string())
        })
        .collect()
}

pub fn rename_knowledge_document(
    connection: &Connection,
    workspace_id: &str,
    id: SourceDocumentId,
    display_name: &str,
) -> Result<SourceDocumentRecord, String> {
    scope(workspace_id)?;
    let display_name = display_name.trim();
    required("document display name", display_name)?;
    let changed = connection
        .execute(
            "UPDATE knowledge_source_documents SET display_name = ?1, updated_at = ?2
             WHERE workspace_id = ?3 AND id = ?4",
            params![display_name, now(), workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("source document not found".to_string());
    }
    super::get_source_document(connection, workspace_id, id)?
        .ok_or_else(|| "source document not found".to_string())
}

pub fn delete_knowledge_document(
    connection: &mut Connection,
    workspace_id: &str,
    id: SourceDocumentId,
) -> Result<(), String> {
    scope(workspace_id)?;
    super::get_source_document(connection, workspace_id, id)?
        .ok_or_else(|| "source document not found".to_string())?;
    let active_task_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM background_tasks
             WHERE workspace_id = ?1
               AND state IN ('queued', 'running', 'waiting_external', 'paused', 'interrupted')
               AND json_extract(payload_json, '$.document_id') = ?2",
            params![workspace_id, id.to_string()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active_task_count != 0 {
        return Err("knowledge_document_busy: cancel active tasks before deletion".to_string());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM knowledge_chunks_fts WHERE workspace_id = ?1 AND document_id = ?2",
            params![workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    let deleted = transaction
        .execute(
            "DELETE FROM knowledge_source_documents WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if deleted == 0 {
        return Err("source document not found".to_string());
    }
    transaction.commit().map_err(|error| error.to_string())
}

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
            copy_document(
                &transaction,
                workspace_id,
                &document,
                destination.id,
            )?;
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

pub fn preview_delete_knowledge_base(
    connection: &Connection,
    workspace_id: &str,
    id: KnowledgeBaseId,
) -> Result<KnowledgeBaseDeleteImpact, String> {
    scope(workspace_id)?;
    connection
        .query_row(
            "SELECT bases.name,
                    (SELECT COUNT(*) FROM knowledge_source_documents d
                     WHERE d.workspace_id = bases.workspace_id
                       AND d.knowledge_base_id = bases.id),
                    (SELECT COUNT(*) FROM knowledge_document_versions v
                     JOIN knowledge_source_documents d ON d.id = v.document_id
                     WHERE d.workspace_id = bases.workspace_id
                       AND d.knowledge_base_id = bases.id),
                    (SELECT COUNT(*) FROM knowledge_chunks c
                     JOIN knowledge_document_versions v ON v.id = c.version_id
                     JOIN knowledge_source_documents d ON d.id = v.document_id
                     WHERE d.workspace_id = bases.workspace_id
                       AND d.knowledge_base_id = bases.id),
                    (SELECT COUNT(*) FROM knowledge_assets a
                     JOIN knowledge_document_versions v ON v.id = a.version_id
                     JOIN knowledge_source_documents d ON d.id = v.document_id
                     WHERE d.workspace_id = bases.workspace_id
                       AND d.knowledge_base_id = bases.id),
                    (SELECT COUNT(*) FROM background_tasks tasks
                     WHERE tasks.workspace_id = bases.workspace_id
                       AND tasks.state IN ('queued', 'running', 'waiting_external', 'paused', 'interrupted')
                       AND EXISTS (
                         SELECT 1 FROM knowledge_source_documents d
                         WHERE d.workspace_id = bases.workspace_id
                           AND d.knowledge_base_id = bases.id
                           AND d.id = json_extract(tasks.payload_json, '$.document_id')
                       ))
             FROM knowledge_bases bases
             WHERE bases.workspace_id = ?1 AND bases.id = ?2",
            params![workspace_id, id.to_string()],
            |row| {
                Ok(KnowledgeBaseDeleteImpact {
                    knowledge_base_id: id,
                    name: row.get(0)?,
                    document_count: row.get(1)?,
                    version_count: row.get(2)?,
                    chunk_count: row.get(3)?,
                    asset_count: row.get(4)?,
                    active_task_count: row.get(5)?,
                })
            },
        )
        .map_err(|error| {
            if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
                "knowledge base not found".to_string()
            } else {
                error.to_string()
            }
        })
}

pub fn delete_knowledge_base_confirmed(
    connection: &mut Connection,
    workspace_id: &str,
    id: KnowledgeBaseId,
) -> Result<(), String> {
    let impact = preview_delete_knowledge_base(connection, workspace_id, id)?;
    if impact.active_task_count != 0 {
        return Err("knowledge_base_busy: cancel active tasks before deletion".to_string());
    }
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let deleted = transaction
        .execute(
            "DELETE FROM knowledge_bases WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if deleted == 0 {
        return Err("knowledge base not found".to_string());
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub fn read_knowledge_health(
    connection: &Connection,
    workspace_id: &str,
) -> Result<KnowledgeHealth, String> {
    scope(workspace_id)?;
    connection
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM knowledge_bases WHERE workspace_id = ?1),
               (SELECT COUNT(*) FROM knowledge_source_documents WHERE workspace_id = ?1),
               (SELECT COUNT(*) FROM knowledge_source_documents
                WHERE workspace_id = ?1 AND active_version_id IS NOT NULL),
               (SELECT COUNT(*) FROM knowledge_document_versions WHERE workspace_id = ?1),
               (SELECT COUNT(*) FROM knowledge_chunks WHERE workspace_id = ?1),
               (SELECT COUNT(*) FROM knowledge_chunk_embeddings WHERE workspace_id = ?1),
               (SELECT COUNT(*) FROM background_tasks
                WHERE workspace_id = ?1
                  AND kind IN ('mineru_parse', 'rag_index_rebuild')
                  AND state IN ('queued', 'running', 'waiting_external', 'paused', 'interrupted'))",
            [workspace_id],
            |row| {
                Ok(KnowledgeHealth {
                    knowledge_base_count: row.get(0)?,
                    document_count: row.get(1)?,
                    active_document_count: row.get(2)?,
                    version_count: row.get(3)?,
                    chunk_count: row.get(4)?,
                    indexed_chunk_count: row.get(5)?,
                    active_task_count: row.get(6)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}
