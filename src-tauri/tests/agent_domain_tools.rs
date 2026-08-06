use bloomery::agent::protocol::PermissionRisk;
use bloomery::agent::runtime::{
    CancellationToken, DomainToolExecutor, ToolExecutionError, ToolExecutor, ToolFuture,
    ToolHandler, ToolInvocation, ToolRegistration,
};
use bloomery::agent::tool_repair::ToolSpec;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

struct TestTools {
    registrations: Vec<ToolRegistration>,
}

impl ToolExecutor for TestTools {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        self.registrations
            .iter()
            .find(|registration| registration.spec.id == invocation.tool_id)
            .map(|registration| {
                registration
                    .handler
                    .execute(invocation.arguments, cancellation)
            })
            .unwrap_or_else(|| {
                Box::pin(async {
                    Err(ToolExecutionError::new(
                        "tool_not_registered",
                        "test tool is not registered",
                    ))
                })
            })
    }
}

struct StaticHandler;

impl ToolHandler for StaticHandler {
    fn execute(&self, _arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
        Box::pin(async { Ok(json!({"ok": true})) })
    }
}

fn tool(id: &str, name: &str) -> ToolRegistration {
    ToolRegistration::new(
        ToolSpec {
            id: id.to_string(),
            name: name.to_string(),
            input_schema: json!({"type": "object"}),
            risk: PermissionRisk::Automatic,
        },
        true,
        Arc::new(StaticHandler),
    )
}

fn steel_manifest() -> bloomery::domains::DomainManifest {
    serde_json::from_value(json!({
        "id": "steel",
        "version": "1.0.0",
        "compatibility": {"min_app_version": "0.1.0", "max_app_version": null},
        "author": "Bloomery contributors",
        "license": "Apache-2.0",
        "prompts": {"system": "Use steel terminology.", "workflow": "Cite sources."},
        "retrieval": {"required_tags": [], "citation_required": true, "max_evidence_items": 12},
        "builtin_tool_allowlist": ["knowledge.query"]
    }))
    .expect("valid domain manifest")
}

#[test]
fn domain_executor_exposes_only_allowlisted_tools() {
    let inner = TestTools {
        registrations: vec![
            tool("knowledge.query", "knowledge_query"),
            tool("file.write", "file_write"),
        ],
    };
    let scoped = DomainToolExecutor::new(&inner, Some(&steel_manifest()));

    assert_eq!(scoped.registrations().len(), 1);
    assert_eq!(scoped.registrations()[0].spec.id, "knowledge.query");
}

#[tokio::test]
async fn domain_executor_rejects_a_non_allowlisted_invocation() {
    let inner = TestTools {
        registrations: vec![tool("knowledge.query", "knowledge_query")],
    };
    let scoped = DomainToolExecutor::new(&inner, Some(&steel_manifest()));
    let error = scoped
        .execute(
            ToolInvocation {
                tool_call_id: Uuid::new_v4(),
                tool_id: "file.write".to_string(),
                tool_name: "file_write".to_string(),
                arguments: json!({}),
            },
            CancellationToken::new(|| false),
        )
        .await
        .expect_err("domain executor must reject tools outside the allowlist");

    assert_eq!(error.code, "domain_tool_not_allowed");
}
