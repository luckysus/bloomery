use bloomery::agent::protocol::PermissionRisk;
use bloomery::agent::runtime::CancellationToken;
use bloomery::tools::{
    ArtifactStore, ConcurrencyPolicy, FileArtifactStore, RegistryError, ToolDefinition, ToolError,
    ToolExecutor, ToolHandler, ToolId, ToolRegistration, ToolRegistry, ToolSource, ToolVersion,
    MAX_INLINE_OUTPUT_BYTES,
};
use futures_util::future::join_all;
use serde_json::json;
use std::collections::BTreeSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

fn tool(id: &str, source: ToolSource) -> ToolDefinition {
    ToolDefinition {
        id: ToolId::new(id).expect("test tool id"),
        version: ToolVersion::parse("1.2.3").expect("test tool version"),
        name: id.rsplit('.').next().unwrap_or(id).to_string(),
        description: format!("Test tool {id}"),
        input_schema: json!({
            "type": "object",
            "properties": {"value": {"type": "string"}},
            "additionalProperties": false
        }),
        output_schema: json!({"type": "object"}),
        risk: PermissionRisk::Automatic,
        read_only: true,
        concurrency: ConcurrencyPolicy::ParallelRead,
        timeout: Duration::from_secs(10),
        source,
        domains: BTreeSet::new(),
    }
}

struct FnHandler<F>(F);

impl<F, Fut> ToolHandler for FnHandler<F>
where
    F: Fn(serde_json::Value, CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<serde_json::Value, ToolError>> + Send + 'static,
{
    fn execute(
        &self,
        arguments: serde_json::Value,
        cancellation: CancellationToken,
    ) -> bloomery::tools::HandlerFuture {
        Box::pin((self.0)(arguments, cancellation))
    }
}

fn registration<F, Fut>(definition: ToolDefinition, handler: F) -> ToolRegistration
where
    F: Fn(serde_json::Value, CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<serde_json::Value, ToolError>> + Send + 'static,
{
    ToolRegistration::new(definition, Arc::new(FnHandler(handler)))
}

fn executor(registrations: Vec<ToolRegistration>) -> (ToolExecutor, PathBuf) {
    let root = std::env::temp_dir().join(format!("bloomery-tools-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(FileArtifactStore::new(root.clone()).unwrap());
    (
        ToolExecutor::new(registrations, store as Arc<dyn ArtifactStore>).unwrap(),
        root,
    )
}

fn never_cancelled() -> CancellationToken {
    CancellationToken::new(|| false)
}

#[test]
fn tool_ids_and_versions_are_stable_and_strictly_parsed() {
    let id = ToolId::new("builtin.read_file").unwrap();
    let version = ToolVersion::parse("1.2.3").unwrap();

    assert_eq!(id.as_str(), "builtin.read_file");
    assert_eq!(version.to_string(), "1.2.3");
    assert!(ToolId::new("Read File").is_err());
    assert!(ToolId::new("builtin..read").is_err());
    assert!(ToolVersion::parse("1.2").is_err());
    assert!(ToolVersion::parse("v1.2.3").is_err());
}

#[test]
fn registry_rejects_duplicate_stable_ids() {
    let mut registry = ToolRegistry::new();
    registry
        .register(tool("builtin.one", ToolSource::Builtin))
        .unwrap();

    let error = registry
        .register(tool("builtin.one", ToolSource::Builtin))
        .unwrap_err();

    assert!(matches!(error, RegistryError::DuplicateId { .. }));
}

#[test]
fn registry_rejects_invalid_input_and_output_schemas() {
    let mut invalid_input = tool("builtin.invalid_input", ToolSource::Builtin);
    invalid_input.input_schema = json!([]);
    let error = ToolRegistry::new().register(invalid_input).unwrap_err();
    assert!(matches!(error, RegistryError::InvalidSchema { field, .. } if field == "input_schema"));

    let mut invalid_output = tool("builtin.invalid_output", ToolSource::Builtin);
    invalid_output.output_schema = json!({"type": "unsupported"});
    let error = ToolRegistry::new().register(invalid_output).unwrap_err();
    assert!(
        matches!(error, RegistryError::InvalidSchema { field, .. } if field == "output_schema")
    );
}

#[test]
fn disabled_tools_are_absent_until_reenabled() {
    let id = ToolId::new("builtin.search").unwrap();
    let mut registry = ToolRegistry::new();
    registry
        .register(tool(id.as_str(), ToolSource::Builtin))
        .unwrap();

    registry.set_enabled(&id, false).unwrap();
    assert!(registry.snapshot().tools.is_empty());
    assert!(!registry.is_enabled(&id));

    registry.set_enabled(&id, true).unwrap();
    assert_eq!(registry.snapshot().tools.len(), 1);
    assert!(registry.is_enabled(&id));
}

#[test]
fn snapshots_filter_domain_tools_but_keep_global_tools() {
    let mut global = tool("builtin.clock", ToolSource::Builtin);
    let mut steel = tool(
        "domain.steel.grade_lookup",
        ToolSource::Domain {
            package_id: "steel".to_string(),
            package_version: ToolVersion::parse("1.0.0").unwrap(),
        },
    );
    steel.domains.insert("steel".to_string());
    let mut chemistry = tool(
        "domain.chemistry.lookup",
        ToolSource::Domain {
            package_id: "chemistry".to_string(),
            package_version: ToolVersion::parse("1.0.0").unwrap(),
        },
    );
    chemistry.domains.insert("chemistry".to_string());
    global.domains.clear();

    let mut registry = ToolRegistry::new();
    registry.register(global).unwrap();
    registry.register(steel).unwrap();
    registry.register(chemistry).unwrap();

    let steel_tools = registry.snapshot_for_domain(Some("steel"));
    assert_eq!(
        steel_tools
            .tools
            .iter()
            .map(|tool| tool.id.as_str())
            .collect::<Vec<_>>(),
        vec!["builtin.clock", "domain.steel.grade_lookup"]
    );
    assert_eq!(registry.snapshot_for_domain(None).tools.len(), 1);
}

#[test]
fn snapshots_preserve_source_attribution() {
    let source = ToolSource::Mcp {
        server_id: "steel-tools".to_string(),
        server_version: ToolVersion::parse("2.0.0").unwrap(),
    };
    let mut registry = ToolRegistry::new();
    registry
        .register(tool("mcp.steel.lookup", source.clone()))
        .unwrap();

    assert_eq!(registry.snapshot().tools[0].source, source);
}

#[test]
fn snapshots_have_deterministic_id_then_version_order() {
    let mut registry = ToolRegistry::new();
    registry
        .register(tool("builtin.zeta", ToolSource::Builtin))
        .unwrap();
    registry
        .register(tool("builtin.alpha", ToolSource::Builtin))
        .unwrap();

    let ids = registry
        .snapshot()
        .tools
        .into_iter()
        .map(|tool| tool.id.to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["builtin.alpha", "builtin.zeta"]);
}

#[test]
fn tool_executor_returns_structured_handler_errors() {
    let definition = tool("builtin.invalid", ToolSource::Builtin);
    let id = definition.id.clone();
    let (executor, root) = executor(vec![registration(definition, |_, _| async {
        Err(ToolError::with_details(
            "invalid_input",
            "value is not supported",
            json!({"path": "$.value"}),
        ))
    })]);

    let error = tauri::async_runtime::block_on(executor.execute(&id, json!({}), never_cancelled()))
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
    assert_eq!(error.details, Some(json!({"path": "$.value"})));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tool_executor_converts_handler_timeout_to_a_stable_error() {
    let mut definition = tool("builtin.slow", ToolSource::Builtin);
    definition.timeout = Duration::from_millis(10);
    let id = definition.id.clone();
    let (executor, root) = executor(vec![registration(definition, |_, _| async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(json!({"done": true}))
    })]);

    let error = tauri::async_runtime::block_on(executor.execute(&id, json!({}), never_cancelled()))
        .unwrap_err();
    assert_eq!(error.code, "tool_timeout");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tool_executor_stops_a_running_handler_when_cancelled() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        signal.store(true, Ordering::SeqCst);
    });
    let definition = tool("builtin.wait", ToolSource::Builtin);
    let id = definition.id.clone();
    let (executor, root) = executor(vec![registration(definition, |_, _| async {
        loop {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })]);
    let token = CancellationToken::new(move || cancelled.load(Ordering::SeqCst));

    let error =
        tauri::async_runtime::block_on(executor.execute(&id, json!({}), token)).unwrap_err();
    assert_eq!(error.code, "cancelled");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn oversized_outputs_are_saved_as_artifacts_with_a_bounded_model_value() {
    let definition = tool("builtin.large", ToolSource::Builtin);
    let id = definition.id.clone();
    let payload = "x".repeat(MAX_INLINE_OUTPUT_BYTES);
    let expected = json!({"payload": payload});
    let handler_output = expected.clone();
    let (executor, root) = executor(vec![registration(definition, move |_, _| {
        let expected = handler_output.clone();
        async move { Ok(expected) }
    })]);

    let output =
        tauri::async_runtime::block_on(executor.execute(&id, json!({}), never_cancelled()))
            .unwrap();
    let artifact = output.artifact.expect("large output artifact");
    assert_eq!(
        artifact.size_bytes,
        serde_json::to_vec(&expected).unwrap().len()
    );
    assert_eq!(output.model_output["truncated"], true);
    assert!(artifact.path.exists());
    assert_eq!(
        std::fs::read(&artifact.path).unwrap(),
        serde_json::to_vec(&expected).unwrap()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn independent_read_tools_run_in_parallel() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let mut first = tool("builtin.read_a", ToolSource::Builtin);
    let mut second = tool("builtin.read_b", ToolSource::Builtin);
    first.timeout = Duration::from_secs(1);
    second.timeout = Duration::from_secs(1);
    let first_id = first.id.clone();
    let second_id = second.id.clone();
    let active_a = active.clone();
    let maximum_a = maximum.clone();
    let active_b = active.clone();
    let maximum_b = maximum.clone();
    let (executor, root) = executor(vec![
        registration(first, move |_, _| {
            let active = active_a.clone();
            let maximum = maximum_a.clone();
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(json!({"ok": true}))
            }
        }),
        registration(second, move |_, _| {
            let active = active_b.clone();
            let maximum = maximum_b.clone();
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(json!({"ok": true}))
            }
        }),
    ]);

    let results = tauri::async_runtime::block_on(join_all(vec![
        executor.execute(&first_id, json!({}), never_cancelled()),
        executor.execute(&second_id, json!({}), never_cancelled()),
    ]));
    assert!(results.into_iter().all(|result| result.is_ok()));
    assert!(maximum.load(Ordering::SeqCst) >= 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn write_tools_are_serialized() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let mut first = tool("builtin.write_a", ToolSource::Builtin);
    let mut second = tool("builtin.write_b", ToolSource::Builtin);
    first.read_only = false;
    second.read_only = false;
    first.concurrency = ConcurrencyPolicy::SerialWrite;
    second.concurrency = ConcurrencyPolicy::SerialWrite;
    let first_id = first.id.clone();
    let second_id = second.id.clone();
    let active_a = active.clone();
    let maximum_a = maximum.clone();
    let active_b = active.clone();
    let maximum_b = maximum.clone();
    let (executor, root) = executor(vec![
        registration(first, move |_, _| {
            let active = active_a.clone();
            let maximum = maximum_a.clone();
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(json!({"ok": true}))
            }
        }),
        registration(second, move |_, _| {
            let active = active_b.clone();
            let maximum = maximum_b.clone();
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(json!({"ok": true}))
            }
        }),
    ]);

    let results = tauri::async_runtime::block_on(join_all(vec![
        executor.execute(&first_id, json!({}), never_cancelled()),
        executor.execute(&second_id, json!({}), never_cancelled()),
    ]));
    assert!(results.into_iter().all(|result| result.is_ok()));
    assert_eq!(maximum.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn panicking_handlers_become_structured_errors() {
    let definition = tool("builtin.panics", ToolSource::Builtin);
    let id = definition.id.clone();
    let (executor, root) = executor(vec![registration(definition, |_, _| async {
        panic!("handler failure")
    })]);

    let error = tauri::async_runtime::block_on(executor.execute(&id, json!({}), never_cancelled()))
        .unwrap_err();
    assert_eq!(error.code, "tool_panicked");
    let _ = std::fs::remove_dir_all(root);
}
