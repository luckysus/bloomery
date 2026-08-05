use super::records::{DocumentVersionRecord, KnowledgeBaseRecord, SourceDocumentRecord};
use super::support::{now, parse, scope};
use super::{get_document_version, get_knowledge_base};
use crate::rag::model::{required, DocumentVersionId, KnowledgeBaseId, SourceDocumentId};
use rusqlite::{params, Connection};
use serde::Serialize;

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
