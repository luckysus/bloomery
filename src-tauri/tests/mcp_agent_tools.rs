use bloomery::agent::protocol::PermissionRisk;
use bloomery::{
    agent::runtime::{
        CancellationToken, CompositeToolExecutor, ToolExecutor, ToolFuture, ToolInvocation,
    },
    mcp::{McpToolBinding, McpToolCaller, McpToolExecutor},
    tools::{ConcurrencyPolicy, ToolDefinition, ToolId, ToolSource, ToolVersion},
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct RecordingCaller {
    calls: Arc<Mutex<Vec<(String, Value)>>>,
}

impl McpToolCaller for RecordingCaller {
    fn call(
        &self,
        tool_name: String,
        arguments: Value,
        _cancellation: CancellationToken,
    ) -> ToolFuture {
        let calls = self.calls.clone();
        Box::pin(async move {
            calls
                .lock()
                .expect("record calls")
                .push((tool_name, arguments));
            Ok(json!({"ok": true}))
        })
    }
}

fn definition(id: &str, source: ToolSource) -> ToolDefinition {
    ToolDefinition {
        id: ToolId::new(id).expect("valid tool id"),
        version: ToolVersion {
            major: 1,
            minor: 0,
            patch: 0,
        },
        name: "lookup".to_string(),
        description: "Look up a steel standard".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {"grade": {"type": "string"}},
            "required": ["grade"],
            "additionalProperties": false
        }),
        output_schema: json!({"type": "object"}),
        risk: PermissionRisk::ConfirmationRequired,
        read_only: true,
        concurrency: ConcurrencyPolicy::ParallelRead,
        timeout: std::time::Duration::from_secs(5),
        source,
        domains: Default::default(),
    }
}

#[tokio::test]
async fn registers_valid_mcp_tools_and_forwards_calls() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let executor = McpToolExecutor::from_bindings(vec![McpToolBinding {
        definition: definition(
            "mcp.steel.lookup",
            ToolSource::Mcp {
                server_id: "steel".to_string(),
                server_version: ToolVersion {
                    major: 1,
                    minor: 0,
                    patch: 0,
                },
            },
        ),
        caller: Arc::new(RecordingCaller {
            calls: calls.clone(),
        }),
    }])
    .expect("valid MCP binding");

    assert_eq!(executor.registrations().len(), 1);
    let result = executor
        .execute(
            ToolInvocation {
                tool_call_id: Uuid::new_v4(),
                tool_id: "mcp.steel.lookup".to_string(),
                tool_name: "lookup".to_string(),
                arguments: json!({"grade": "Q355B"}),
            },
            CancellationToken::new(|| false),
        )
        .await
        .expect("MCP call succeeds");

    assert_eq!(result, json!({"ok": true}));
    assert_eq!(
        calls.lock().expect("read calls").as_slice(),
        [("lookup".to_string(), json!({"grade": "Q355B"}))]
    );
}

#[test]
fn rejects_non_mcp_sources_and_duplicate_ids() {
    let caller = Arc::new(RecordingCaller {
        calls: Arc::new(Mutex::new(Vec::new())),
    });
    let error = McpToolExecutor::from_bindings(vec![McpToolBinding {
        definition: definition("builtin.lookup", ToolSource::Builtin),
        caller: caller.clone(),
    }])
    .err()
    .expect("built-in source must not enter MCP executor");
    assert!(error.contains("MCP"));

    let mcp_definition = definition(
        "mcp.steel.lookup",
        ToolSource::Mcp {
            server_id: "steel".to_string(),
            server_version: ToolVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
        },
    );
    let error = McpToolExecutor::from_bindings(vec![
        McpToolBinding {
            definition: mcp_definition.clone(),
            caller: caller.clone(),
        },
        McpToolBinding {
            definition: mcp_definition,
            caller,
        },
    ])
    .err()
    .expect("duplicate MCP tool ids must be rejected");
    assert!(error.contains("already registered"));
}

#[tokio::test]
async fn composite_executor_keeps_builtin_and_mcp_tools_available() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mcp = McpToolExecutor::from_bindings(vec![McpToolBinding {
        definition: definition(
            "mcp.steel.lookup",
            ToolSource::Mcp {
                server_id: "steel".to_string(),
                server_version: ToolVersion {
                    major: 1,
                    minor: 0,
                    patch: 0,
                },
            },
        ),
        caller: Arc::new(RecordingCaller {
            calls: calls.clone(),
        }),
    }])
    .expect("valid MCP binding");
    let steel = bloomery::steel::SteelToolExecutor::new(true);
    let combined = CompositeToolExecutor::try_new(vec![&steel, &mcp])
        .expect("built-in and MCP tools have distinct ids");

    assert_eq!(combined.registrations().len(), 2);
    combined
        .execute(
            ToolInvocation {
                tool_call_id: Uuid::new_v4(),
                tool_id: "mcp.steel.lookup".to_string(),
                tool_name: "lookup".to_string(),
                arguments: json!({"grade": "Q355B"}),
            },
            CancellationToken::new(|| false),
        )
        .await
        .expect("MCP tool remains executable through composite");
    assert_eq!(calls.lock().expect("read calls").len(), 1);
}

#[test]
fn limits_mcp_tools_for_agent_prompt_when_many_are_enabled() {
    let caller = Arc::new(RecordingCaller {
        calls: Arc::new(Mutex::new(Vec::new())),
    });
    let mut bindings = Vec::new();
    for index in 0..12 {
        let mut item = definition(
            &format!("mcp.steel.tool_{index}"),
            ToolSource::Mcp {
                server_id: "steel".to_string(),
                server_version: ToolVersion {
                    major: 1,
                    minor: 0,
                    patch: 0,
                },
            },
        );
        item.name = if index == 9 {
            "calculator".to_string()
        } else {
            format!("tool_{index}")
        };
        item.description = if index == 9 {
            "Calculate steel process windows".to_string()
        } else {
            "Generic MCP helper".to_string()
        };
        bindings.push(McpToolBinding {
            definition: item,
            caller: caller.clone(),
        });
    }

    let executor = McpToolExecutor::from_bindings_for_query(bindings, "calculate Q355B")
        .expect("filtered MCP executor");

    assert!(executor.registrations().len() <= 8);
    assert_eq!(executor.registrations()[0].spec.name, "calculator");
}
