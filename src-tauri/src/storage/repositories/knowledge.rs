mod activation;
mod attempts;
mod catalog;
mod content;
mod content_batch;
mod flat_index;
mod records;
mod support;
mod vectors;
mod versions;

pub use activation::{activate_document_version, activate_document_version_for_task};
pub use attempts::{create_ingest_attempt, finish_ingest_attempt, get_ingest_attempt};
pub use catalog::{
    delete_knowledge_base_confirmed, list_document_versions, list_source_documents,
    preview_delete_knowledge_base, read_knowledge_health, rename_knowledge_base,
    KnowledgeBaseDeleteImpact, KnowledgeHealth,
};
pub use content::{
    add_asset, add_chunk, index_chunk_fts, record_chunk_embedding, set_vector_watermark,
};
pub use content_batch::persist_parsed_content;
pub use flat_index::finalize_flat_index;
pub use records::{
    DocumentVersionRecord, IngestAttemptRecord, KnowledgeBaseRecord, SourceDocumentRecord,
};
pub use vectors::{
    find_reusable_vector, linked_embedding_chunk_ids, list_chunks_for_embedding,
    persist_embedding_batch, persist_reused_embedding_links,
};
pub use versions::{
    create_document_version, create_pending_document_version, get_document_version,
    seal_document_manifest,
};

use self::support::{ensure_owner, now, parse, scope};
use crate::rag::model::{required, KnowledgeBaseId, NewSourceDocument, SourceDocumentId};
use rusqlite::{params, Connection, OptionalExtension};

pub fn create_knowledge_base(
    connection: &Connection,
    workspace_id: &str,
    name: &str,
) -> Result<KnowledgeBaseRecord, String> {
    scope(workspace_id)?;
    let name = name.trim();
    required("knowledge base name", name)?;
    let record = KnowledgeBaseRecord {
        id: KnowledgeBaseId::new(),
        name: name.to_string(),
        created_at: now(),
        updated_at: now(),
    };
    connection
        .execute(
            "INSERT INTO knowledge_bases (id, workspace_id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.id.to_string(),
                workspace_id,
                record.name,
                record.created_at,
                record.updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(record)
}

pub fn get_knowledge_base(
    connection: &Connection,
    workspace_id: &str,
    id: KnowledgeBaseId,
) -> Result<Option<KnowledgeBaseRecord>, String> {
    scope(workspace_id)?;
    connection
        .query_row(
            "SELECT id, name, created_at, updated_at FROM knowledge_bases
             WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .map(|(id, name, created_at, updated_at)| {
            Ok(KnowledgeBaseRecord {
                id: parse(id, "knowledge base ID")?,
                name,
                created_at,
                updated_at,
            })
        })
        .transpose()
}

pub fn list_knowledge_bases(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<KnowledgeBaseRecord>, String> {
    scope(workspace_id)?;
    let mut statement = connection
        .prepare(
            "SELECT id, name, created_at, updated_at FROM knowledge_bases
             WHERE workspace_id = ?1 ORDER BY updated_at DESC, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let (id, name, created_at, updated_at) = row.map_err(|error| error.to_string())?;
        Ok(KnowledgeBaseRecord {
            id: parse(id, "knowledge base ID")?,
            name,
            created_at,
            updated_at,
        })
    })
    .collect()
}

pub fn create_source_document(
    connection: &Connection,
    workspace_id: &str,
    input: NewSourceDocument,
) -> Result<SourceDocumentRecord, String> {
    scope(workspace_id)?;
    required("document display name", &input.display_name)?;
    required("source kind", &input.source_kind)?;
    ensure_owner(
        connection,
        "knowledge_bases",
        workspace_id,
        &input.knowledge_base_id.to_string(),
    )?;
    let timestamp = now();
    let record = SourceDocumentRecord {
        id: SourceDocumentId::new(),
        knowledge_base_id: input.knowledge_base_id,
        display_name: input.display_name,
        source_kind: input.source_kind,
        active_version_id: None,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };
    connection
        .execute(
            "INSERT INTO knowledge_source_documents
             (id, workspace_id, knowledge_base_id, display_name, source_kind,
              active_version_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?6)",
            params![
                record.id.to_string(),
                workspace_id,
                record.knowledge_base_id.to_string(),
                record.display_name,
                record.source_kind,
                record.created_at
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(record)
}

pub fn get_source_document(
    connection: &Connection,
    workspace_id: &str,
    id: SourceDocumentId,
) -> Result<Option<SourceDocumentRecord>, String> {
    scope(workspace_id)?;
    connection
        .query_row(
            "SELECT id, knowledge_base_id, display_name, source_kind, active_version_id,
                    created_at, updated_at
             FROM knowledge_source_documents WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
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
        .optional()
        .map_err(|error| error.to_string())?
        .map(
            |(id, base, display_name, source_kind, active, created_at, updated_at)| {
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
            },
        )
        .transpose()
}
