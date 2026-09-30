use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

mod memory;
pub use memory::MemoryVectorStore;

pub type VectorStoreFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, VectorStoreError>> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorRecord {
    pub chunk_id: Uuid,
    pub knowledge_base_id: Uuid,
    pub document_id: Uuid,
    pub model_id: String,
    pub vector: Vec<f32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VectorScope {
    pub knowledge_base_id: Option<Uuid>,
    pub document_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorSearchRequest {
    pub scope: VectorScope,
    pub query: Vec<f32>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorSearchHit {
    pub record: VectorRecord,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorStoreError {
    code: &'static str,
    message: String,
}

impl VectorStoreError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for VectorStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for VectorStoreError {}

/// Storage contract shared by pgvector and test/local implementations.
pub trait VectorStore: Send + Sync {
    fn upsert(&self, records: Vec<VectorRecord>) -> VectorStoreFuture<usize>;
    fn delete(&self, chunk_id: Uuid) -> VectorStoreFuture<bool>;
    fn search(&self, request: VectorSearchRequest) -> VectorStoreFuture<Vec<VectorSearchHit>>;
    fn get(&self, chunk_id: Uuid) -> VectorStoreFuture<Option<VectorRecord>>;
    fn count(&self, scope: VectorScope) -> VectorStoreFuture<usize>;
}

#[derive(Clone)]
pub struct PostgresVectorStore {
    pool: PgPool,
}

impl PostgresVectorStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn encode(vector: &[f32]) -> Result<String, VectorStoreError> {
        if vector.is_empty() || vector.iter().any(|value| !value.is_finite()) {
            return Err(VectorStoreError::new(
                "vector_invalid",
                "vector must be non-empty and finite",
            ));
        }
        Ok(format!(
            "[{}]",
            vector
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ))
    }

    fn decode(row: &sqlx::postgres::PgRow) -> Result<VectorRecord, VectorStoreError> {
        let vector_text: String = row
            .try_get("embedding")
            .map_err(|error| VectorStoreError::new("vector_decode_failed", error.to_string()))?;
        let vector = vector_text
            .trim_matches(['[', ']'])
            .split(',')
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                value.trim().parse::<f32>().map_err(|error| {
                    VectorStoreError::new("vector_decode_failed", error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if vector.is_empty() || vector.iter().any(|value| !value.is_finite()) {
            return Err(VectorStoreError::new(
                "vector_decode_failed",
                "stored vector is invalid",
            ));
        }
        Ok(VectorRecord {
            chunk_id: row.try_get("chunk_id").map_err(|error| {
                VectorStoreError::new("vector_decode_failed", error.to_string())
            })?,
            knowledge_base_id: row.try_get("knowledge_base_id").map_err(|error| {
                VectorStoreError::new("vector_decode_failed", error.to_string())
            })?,
            document_id: row.try_get("document_id").map_err(|error| {
                VectorStoreError::new("vector_decode_failed", error.to_string())
            })?,
            model_id: row.try_get("model_id").map_err(|error| {
                VectorStoreError::new("vector_decode_failed", error.to_string())
            })?,
            vector,
            metadata: row
                .try_get("metadata")
                .unwrap_or_else(|_| Value::Object(Default::default())),
        })
    }
}

impl VectorStore for PostgresVectorStore {
    fn upsert(&self, records: Vec<VectorRecord>) -> VectorStoreFuture<usize> {
        let pool = self.pool.clone();
        Box::pin(async move {
            if records.is_empty() {
                return Ok(0);
            }
            let mut transaction = pool.begin().await.map_err(|error| {
                VectorStoreError::new("vector_store_unavailable", error.to_string())
            })?;
            let mut dimension = None;
            for record in &records {
                let encoded = Self::encode(&record.vector)?;
                if let Some(expected) = dimension {
                    if expected != record.vector.len() {
                        return Err(VectorStoreError::new(
                            "vector_dimension_mismatch",
                            "batch dimensions differ",
                        ));
                    }
                } else {
                    dimension = Some(record.vector.len());
                }
                sqlx::query(
                    "INSERT INTO chunk_embeddings
                       (chunk_id, knowledge_base_id, document_id, model_id, dimension, embedding, metadata)
                     VALUES ($1, $2, $3, $4, $5, $6::vector, $7)
                     ON CONFLICT (chunk_id) DO UPDATE SET
                       knowledge_base_id = EXCLUDED.knowledge_base_id,
                       document_id = EXCLUDED.document_id,
                       model_id = EXCLUDED.model_id,
                       dimension = EXCLUDED.dimension,
                       embedding = EXCLUDED.embedding,
                       metadata = EXCLUDED.metadata",
                )
                .bind(record.chunk_id)
                .bind(record.knowledge_base_id)
                .bind(record.document_id)
                .bind(&record.model_id)
                .bind(i32::try_from(record.vector.len()).map_err(|_| VectorStoreError::new("vector_dimension_invalid", "vector dimension is too large"))?)
                .bind(encoded)
                .bind(&record.metadata)
                .execute(&mut *transaction)
                .await
                .map_err(|error| VectorStoreError::new("vector_store_write_failed", error.to_string()))?;
            }
            transaction.commit().await.map_err(|error| {
                VectorStoreError::new("vector_store_write_failed", error.to_string())
            })?;
            Ok(records.len())
        })
    }

    fn delete(&self, chunk_id: Uuid) -> VectorStoreFuture<bool> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result = sqlx::query("DELETE FROM chunk_embeddings WHERE chunk_id = $1")
                .bind(chunk_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    VectorStoreError::new("vector_store_delete_failed", error.to_string())
                })?;
            Ok(result.rows_affected() > 0)
        })
    }

    fn search(&self, request: VectorSearchRequest) -> VectorStoreFuture<Vec<VectorSearchHit>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let encoded = Self::encode(&request.query)?;
            let limit = request.limit.min(500);
            if limit == 0 {
                return Ok(Vec::new());
            }
            let rows = sqlx::query(
                "SELECT chunk_id, knowledge_base_id, document_id, model_id, embedding::text AS embedding, metadata,
                        (1 - (embedding <=> $3::vector))::real AS score
                 FROM chunk_embeddings
                 WHERE ($1::uuid IS NULL OR knowledge_base_id = $1)
                   AND ($2::uuid IS NULL OR document_id = $2)
                   AND dimension = $4
                 ORDER BY embedding <=> $3::vector, chunk_id
                 LIMIT $5",
            )
            .bind(request.scope.knowledge_base_id)
            .bind(request.scope.document_id)
            .bind(encoded)
            .bind(i32::try_from(request.query.len()).map_err(|_| VectorStoreError::new("vector_dimension_invalid", "vector dimension is too large"))?)
            .bind(i64::try_from(limit).map_err(|_| VectorStoreError::new("vector_limit_invalid", "vector limit is invalid"))?)
            .fetch_all(&pool)
            .await
            .map_err(|error| VectorStoreError::new("vector_store_search_failed", error.to_string()))?;
            rows.iter()
                .map(|row| {
                    Ok(VectorSearchHit {
                        record: Self::decode(row)?,
                        score: row.try_get("score").map_err(|error| {
                            VectorStoreError::new("vector_decode_failed", error.to_string())
                        })?,
                    })
                })
                .collect()
        })
    }

    fn get(&self, chunk_id: Uuid) -> VectorStoreFuture<Option<VectorRecord>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT chunk_id, knowledge_base_id, document_id, model_id, embedding::text AS embedding, metadata
                 FROM chunk_embeddings WHERE chunk_id = $1",
            )
            .bind(chunk_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| VectorStoreError::new("vector_store_read_failed", error.to_string()))?;
            row.as_ref().map(Self::decode).transpose()
        })
    }

    fn count(&self, scope: VectorScope) -> VectorStoreFuture<usize> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM chunk_embeddings
                 WHERE ($1::uuid IS NULL OR knowledge_base_id = $1)
                   AND ($2::uuid IS NULL OR document_id = $2)",
            )
            .bind(scope.knowledge_base_id)
            .bind(scope.document_id)
            .fetch_one(&pool)
            .await
            .map_err(|error| {
                VectorStoreError::new("vector_store_read_failed", error.to_string())
            })?;
            usize::try_from(count).map_err(|_| {
                VectorStoreError::new("vector_count_invalid", "vector count is invalid")
            })
        })
    }
}

#[cfg(test)]
mod tests;
