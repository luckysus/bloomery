use super::support::{ensure_owner, now, parse, scope};
use super::IngestAttemptRecord;
use crate::rag::model::{DocumentVersionId, IngestAttemptId, IngestAttemptState, SourceDocumentId};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn create_ingest_attempt(
    connection: &Connection,
    workspace_id: &str,
    document_id: SourceDocumentId,
    version_id: Option<DocumentVersionId>,
    task_id: Option<String>,
) -> Result<IngestAttemptRecord, String> {
    scope(workspace_id)?;
    ensure_owner(
        connection,
        "knowledge_source_documents",
        workspace_id,
        &document_id.to_string(),
    )?;
    if let Some(version_id) = version_id {
        let belongs = connection
            .query_row(
                "SELECT 1 FROM knowledge_document_versions
                 WHERE workspace_id = ?1 AND id = ?2 AND document_id = ?3",
                params![
                    workspace_id,
                    version_id.to_string(),
                    document_id.to_string()
                ],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !belongs {
            return Err("document version does not belong to source".to_string());
        }
    }
    if let Some(task_id) = &task_id {
        Uuid::parse_str(task_id).map_err(|error| format!("invalid task ID: {error}"))?;
    }
    let timestamp = now();
    let record = IngestAttemptRecord {
        id: IngestAttemptId::new(),
        document_id,
        version_id,
        task_id,
        state: IngestAttemptState::Running,
        error_code: None,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        finished_at: None,
    };
    connection
        .execute(
            "INSERT INTO knowledge_ingest_attempts
             (id, workspace_id, document_id, version_id, task_id, state, error_code,
              created_at, updated_at, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?7, NULL)",
            params![
                record.id.to_string(),
                workspace_id,
                record.document_id.to_string(),
                record.version_id.map(|id| id.to_string()),
                record.task_id,
                record.state.as_str(),
                record.created_at
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(record)
}

pub fn get_ingest_attempt(
    connection: &Connection,
    workspace_id: &str,
    id: IngestAttemptId,
) -> Result<Option<IngestAttemptRecord>, String> {
    scope(workspace_id)?;
    connection
        .query_row(
            "SELECT id, document_id, version_id, task_id, state, error_code,
                    created_at, updated_at, finished_at
             FROM knowledge_ingest_attempts WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get(3)?,
                    row.get::<_, String>(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .map(|row| {
            Ok(IngestAttemptRecord {
                id: parse(row.0, "ingest attempt ID")?,
                document_id: parse(row.1, "source document ID")?,
                version_id: row
                    .2
                    .map(|value| parse(value, "document version ID"))
                    .transpose()?,
                task_id: row.3,
                state: row.4.parse()?,
                error_code: row.5,
                created_at: row.6,
                updated_at: row.7,
                finished_at: row.8,
            })
        })
        .transpose()
}

pub fn finish_ingest_attempt(
    connection: &mut Connection,
    workspace_id: &str,
    id: IngestAttemptId,
    state: IngestAttemptState,
    error_code: Option<&str>,
) -> Result<IngestAttemptRecord, String> {
    scope(workspace_id)?;
    match (state, error_code) {
        (IngestAttemptState::Running, _) => {
            return Err("invalid_attempt_transition: target must be terminal".to_string());
        }
        (IngestAttemptState::Failed, None) => {
            return Err("attempt_error_required: failed attempt requires error code".to_string());
        }
        (IngestAttemptState::Failed, Some(code))
            if code.is_empty()
                || code.len() > 128
                || !code.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                }) =>
        {
            return Err("invalid attempt error code".to_string());
        }
        (IngestAttemptState::Failed, Some(_)) => {}
        (_, Some(_)) => return Err("attempt_error_not_allowed".to_string()),
        (_, None) => {}
    }
    let timestamp = now();
    let changed = connection
        .execute(
            "UPDATE knowledge_ingest_attempts
             SET state = ?1, error_code = ?2, updated_at = ?3, finished_at = ?3
             WHERE workspace_id = ?4 AND id = ?5 AND state = 'running'",
            params![
                state.as_str(),
                error_code,
                timestamp,
                workspace_id,
                id.to_string()
            ],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return if get_ingest_attempt(connection, workspace_id, id)?.is_some() {
            Err("attempt_already_finished: ingest attempt is terminal".to_string())
        } else {
            Err("ingest attempt not found".to_string())
        };
    }
    get_ingest_attempt(connection, workspace_id, id)?
        .ok_or_else(|| "ingest attempt not found".to_string())
}
