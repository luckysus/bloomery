use super::*;

#[test]
fn local_workspace_is_stable() {
    assert_eq!(current_workspace_id(), "local");
}

#[test]
fn configured_data_directory_prefers_absolute_override() {
    let default = std::env::temp_dir().join("bloomery-default-data");
    let override_path = std::env::temp_dir().join("bloomery-override-data");

    let resolved = configured_data_directory(default, Some(override_path.clone()))
        .expect("absolute data directory override should be accepted");

    assert_eq!(resolved, override_path);
}

#[test]
fn configured_data_directory_rejects_relative_override() {
    let result = configured_data_directory(
        std::env::temp_dir().join("bloomery-default-data"),
        Some(PathBuf::from("relative-data")),
    );

    assert!(result.is_err());
}

#[test]
fn production_scheduler_registers_mineru_ingest_handler() {
    let root = std::env::temp_dir().join(format!("bloomery-handlers-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create handler root");
    let handlers =
        rag_task_handlers_with_compute(root.join("bloomery.sqlite3"), root.clone(), None);

    assert_eq!(handlers.len(), 10);
    assert!(handlers.iter().any(|handler| {
        handler.kind() == crate::compute::handler::COMPUTE_TRAIN_LINEAR_REGRESSION_KIND
    }));
    assert!(handlers.iter().any(|handler| {
        handler.kind() == crate::compute::handler::COMPUTE_PREDICT_LINEAR_REGRESSION_KIND
    }));
    assert!(handlers
        .iter()
        .any(|handler| handler.kind() == crate::compute::handler::COMPUTE_PREDICT_ONNX_KIND));
    assert!(handlers.iter().any(|handler| {
        handler.kind() == crate::compute::handler::COMPUTE_OPTIMIZE_CONSTRAINED_KIND
    }));
    assert!(handlers
        .iter()
        .any(|handler| handler.kind() == crate::compute::handler::COMPUTE_EXPORT_ONNX_KIND));
    assert!(handlers.iter().any(|handler| {
        handler.kind() == crate::compute::handler::COMPUTE_PREDICT_TRAINED_KIND
    }));
    assert!(handlers
        .iter()
        .any(|handler| handler.kind() == crate::compute::handler::COMPUTE_TRAIN_SKLEARN_KIND));
    assert!(handlers
        .iter()
        .any(|handler| handler.kind() == crate::rag::tasks::MINERU_TASK_KIND));
    assert!(handlers
        .iter()
        .any(|handler| handler.kind() == crate::rag::index::rebuild::INDEX_REBUILD_KIND));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn python_worker_fallback_uses_process_tree_isolation() {
    let config = python_worker_config(PathBuf::from("python.exe"), PathBuf::from("compute-worker"));

    assert!(
        config.isolate_process_tree,
        "development Python workers must use the same process-tree guard as packaged workers"
    );
}
