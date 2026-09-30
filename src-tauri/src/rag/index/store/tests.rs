use super::*;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn memory_store_supports_scoped_upsert_search_get_delete_and_count() {
    let store = MemoryVectorStore::default();
    let knowledge_base_id = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    let chunk_id = Uuid::new_v4();
    let inserted = store
        .upsert(vec![VectorRecord {
            chunk_id,
            knowledge_base_id,
            document_id,
            model_id: "test-embedding".to_string(),
            vector: vec![1.0, 0.0],
            metadata: json!({"page": 2}),
        }])
        .await
        .unwrap();
    assert_eq!(inserted, 1);
    assert_eq!(
        store
            .count(VectorScope {
                knowledge_base_id: Some(knowledge_base_id),
                document_id: None,
            })
            .await
            .unwrap(),
        1
    );
    let hits = store
        .search(VectorSearchRequest {
            scope: VectorScope {
                knowledge_base_id: Some(knowledge_base_id),
                document_id: Some(document_id),
            },
            query: vec![1.0, 0.0],
            limit: 5,
        })
        .await
        .unwrap();
    assert_eq!(hits[0].record.chunk_id, chunk_id);
    assert!(store.get(chunk_id).await.unwrap().is_some());
    assert!(store.delete(chunk_id).await.unwrap());
    assert_eq!(store.count(VectorScope::default()).await.unwrap(), 0);
}

#[tokio::test]
async fn memory_store_rejects_mixed_dimensions() {
    let store = MemoryVectorStore::default();
    let base = Uuid::new_v4();
    let record = |vector| VectorRecord {
        chunk_id: Uuid::new_v4(),
        knowledge_base_id: base,
        document_id: Uuid::new_v4(),
        model_id: "test".to_string(),
        vector,
        metadata: json!({}),
    };
    assert!(store
        .upsert(vec![record(vec![1.0]), record(vec![1.0, 2.0])])
        .await
        .is_err());
}
