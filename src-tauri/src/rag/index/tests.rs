use super::*;

#[test]
fn sqlite_busy_storage_errors_remain_retryable() {
    let busy = storage("database is locked".to_string());
    assert_eq!(busy.code(), "embedding_storage");
    assert!(busy.retryable());
    assert!(!storage("invalid embedding row".to_string()).retryable());
}
