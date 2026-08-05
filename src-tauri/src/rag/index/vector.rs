use crate::rag::model::{ChunkId, DocumentVersionId};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

const MAX_RESULTS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexWatermark {
    pub format_version: u32,
    pub workspace_id: String,
    pub provider_profile_id: String,
    pub model_id: String,
    pub dimension: u32,
    pub chunk_count: u32,
    pub sqlite_watermark: String,
}

impl IndexWatermark {
    pub(crate) fn validate(&self) -> Result<(), IndexError> {
        if self.format_version != 1
            || self.workspace_id.trim() != self.workspace_id
            || self.workspace_id.is_empty()
            || uuid::Uuid::parse_str(&self.provider_profile_id).is_err()
            || self.model_id.trim() != self.model_id
            || self.model_id.is_empty()
            || self.dimension == 0
            || self.sqlite_watermark.trim() != self.sqlite_watermark
            || self.sqlite_watermark.is_empty()
        {
            return Err(IndexError::new(
                "index_watermark_invalid",
                "vector index watermark is invalid",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct CandidateFilter {
    version_ids: HashSet<DocumentVersionId>,
}

impl CandidateFilter {
    pub fn new(version_ids: Vec<DocumentVersionId>) -> Self {
        Self {
            version_ids: version_ids.into_iter().collect(),
        }
    }

    pub(crate) fn allows(&self, version_id: DocumentVersionId) -> bool {
        self.version_ids.contains(&version_id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorHit {
    pub version_id: DocumentVersionId,
    pub chunk_id: ChunkId,
    pub distance: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexError {
    code: &'static str,
    message: String,
}

impl IndexError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for IndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for IndexError {}

pub trait VectorIndex: Send + Sync {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &CandidateFilter,
    ) -> Result<Vec<VectorHit>, IndexError>;

    fn watermark(&self) -> &IndexWatermark;
}

#[derive(Debug)]
pub struct FlatVectorIndex {
    watermark: IndexWatermark,
    records: Vec<FlatRecord>,
}

#[derive(Debug)]
struct FlatRecord {
    version_id: DocumentVersionId,
    chunk_id: ChunkId,
    vector: Vec<f32>,
}

impl FlatVectorIndex {
    pub(crate) fn load(
        connection: &Connection,
        watermark: &IndexWatermark,
    ) -> Result<Self, IndexError> {
        watermark.validate()?;
        let mut statement = connection
            .prepare(
                "SELECT embeddings.version_id, embeddings.chunk_id, vectors.vector_blob,
                        vectors.vector_sha256
                 FROM knowledge_chunk_embeddings AS embeddings
                 JOIN knowledge_vectors AS vectors
                   ON vectors.workspace_id = embeddings.workspace_id
                  AND vectors.id = embeddings.vector_key
                 WHERE embeddings.workspace_id = ?1
                   AND embeddings.provider_profile_id = ?2
                   AND embeddings.model_id = ?3 AND embeddings.dimension = ?4
                 ORDER BY embeddings.version_id, embeddings.chunk_id",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(
                params![
                    watermark.workspace_id,
                    watermark.provider_profile_id,
                    watermark.model_id,
                    watermark.dimension
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .map_err(storage)?;
        let mut records = Vec::new();
        for row in rows {
            let (version, chunk, blob, expected_sha256) = row.map_err(storage)?;
            if format!("{:x}", Sha256::digest(&blob)) != expected_sha256 {
                return Err(IndexError::new(
                    "index_checksum_mismatch",
                    "SQLite vector checksum does not match",
                ));
            }
            records.push(FlatRecord {
                version_id: DocumentVersionId::from_str(&version)
                    .map_err(|error| invalid_row("version ID", error))?,
                chunk_id: ChunkId::new(chunk).map_err(|error| invalid_row("chunk ID", error))?,
                vector: decode_vector(&blob, watermark.dimension)?,
            });
        }
        if records.len() != watermark.chunk_count as usize {
            return Err(IndexError::new(
                "index_watermark_mismatch",
                "SQLite vector count does not match the watermark",
            ));
        }
        Ok(Self {
            watermark: watermark.clone(),
            records,
        })
    }
}

impl VectorIndex for FlatVectorIndex {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &CandidateFilter,
    ) -> Result<Vec<VectorHit>, IndexError> {
        validate_query(query, self.watermark.dimension)?;
        let limit = limit.min(MAX_RESULTS);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut hits = self
            .records
            .iter()
            .filter(|record| filter.allows(record.version_id))
            .map(|record| VectorHit {
                version_id: record.version_id,
                chunk_id: record.chunk_id.clone(),
                distance: cosine_distance(query, &record.vector),
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            left.distance
                .total_cmp(&right.distance)
                .then_with(|| left.chunk_id.as_str().cmp(right.chunk_id.as_str()))
                .then_with(|| {
                    left.version_id
                        .to_string()
                        .cmp(&right.version_id.to_string())
                })
        });
        hits.truncate(limit);
        Ok(hits)
    }

    fn watermark(&self) -> &IndexWatermark {
        &self.watermark
    }
}

pub(crate) fn validate_query(query: &[f32], dimension: u32) -> Result<(), IndexError> {
    if query.len() != dimension as usize || query.iter().any(|value| !value.is_finite()) {
        return Err(IndexError::new(
            "index_query_invalid",
            "query vector has an invalid dimension or value",
        ));
    }
    Ok(())
}

fn decode_vector(blob: &[u8], dimension: u32) -> Result<Vec<f32>, IndexError> {
    if blob.len() != dimension as usize * 4 {
        return Err(IndexError::new(
            "index_vector_invalid",
            "stored vector has an invalid dimension",
        ));
    }
    let vector = blob
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte vector value")))
        .collect::<Vec<_>>();
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(IndexError::new(
            "index_vector_invalid",
            "stored vector contains a non-finite value",
        ));
    }
    Ok(vector)
}

fn cosine_distance(left: &[f32], right: &[f32]) -> f32 {
    let dot = left.iter().zip(right).map(|(a, b)| a * b).sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        1.0
    } else {
        1.0 - dot / (left_norm * right_norm)
    }
}

fn invalid_row(name: &str, error: impl fmt::Display) -> IndexError {
    IndexError::new("index_row_invalid", format!("invalid {name}: {error}"))
}

fn storage(error: rusqlite::Error) -> IndexError {
    IndexError::new("index_storage_failed", error.to_string())
}
