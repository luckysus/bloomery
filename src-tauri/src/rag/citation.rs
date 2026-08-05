use crate::rag::model::{AssetId, KnowledgeBaseId, SourceLocation};
use crate::rag::rerank::RerankDegradationReason;
use crate::rag::retrieve::RetrievedChunk;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

const MAX_EVIDENCE_ITEMS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrievalConfigSnapshot {
    pub knowledge_base_ids: Vec<KnowledgeBaseId>,
    pub lexical_limit: usize,
    pub dense_limit: usize,
    pub candidate_limit: usize,
    pub rrf_k: u32,
    pub embedding_provider_profile_id: String,
    pub embedding_model_id: String,
    pub rerank_provider_profile_id: Option<String>,
    pub rerank_model_id: Option<String>,
    pub rerank_degradation: Option<RerankDegradationReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceAsset {
    pub id: AssetId,
    pub kind: String,
    pub storage_key: String,
    pub media_type: String,
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub citation_number: u32,
    pub chunk: RetrievedChunk,
    pub assets: Vec<EvidenceAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidencePack {
    pub id: Uuid,
    pub workspace_id: String,
    pub query: String,
    pub configuration: RetrievalConfigSnapshot,
    pub evidence: Vec<EvidenceItem>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationSourceState {
    Active,
    Inactive,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedCitation {
    pub audit_id: Uuid,
    pub citation_number: u32,
    pub label: String,
    pub source_state: CitationSourceState,
    pub chunk: RetrievedChunk,
    pub assets: Vec<EvidenceAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationError {
    code: &'static str,
    message: String,
}

impl CitationError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for CitationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CitationError {}

pub fn persist_evidence_pack(
    connection: &Connection,
    workspace_id: &str,
    query: &str,
    configuration: RetrievalConfigSnapshot,
    chunks: Vec<RetrievedChunk>,
) -> Result<EvidencePack, CitationError> {
    validate_scope(workspace_id)?;
    let query = query.trim();
    if query.is_empty() {
        return Err(CitationError::new(
            "retrieval_query_invalid",
            "retrieval query is required",
        ));
    }
    validate_configuration(&configuration)?;
    if chunks.len() > MAX_EVIDENCE_ITEMS {
        return Err(CitationError::new(
            "evidence_limit_exceeded",
            "retrieval evidence exceeds the bounded candidate limit",
        ));
    }
    let mut seen = HashSet::new();
    let mut evidence = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.into_iter().enumerate() {
        let identity = (chunk.version_id, chunk.chunk_id.clone());
        if !seen.insert(identity) || !valid_scores(&chunk) {
            return Err(CitationError::new(
                "evidence_invalid",
                "retrieval evidence contains duplicate identity or invalid scores",
            ));
        }
        let assets = matching_assets(connection, workspace_id, &chunk)?;
        evidence.push(EvidenceItem {
            citation_number: u32::try_from(index + 1).map_err(|_| {
                CitationError::new("evidence_limit_exceeded", "too many evidence items")
            })?,
            chunk,
            assets,
        });
    }
    let pack = EvidencePack {
        id: Uuid::new_v4(),
        workspace_id: workspace_id.to_string(),
        query: query.to_string(),
        configuration,
        evidence,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    let configuration_json = serde_json::to_string(&pack.configuration).map_err(encode)?;
    let evidence_json = serde_json::to_string(&pack.evidence).map_err(encode)?;
    connection
        .execute(
            "INSERT INTO retrieval_audits
             (id, workspace_id, query, configuration_json, evidence_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                pack.id.to_string(),
                pack.workspace_id,
                pack.query,
                configuration_json,
                evidence_json,
                pack.created_at
            ],
        )
        .map_err(storage)?;
    Ok(pack)
}

pub fn load_evidence_pack(
    connection: &Connection,
    workspace_id: &str,
    audit_id: Uuid,
) -> Result<Option<EvidencePack>, CitationError> {
    validate_scope(workspace_id)?;
    connection
        .query_row(
            "SELECT query, configuration_json, evidence_json, created_at
             FROM retrieval_audits WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, audit_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(storage)?
        .map(|(query, configuration, evidence, created_at)| {
            Ok(EvidencePack {
                id: audit_id,
                workspace_id: workspace_id.to_string(),
                query,
                configuration: serde_json::from_str(&configuration).map_err(decode)?,
                evidence: serde_json::from_str(&evidence).map_err(decode)?,
                created_at,
            })
        })
        .transpose()
}

pub fn resolve_citation(
    connection: &Connection,
    workspace_id: &str,
    audit_id: Uuid,
    citation_number: u32,
) -> Result<Option<ResolvedCitation>, CitationError> {
    if citation_number == 0 {
        return Err(CitationError::new(
            "citation_number_invalid",
            "citation numbers start at one",
        ));
    }
    let Some(pack) = load_evidence_pack(connection, workspace_id, audit_id)? else {
        return Ok(None);
    };
    let Some(item) = pack
        .evidence
        .into_iter()
        .find(|item| item.citation_number == citation_number)
    else {
        return Ok(None);
    };
    let source_state = source_state(connection, workspace_id, &item.chunk)?;
    Ok(Some(ResolvedCitation {
        audit_id,
        citation_number,
        label: citation_label(&item.chunk.source_name, &item.chunk.source_location),
        source_state,
        chunk: item.chunk,
        assets: item.assets,
    }))
}

fn matching_assets(
    connection: &Connection,
    workspace_id: &str,
    chunk: &RetrievedChunk,
) -> Result<Vec<EvidenceAsset>, CitationError> {
    let location = serde_json::to_string(&chunk.source_location).map_err(encode)?;
    let mut statement = connection
        .prepare(
            "SELECT id, kind, storage_key, media_type, source_location_json
             FROM knowledge_assets
             WHERE workspace_id = ?1 AND version_id = ?2 AND source_location_json = ?3
             ORDER BY id",
        )
        .map_err(storage)?;
    let rows = statement
        .query_map(
            params![workspace_id, chunk.version_id.to_string(), location],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .map_err(storage)?;
    rows.map(|row| {
        let (id, kind, storage_key, media_type, location) = row.map_err(storage)?;
        Ok(EvidenceAsset {
            id: AssetId::from_str(&id).map_err(row_error)?,
            kind,
            storage_key,
            media_type,
            source_location: location
                .map(|value| serde_json::from_str(&value).map_err(decode))
                .transpose()?,
        })
    })
    .collect()
}

fn source_state(
    connection: &Connection,
    workspace_id: &str,
    chunk: &RetrievedChunk,
) -> Result<CitationSourceState, CitationError> {
    let active = connection
        .query_row(
            "SELECT active_version_id FROM knowledge_source_documents
             WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, chunk.document_id.to_string()],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(storage)?;
    Ok(match active {
        None => CitationSourceState::Deleted,
        Some(Some(version)) if version == chunk.version_id.to_string() => {
            CitationSourceState::Active
        }
        Some(_) => CitationSourceState::Inactive,
    })
}

fn citation_label(source_name: &str, location: &SourceLocation) -> String {
    let suffix = match location {
        SourceLocation::PdfPage { page, .. } => format!("page {page}"),
        SourceLocation::SheetRange { sheet, range } => format!("{sheet}!{range}"),
        SourceLocation::Heading { path } => path.join(" > "),
        SourceLocation::TextOffsets { start, end } => format!("characters {start}-{end}"),
    };
    format!("{source_name}, {suffix}")
}

fn validate_configuration(config: &RetrievalConfigSnapshot) -> Result<(), CitationError> {
    for (name, value) in [
        (
            "embedding provider profile ID",
            config.embedding_provider_profile_id.as_str(),
        ),
        ("embedding model ID", config.embedding_model_id.as_str()),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(CitationError::new(
                "retrieval_configuration_invalid",
                format!("{name} is invalid"),
            ));
        }
    }
    Ok(())
}

fn validate_scope(workspace_id: &str) -> Result<(), CitationError> {
    if workspace_id.is_empty() || workspace_id.trim() != workspace_id {
        Err(CitationError::new(
            "retrieval_scope_invalid",
            "workspace ID is invalid",
        ))
    } else {
        Ok(())
    }
}

fn valid_scores(chunk: &RetrievedChunk) -> bool {
    chunk.rrf_score.is_finite() && chunk.rerank_score.is_none_or(|score| score.is_finite())
}

fn encode(error: serde_json::Error) -> CitationError {
    CitationError::new("evidence_encode_failed", error.to_string())
}

fn decode(error: serde_json::Error) -> CitationError {
    CitationError::new("evidence_decode_failed", error.to_string())
}

fn row_error(error: impl fmt::Display) -> CitationError {
    CitationError::new("evidence_row_invalid", error.to_string())
}

fn storage(error: rusqlite::Error) -> CitationError {
    CitationError::new("evidence_storage_failed", error.to_string())
}
