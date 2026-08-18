use bloomery::{
    agent::runtime::CancellationToken,
    mcp::{McpClient, McpClientConfig, McpError, McpServerIdentity},
    tools::{ToolSource, ToolVersion},
};
use rmcp::{
    model::{
        CallToolResult, ClientJsonRpcMessage, ClientRequest, ErrorData, Implementation,
        InitializeResult, JsonRpcError, ListPromptsResult, ListResourcesResult, ListToolsResult,
        Prompt, Resource, ServerCapabilities, ServerJsonRpcMessage, ServerResult, Tool,
        ToolAnnotations,
    },
    transport::{IntoTransport, Transport},
    RoleServer,
};
use serde_json::json;
use std::{borrow::Cow, sync::Arc, time::Duration};
use tokio::task::JoinHandle;

#[derive(Clone, Copy)]
enum CallBehavior {
    Success,
    Timeout,
    ProtocolError,
}

#[tokio::test]
async fn initializes_and_exposes_server_identity_and_capabilities() {
    let (client, server_task) = connected(CallBehavior::Success).await;

    assert_eq!(client.server_identity().name, "steel-fixture");
    assert_eq!(client.server_identity().version, "1.2.3");
    assert!(client.capabilities().tools);
    assert!(client.capabilities().resources);
    assert!(client.capabilities().prompts);

    client.shutdown().await.unwrap();
    server_task.abort();
}

#[tokio::test]
async fn discovers_tools_resources_and_prompts() {
    let (client, server_task) = connected(CallBehavior::Success).await;

    let tools = client.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "steel.lookup");
    let resources = client.list_resources().await.unwrap();
    assert_eq!(resources[0].uri, "steel://standards/q355");
    let prompts = client.list_prompts().await.unwrap();
    assert_eq!(prompts[0].name, "steel.answer");

    client.shutdown().await.unwrap();
    server_task.abort();
}

#[tokio::test]
async fn converts_mcp_tool_schema_to_bloomery_definition() {
    let (client, server_task) = connected(CallBehavior::Success).await;

    let definitions = client.tool_definitions().await.unwrap();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].id.as_str(), "mcp.steel-fixture.steel.lookup");
    assert_eq!(definitions[0].input_schema["type"], "object");
    assert_eq!(
        definitions[0].risk,
        bloomery::agent::protocol::PermissionRisk::Automatic
    );
    assert!(definitions[0].read_only);
    assert!(matches!(
        definitions[0].source,
        ToolSource::Mcp {
            server_id: ref id,
            server_version: ToolVersion {
                major: 1,
                minor: 2,
                patch: 3
            }
        } if id == "steel-fixture"
    ));

    client.shutdown().await.unwrap();
    server_task.abort();
}

#[tokio::test]
async fn converts_structured_call_results_without_losing_error_state() {
    let (client, server_task) = connected(CallBehavior::Success).await;

    let result = client
        .call_tool(
            "steel.lookup",
            json!({"grade": "Q355B"}),
            &CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(result.structured_content, Some(json!({"grade": "Q355B"})));
    assert!(!result.is_error);

    client.shutdown().await.unwrap();
    server_task.abort();
}

#[tokio::test]
async fn bounds_calls_by_the_configured_timeout() {
    let (client, server_task) = connected(CallBehavior::Timeout).await;

    let error = client
        .call_tool("steel.lookup", json!({}), &CancellationToken::new(|| false))
        .await
        .unwrap_err();
    assert!(matches!(error, McpError::Timeout));

    client.shutdown().await.unwrap();
    server_task.abort();
}

#[tokio::test]
async fn stops_calls_when_the_agent_cancellation_token_is_set() {
    let (client, server_task) = connected(CallBehavior::Timeout).await;
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let token = CancellationToken::new({
        let cancelled = cancelled.clone();
        move || cancelled.load(std::sync::atomic::Ordering::SeqCst)
    });

    let error = client
        .call_tool("steel.lookup", json!({}), &token)
        .await
        .unwrap_err();
    assert!(matches!(error, McpError::Cancelled));

    client.shutdown().await.unwrap();
    server_task.abort();
}

#[tokio::test]
async fn preserves_protocol_errors_as_stable_mcp_errors() {
    let (client, server_task) = connected(CallBehavior::ProtocolError).await;

    let error = client
        .call_tool("steel.lookup", json!({}), &CancellationToken::new(|| false))
        .await
        .unwrap_err();
    assert!(matches!(error, McpError::Protocol { code: -32602, .. }));

    client.shutdown().await.unwrap();
    server_task.abort();
}

#[test]
fn rejects_a_server_version_change_for_existing_tool_bindings() {
    let original = McpServerIdentity {
        name: "steel-fixture".to_string(),
        version: "1.2.3".to_string(),
    };
    let changed = McpServerIdentity {
        name: "steel-fixture".to_string(),
        version: "2.0.0".to_string(),
    };

    assert!(original.ensure_same(&original).is_ok());
    assert!(matches!(
        original.ensure_same(&changed),
        Err(McpError::ServerVersionChanged { .. })
    ));
}

async fn connected(behavior: CallBehavior) -> (McpClient, JoinHandle<()>) {
    let (server_io, client_io) = tokio::io::duplex(16 * 1024);
    let server_task = tokio::spawn(run_fixture_server(server_io, behavior));
    let client = McpClient::connect(
        client_io,
        McpClientConfig {
            server_id: "steel-fixture".to_string(),
            request_timeout: Duration::from_millis(40),
            ..McpClientConfig::default()
        },
    )
    .await
    .expect("fixture MCP client should initialize");
    (client, server_task)
}

async fn run_fixture_server(io: tokio::io::DuplexStream, behavior: CallBehavior) {
    let mut server = IntoTransport::<RoleServer, _, _>::into_transport(io);
    let Some(ClientJsonRpcMessage::Request(initialize)) = server.receive().await else {
        return;
    };
    let capabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_resources()
        .enable_prompts()
        .build();
    let initialization = InitializeResult::new(capabilities)
        .with_server_info(Implementation::new("steel-fixture", "1.2.3"));
    server
        .send(ServerJsonRpcMessage::response(
            ServerResult::InitializeResult(initialization),
            initialize.id,
        ))
        .await
        .unwrap();
    let _ = server.receive().await;

    while let Some(ClientJsonRpcMessage::Request(request)) = server.receive().await {
        let id = request.id.clone();
        match request.request {
            ClientRequest::ListToolsRequest(_) => {
                let mut result = ListToolsResult::default();
                result.tools = vec![fixture_tool()];
                server
                    .send(ServerJsonRpcMessage::response(
                        ServerResult::ListToolsResult(result),
                        id,
                    ))
                    .await
                    .unwrap();
            }
            ClientRequest::ListResourcesRequest(_) => {
                let mut result = ListResourcesResult::default();
                result.resources = vec![Resource::new("steel://standards/q355", "Q355 standard")];
                server
                    .send(ServerJsonRpcMessage::response(
                        ServerResult::ListResourcesResult(result),
                        id,
                    ))
                    .await
                    .unwrap();
            }
            ClientRequest::ListPromptsRequest(_) => {
                let mut result = ListPromptsResult::default();
                result.prompts = vec![Prompt::new(
                    "steel.answer",
                    Some("Answer a steel question"),
                    None,
                )];
                server
                    .send(ServerJsonRpcMessage::response(
                        ServerResult::ListPromptsResult(result),
                        id,
                    ))
                    .await
                    .unwrap();
            }
            ClientRequest::CallToolRequest(_) => match behavior {
                CallBehavior::Success => {
                    server
                        .send(ServerJsonRpcMessage::response(
                            ServerResult::CallToolResult(CallToolResult::structured(
                                json!({"grade": "Q355B"}),
                            )),
                            id,
                        ))
                        .await
                        .unwrap();
                }
                CallBehavior::Timeout => tokio::time::sleep(Duration::from_secs(1)).await,
                CallBehavior::ProtocolError => {
                    server
                        .send(ServerJsonRpcMessage::Error(JsonRpcError::new(
                            Some(id),
                            ErrorData::invalid_params("invalid steel grade", None),
                        )))
                        .await
                        .unwrap();
                }
            },
            _ => return,
        }
    }
}

fn fixture_tool() -> Tool {
    let mut tool = Tool::default();
    tool.name = Cow::Borrowed("steel.lookup");
    tool.description = Some(Cow::Borrowed("Look up a steel grade"));
    tool.input_schema = Arc::new(
        json!({
            "type": "object",
            "properties": {"grade": {"type": "string"}},
            "required": ["grade"]
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    tool.annotations = Some(ToolAnnotations::new().read_only(true));
    tool
}
