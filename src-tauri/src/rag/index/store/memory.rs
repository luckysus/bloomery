use super::*;

#[derive(Clone, Default)]
pub struct MemoryVectorStore {
    records: Arc<Mutex<HashMap<Uuid, VectorRecord>>>,
}

impl MemoryVectorStore {
    fn validate_vector(vector: &[f32]) -> Result<usize, VectorStoreError> {
        if vector.is_empty() || vector.iter().any(|value| !value.is_finite()) {
            return Err(VectorStoreError::new(
                "vector_invalid",
                "vector must be non-empty and finite",
            ));
        }
        Ok(vector.len())
    }

    fn matches(scope: &VectorScope, record: &VectorRecord) -> bool {
        scope
            .knowledge_base_id
            .is_none_or(|id| id == record.knowledge_base_id)
            && scope.document_id.is_none_or(|id| id == record.document_id)
    }
}

impl VectorStore for MemoryVectorStore {
    fn upsert(&self, records: Vec<VectorRecord>) -> VectorStoreFuture<usize> {
        let store = Arc::clone(&self.records);
        Box::pin(async move {
            let mut guard = store.lock().map_err(|_| {
                VectorStoreError::new("vector_store_poisoned", "vector store is unavailable")
            })?;
            let mut dimension = None;
            for record in &records {
                let current = Self::validate_vector(&record.vector)?;
                if let Some(expected) = dimension {
                    if expected != current {
                        return Err(VectorStoreError::new(
                            "vector_dimension_mismatch",
                            "all vectors in a batch must have the same dimension",
                        ));
                    }
                } else {
                    dimension = Some(current);
                }
                if record.model_id.trim().is_empty() {
                    return Err(VectorStoreError::new(
                        "vector_model_missing",
                        "model ID is required",
                    ));
                }
            }
            let count = records.len();
            for record in records {
                guard.insert(record.chunk_id, record);
            }
            Ok(count)
        })
    }

    fn delete(&self, chunk_id: Uuid) -> VectorStoreFuture<bool> {
        let store = Arc::clone(&self.records);
        Box::pin(async move {
            let mut guard = store.lock().map_err(|_| {
                VectorStoreError::new("vector_store_poisoned", "vector store is unavailable")
            })?;
            Ok(guard.remove(&chunk_id).is_some())
        })
    }

    fn search(&self, request: VectorSearchRequest) -> VectorStoreFuture<Vec<VectorSearchHit>> {
        let store = Arc::clone(&self.records);
        Box::pin(async move {
            let query_dimension = Self::validate_vector(&request.query)?;
            let limit = request.limit.min(500);
            if limit == 0 {
                return Ok(Vec::new());
            }
            let guard = store.lock().map_err(|_| {
                VectorStoreError::new("vector_store_poisoned", "vector store is unavailable")
            })?;
            let mut hits = guard
                .values()
                .filter(|record| Self::matches(&request.scope, record))
                .filter(|record| record.vector.len() == query_dimension)
                .map(|record| VectorSearchHit {
                    record: record.clone(),
                    score: cosine_similarity(&request.query, &record.vector),
                })
                .collect::<Vec<_>>();
            hits.sort_by(|left, right| {
                right
                    .score
                    .total_cmp(&left.score)
                    .then_with(|| left.record.chunk_id.cmp(&right.record.chunk_id))
            });
            hits.truncate(limit);
            Ok(hits)
        })
    }

    fn get(&self, chunk_id: Uuid) -> VectorStoreFuture<Option<VectorRecord>> {
        let store = Arc::clone(&self.records);
        Box::pin(async move {
            let guard = store.lock().map_err(|_| {
                VectorStoreError::new("vector_store_poisoned", "vector store is unavailable")
            })?;
            Ok(guard.get(&chunk_id).cloned())
        })
    }

    fn count(&self, scope: VectorScope) -> VectorStoreFuture<usize> {
        let store = Arc::clone(&self.records);
        Box::pin(async move {
            let guard = store.lock().map_err(|_| {
                VectorStoreError::new("vector_store_poisoned", "vector store is unavailable")
            })?;
            Ok(guard
                .values()
                .filter(|record| Self::matches(&scope, record))
                .count())
        })
    }
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let dot = left.iter().zip(right).map(|(a, b)| a * b).sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}
