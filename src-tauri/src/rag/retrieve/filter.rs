use crate::rag::model::{DocumentVersionId, KnowledgeBaseId};
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use std::str::FromStr;

pub fn active_versions(
    connection: &Connection,
    workspace_id: &str,
    knowledge_base_ids: &[KnowledgeBaseId],
) -> Result<Vec<DocumentVersionId>, String> {
    if workspace_id.is_empty() || workspace_id.trim() != workspace_id {
        return Err("retrieval workspace is invalid".to_string());
    }
    if knowledge_base_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut base_ids = knowledge_base_ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    base_ids.sort_unstable();
    base_ids.dedup();
    let placeholders = vec!["?"; base_ids.len()].join(", ");
    let sql = format!(
        "SELECT versions.id
         FROM knowledge_source_documents AS documents
         JOIN knowledge_document_versions AS versions
           ON versions.workspace_id = documents.workspace_id
          AND versions.document_id = documents.id
          AND versions.id = documents.active_version_id
         WHERE documents.workspace_id = ?
           AND documents.knowledge_base_id IN ({placeholders})
         ORDER BY versions.id"
    );
    let mut values = vec![Value::Text(workspace_id.to_string())];
    values.extend(base_ids.into_iter().map(Value::Text));
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params_from_iter(values), |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let value = row.map_err(|error| error.to_string())?;
        DocumentVersionId::from_str(&value).map_err(|error| error.to_string())
    })
    .collect()
}
