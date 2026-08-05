use super::records::DocumentVersionRecord;
use super::support::{ensure_owner, now, parse, scope};
use crate::rag::model::{required, sha256, DocumentVersionId, NewDocumentVersion};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn create_document_version(
    connection: &Connection,
    workspace_id: &str,
    input: NewDocumentVersion,
) -> Result<DocumentVersionRecord, String> {
    create_document_version_with_manifest(connection, workspace_id, input, true)
}

pub fn create_pending_document_version(
    connection: &Connection,
    workspace_id: &str,
    input: NewDocumentVersion,
) -> Result<DocumentVersionRecord, String> {
    if input.expected_asset_count != 0 || input.expected_chunk_count != 0 {
        return Err("pending document manifest counts must be zero".to_string());
    }
    create_document_version_with_manifest(connection, workspace_id, input, false)
}

fn create_document_version_with_manifest(
    connection: &Connection,
    workspace_id: &str,
    input: NewDocumentVersion,
    manifest_sealed: bool,
) -> Result<DocumentVersionRecord, String> {
    scope(workspace_id)?;
    ensure_owner(
        connection,
        "knowledge_source_documents",
        workspace_id,
        &input.document_id.to_string(),
    )?;
    sha256("content_sha256", &input.content_sha256)?;
    for (name, value) in [
        ("mime_type", input.mime_type.as_str()),
        ("parser", input.parser.as_str()),
        ("parser_version", input.parser_version.as_str()),
        ("chunk_policy_version", input.chunk_policy_version.as_str()),
        ("embedding_model_id", input.embedding_model_id.as_str()),
    ] {
        required(name, value)?;
    }
    Uuid::parse_str(&input.embedding_profile_id)
        .map_err(|error| format!("invalid embedding profile ID: {error}"))?;
    if input.embedding_dimension == 0 {
        return Err("embedding dimension must be positive".to_string());
    }
    let timestamp = now();
    let record = DocumentVersionRecord {
        id: DocumentVersionId::new(),
        document_id: input.document_id,
        content_sha256: input.content_sha256,
        mime_type: input.mime_type,
        parser: input.parser,
        parser_version: input.parser_version,
        chunk_policy_version: input.chunk_policy_version,
        embedding_profile_id: input.embedding_profile_id,
        embedding_model_id: input.embedding_model_id,
        embedding_dimension: input.embedding_dimension,
        expected_asset_count: input.expected_asset_count,
        expected_chunk_count: input.expected_chunk_count,
        manifest_sealed,
        created_at: timestamp,
        activated_at: None,
    };
    connection
        .execute(
            "INSERT INTO knowledge_document_versions
             (id, workspace_id, document_id, content_sha256, mime_type, parser, parser_version,
              chunk_policy_version, embedding_profile_id, embedding_model_id,
              embedding_dimension, expected_asset_count, expected_chunk_count, manifest_sealed,
              created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                record.id.to_string(),
                workspace_id,
                record.document_id.to_string(),
                record.content_sha256,
                record.mime_type,
                record.parser,
                record.parser_version,
                record.chunk_policy_version,
                record.embedding_profile_id,
                record.embedding_model_id,
                record.embedding_dimension,
                record.expected_asset_count,
                record.expected_chunk_count,
                i64::from(record.manifest_sealed),
                record.created_at
            ],
        )
        .map_err(|error| {
            if error
                .to_string()
                .contains("knowledge_document_versions.document_id")
            {
                "duplicate_document_version: content already exists".to_string()
            } else {
                error.to_string()
            }
        })?;
    Ok(record)
}

pub fn get_document_version(
    connection: &Connection,
    workspace_id: &str,
    id: DocumentVersionId,
) -> Result<Option<DocumentVersionRecord>, String> {
    scope(workspace_id)?;
    connection
        .query_row(
            "SELECT id, document_id, content_sha256, mime_type, parser, parser_version,
                    chunk_policy_version, embedding_profile_id, embedding_model_id,
                    embedding_dimension, expected_asset_count, expected_chunk_count,
                    manifest_sealed, created_at, activated_at
             FROM knowledge_document_versions WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get::<_, i64>(12)? != 0,
                    row.get(13)?,
                    row.get(14)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .map(
            |(
                id,
                document_id,
                content_sha256,
                mime_type,
                parser,
                parser_version,
                chunk_policy_version,
                embedding_profile_id,
                embedding_model_id,
                embedding_dimension,
                expected_asset_count,
                expected_chunk_count,
                manifest_sealed,
                created_at,
                activated_at,
            )| {
                Ok(DocumentVersionRecord {
                    id: parse(id, "document version ID")?,
                    document_id: parse(document_id, "source document ID")?,
                    content_sha256,
                    mime_type,
                    parser,
                    parser_version,
                    chunk_policy_version,
                    embedding_profile_id,
                    embedding_model_id,
                    embedding_dimension,
                    expected_asset_count,
                    expected_chunk_count,
                    manifest_sealed,
                    created_at,
                    activated_at,
                })
            },
        )
        .transpose()
}

pub fn seal_document_manifest(
    connection: &mut Connection,
    workspace_id: &str,
    version_id: DocumentVersionId,
    expected_asset_count: u32,
    expected_chunk_count: u32,
) -> Result<DocumentVersionRecord, String> {
    scope(workspace_id)?;
    if expected_chunk_count == 0 {
        return Err("document_manifest_empty: parsed document has no chunks".to_string());
    }
    let current = get_document_version(connection, workspace_id, version_id)?
        .ok_or_else(|| "document version not found".to_string())?;
    if current.manifest_sealed {
        return if (current.expected_asset_count, current.expected_chunk_count)
            == (expected_asset_count, expected_chunk_count)
        {
            Ok(current)
        } else {
            Err("document_manifest_mismatch: sealed counts differ".to_string())
        };
    }
    connection
        .execute(
            "UPDATE knowledge_document_versions
             SET expected_asset_count = ?1, expected_chunk_count = ?2, manifest_sealed = 1
             WHERE workspace_id = ?3 AND id = ?4 AND manifest_sealed = 0",
            params![
                expected_asset_count,
                expected_chunk_count,
                workspace_id,
                version_id.to_string()
            ],
        )
        .map_err(|error| error.to_string())?;
    get_document_version(connection, workspace_id, version_id)?
        .ok_or_else(|| "document version not found".to_string())
}
