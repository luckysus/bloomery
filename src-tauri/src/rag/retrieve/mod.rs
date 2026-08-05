pub mod filter;
pub mod rrf;

use self::filter::active_versions;
use self::rrf::{reciprocal_rank_fusion, FusedChunk, RankedChunk};
use crate::rag::index::fts::{search as search_fts, FtsSearchRequest};
use crate::rag::index::vector::{CandidateFilter, VectorIndex};
use crate::rag::model::{
    ChunkId, DocumentVersionId, KnowledgeBaseId, SourceDocumentId, SourceLocation,
};
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct HybridSearchRequest {
    pub workspace_id: String,
    pub query: String,
    pub query_vector: Vec<f32>,
    pub knowledge_base_ids: Vec<KnowledgeBaseId>,
    pub lexical_limit: usize,
    pub dense_limit: usize,
    pub candidate_limit: usize,
    pub rrf_k: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievedChunk {
    pub knowledge_base_id: KnowledgeBaseId,
    pub document_id: SourceDocumentId,
    pub version_id: DocumentVersionId,
    pub chunk_id: ChunkId,
    pub source_name: String,
    pub source_location: SourceLocation,
    pub text: String,
    pub lexical_rank: Option<usize>,
    pub dense_rank: Option<usize>,
    pub rrf_score: f64,
    pub rerank_score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalError {
    code: &'static str,
    message: String,
}

impl RetrievalError {
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

impl fmt::Display for RetrievalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for RetrievalError {}

pub fn retrieve(
    connection: &Connection,
    vector_index: &dyn VectorIndex,
    request: &HybridSearchRequest,
) -> Result<Vec<RetrievedChunk>, RetrievalError> {
    validate_request(vector_index, request)?;
    if request.knowledge_base_ids.is_empty() || request.candidate_limit == 0 {
        return Ok(Vec::new());
    }
    let versions = active_versions(
        connection,
        &request.workspace_id,
        &request.knowledge_base_ids,
    )
    .map_err(|error| RetrievalError::new("retrieval_filter_failed", error))?;
    if versions.is_empty() {
        return Ok(Vec::new());
    }
    let allowed = versions.iter().copied().collect::<HashSet<_>>();

    let lexical = if request.lexical_limit == 0 {
        Vec::new()
    } else {
        search_fts(
            connection,
            &FtsSearchRequest {
                workspace_id: request.workspace_id.clone(),
                query: request.query.clone(),
                knowledge_base_ids: request.knowledge_base_ids.clone(),
                limit: request.lexical_limit,
            },
        )
        .map_err(|error| RetrievalError::new("retrieval_fts_failed", error.to_string()))?
        .into_iter()
        .filter(|hit| allowed.contains(&hit.version_id))
        .map(|hit| RankedChunk {
            version_id: hit.version_id,
            chunk_id: hit.chunk_id,
        })
        .collect()
    };
    let dense = if request.dense_limit == 0 {
        Vec::new()
    } else {
        vector_index
            .search(
                &request.query_vector,
                request.dense_limit,
                &CandidateFilter::new(versions),
            )
            .map_err(|error| RetrievalError::new("retrieval_dense_failed", error.to_string()))?
            .into_iter()
            .filter(|hit| allowed.contains(&hit.version_id))
            .map(|hit| RankedChunk {
                version_id: hit.version_id,
                chunk_id: hit.chunk_id,
            })
            .collect()
    };
    let fused = reciprocal_rank_fusion(&lexical, &dense, request.rrf_k, request.candidate_limit);
    fetch_authoritative(connection, &request.workspace_id, fused)
}

fn validate_request(
    vector_index: &dyn VectorIndex,
    request: &HybridSearchRequest,
) -> Result<(), RetrievalError> {
    if request.workspace_id.is_empty() || request.workspace_id.trim() != request.workspace_id {
        return Err(RetrievalError::new(
            "retrieval_scope_invalid",
            "workspace ID is invalid",
        ));
    }
    if vector_index.watermark().workspace_id != request.workspace_id {
        return Err(RetrievalError::new(
            "retrieval_index_mismatch",
            "vector index belongs to another workspace",
        ));
    }
    Ok(())
}

fn fetch_authoritative(
    connection: &Connection,
    workspace_id: &str,
    fused: Vec<FusedChunk>,
) -> Result<Vec<RetrievedChunk>, RetrievalError> {
    if fused.is_empty() {
        return Ok(Vec::new());
    }
    let predicates = vec!["(chunks.version_id = ? AND chunks.id = ?)"; fused.len()].join(" OR ");
    let sql = format!(
        "SELECT documents.knowledge_base_id, documents.id, versions.id, chunks.id,
                documents.display_name, chunks.source_location_json, chunks.text
         FROM knowledge_chunks AS chunks
         JOIN knowledge_document_versions AS versions
           ON versions.workspace_id = chunks.workspace_id AND versions.id = chunks.version_id
         JOIN knowledge_source_documents AS documents
           ON documents.workspace_id = versions.workspace_id
          AND documents.id = versions.document_id
          AND documents.active_version_id = versions.id
         WHERE chunks.workspace_id = ? AND ({predicates})"
    );
    let mut values = vec![Value::Text(workspace_id.to_string())];
    for hit in &fused {
        values.push(Value::Text(hit.version_id.to_string()));
        values.push(Value::Text(hit.chunk_id.to_string()));
    }
    let mut statement = connection.prepare(&sql).map_err(storage)?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok(SourceRow {
                knowledge_base_id: row.get(0)?,
                document_id: row.get(1)?,
                version_id: row.get(2)?,
                chunk_id: row.get(3)?,
                source_name: row.get(4)?,
                source_location: row.get(5)?,
                text: row.get(6)?,
            })
        })
        .map_err(storage)?;
    let mut sources = HashMap::new();
    for row in rows {
        let source = row.map_err(storage)?;
        let key = source.key()?;
        sources.insert(key, source);
    }
    let mut results = Vec::with_capacity(fused.len());
    for hit in fused {
        let key = RankedChunk {
            version_id: hit.version_id,
            chunk_id: hit.chunk_id.clone(),
        };
        if let Some(source) = sources.remove(&key) {
            results.push(source.into_retrieved(hit)?);
        }
    }
    Ok(results)
}

struct SourceRow {
    knowledge_base_id: String,
    document_id: String,
    version_id: String,
    chunk_id: String,
    source_name: String,
    source_location: String,
    text: String,
}

impl SourceRow {
    fn key(&self) -> Result<RankedChunk, RetrievalError> {
        Ok(RankedChunk {
            version_id: parse(&self.version_id, "version ID")?,
            chunk_id: ChunkId::new(&self.chunk_id).map_err(row_error)?,
        })
    }

    fn into_retrieved(self, hit: FusedChunk) -> Result<RetrievedChunk, RetrievalError> {
        Ok(RetrievedChunk {
            knowledge_base_id: parse(&self.knowledge_base_id, "knowledge base ID")?,
            document_id: parse(&self.document_id, "document ID")?,
            version_id: hit.version_id,
            chunk_id: hit.chunk_id,
            source_name: self.source_name,
            source_location: serde_json::from_str(&self.source_location).map_err(row_error)?,
            text: self.text,
            lexical_rank: hit.lexical_rank,
            dense_rank: hit.dense_rank,
            rrf_score: hit.rrf_score,
            rerank_score: None,
        })
    }
}

fn parse<T: FromStr>(value: &str, name: &str) -> Result<T, RetrievalError>
where
    T::Err: fmt::Display,
{
    value
        .parse()
        .map_err(|error| row_error(format!("invalid {name}: {error}")))
}

fn row_error(error: impl fmt::Display) -> RetrievalError {
    RetrievalError::new("retrieval_row_invalid", error.to_string())
}

fn storage(error: rusqlite::Error) -> RetrievalError {
    RetrievalError::new("retrieval_storage_failed", error.to_string())
}
