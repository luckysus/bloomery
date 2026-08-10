use super::{
    McpCallResult, McpCapabilities, McpClientConfig, McpError, McpPrompt, McpResource,
    McpServerIdentity, McpTool,
};
use crate::{
    agent::runtime::CancellationToken,
    tools::{ConcurrencyPolicy, ToolDefinition, ToolId, ToolSource, ToolVersion},
};
use rmcp::{
    model::{CallToolRequestParams, ClientInfo, ServerPeerInfo},
    service::RunningService,
    transport::IntoTransport,
    RoleClient, ServiceError, ServiceExt,
};
use serde_json::Value;
use std::{borrow::Cow, collections::BTreeSet, future::Future, time::Duration};

pub struct McpClient {
    service: RunningService<RoleClient, ClientInfo>,
    config: McpClientConfig,
    identity: McpServerIdentity,
    capabilities: McpCapabilities,
}

impl McpClient {
    pub async fn connect<T, E, A>(transport: T, config: McpClientConfig) -> Result<Self, McpError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        validate_config(&config)?;
        let handler = ClientInfo::new(
            Default::default(),
            rmcp::model::Implementation::new(&config.client_name, &config.client_version),
        );
        let service = tokio::time::timeout(config.request_timeout, handler.serve(transport))
            .await
            .map_err(|_| McpError::Timeout)?
            .map_err(|error| McpError::Initialization(error.to_string()))?;
        let peer = service.peer_info().ok_or(McpError::ServerIdentityMissing)?;
        let (identity, capabilities) = peer_info(peer.as_ref())?;
        Ok(Self {
            service,
            config,
            identity,
            capabilities,
        })
    }

    pub fn server_identity(&self) -> &McpServerIdentity {
        &self.identity
    }

    pub fn capabilities(&self) -> &McpCapabilities {
        &self.capabilities
    }

    pub async fn list_tools(&self) -> Result<Vec<McpTool>, McpError> {
        let result = self.request(self.service.list_tools(None)).await?;
        Ok(result
            .tools
            .into_iter()
            .map(|tool| McpTool {
                name: tool.name.into_owned(),
                description: tool.description.map(|value| value.into_owned()),
                input_schema: Value::Object((*tool.input_schema).clone()),
                output_schema: tool
                    .output_schema
                    .map(|schema| Value::Object((*schema).clone())),
                read_only_hint: tool
                    .annotations
                    .as_ref()
                    .and_then(|annotations| annotations.read_only_hint)
                    .unwrap_or(false),
            })
            .collect())
    }

    pub async fn list_resources(&self) -> Result<Vec<McpResource>, McpError> {
        let result = self.request(self.service.list_resources(None)).await?;
        Ok(result
            .resources
            .into_iter()
            .map(|resource| McpResource {
                uri: resource.uri,
                name: resource.name,
                description: resource.description,
                mime_type: resource.mime_type,
            })
            .collect())
    }

    pub async fn list_prompts(&self) -> Result<Vec<McpPrompt>, McpError> {
        let result = self.request(self.service.list_prompts(None)).await?;
        Ok(result
            .prompts
            .into_iter()
            .map(|prompt| McpPrompt {
                name: prompt.name,
                description: prompt.description,
            })
            .collect())
    }

    pub async fn tool_definitions(&self) -> Result<Vec<ToolDefinition>, McpError> {
        let server_version = ToolVersion::parse(&self.identity.version)
            .map_err(|_| McpError::InvalidServerVersion(self.identity.version.clone()))?;
        let tools = self.list_tools().await?;
        tools
            .into_iter()
            .map(|tool| {
                let id = stable_tool_id(&self.config.server_id, &tool.name)?;
                Ok(ToolDefinition {
                    id,
                    version: ToolVersion {
                        major: 1,
                        minor: 0,
                        patch: 0,
                    },
                    name: tool.name,
                    description: tool.description.unwrap_or_else(|| "MCP tool".to_string()),
                    input_schema: normalize_input_schema(tool.input_schema),
                    output_schema: tool
                        .output_schema
                        .unwrap_or_else(|| serde_json::json!({"type": "object"})),
                    risk: crate::agent::protocol::PermissionRisk::ConfirmationRequired,
                    read_only: tool.read_only_hint,
                    concurrency: if tool.read_only_hint {
                        ConcurrencyPolicy::ParallelRead
                    } else {
                        ConcurrencyPolicy::SerialWrite
                    },
                    timeout: self.config.request_timeout,
                    source: ToolSource::Mcp {
                        server_id: self.config.server_id.clone(),
                        server_version,
                    },
                    domains: BTreeSet::new(),
                })
            })
            .collect()
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        cancellation: &CancellationToken,
    ) -> Result<McpCallResult, McpError> {
        let Some(arguments) = arguments.as_object() else {
            return Err(McpError::InvalidArguments);
        };
        let mut request = CallToolRequestParams::default();
        request.name = Cow::Owned(name.to_string());
        request.arguments = Some(arguments.clone());
        let future = self.service.call_tool(request);
        tokio::pin!(future);
        let timeout = tokio::time::sleep(self.config.request_timeout);
        tokio::pin!(timeout);
        loop {
            if cancellation.is_cancelled() {
                return Err(McpError::Cancelled);
            }
            tokio::select! {
                result = &mut future => return result.map(call_result).map_err(map_service_error),
                _ = &mut timeout => return Err(McpError::Timeout),
                _ = tokio::time::sleep(Duration::from_millis(5)) => {}
            }
        }
    }

    pub async fn shutdown(self) -> Result<(), McpError> {
        self.service
            .cancel()
            .await
            .map(|_| ())
            .map_err(|error| McpError::Transport(error.to_string()))
    }

    async fn request<T, F>(&self, future: F) -> Result<T, McpError>
    where
        F: Future<Output = Result<T, ServiceError>>,
    {
        tokio::time::timeout(self.config.request_timeout, future)
            .await
            .map_err(|_| McpError::Timeout)?
            .map_err(map_service_error)
    }
}

fn validate_config(config: &McpClientConfig) -> Result<(), McpError> {
    if config.server_id.trim().is_empty()
        || config.client_name.trim().is_empty()
        || config.client_version.trim().is_empty()
        || config.request_timeout.is_zero()
    {
        return Err(McpError::InvalidConfiguration(
            "server and client identities and timeout are required".to_string(),
        ));
    }
    Ok(())
}

fn peer_info(peer: &ServerPeerInfo) -> Result<(McpServerIdentity, McpCapabilities), McpError> {
    let Some(server) = peer.server_info.as_ref() else {
        return Err(McpError::ServerIdentityMissing);
    };
    Ok((
        McpServerIdentity {
            name: server.name.clone(),
            version: server.version.clone(),
        },
        McpCapabilities {
            tools: peer.capabilities.tools.is_some(),
            resources: peer.capabilities.resources.is_some(),
            prompts: peer.capabilities.prompts.is_some(),
        },
    ))
}

fn stable_tool_id(server_id: &str, name: &str) -> Result<ToolId, McpError> {
    let id = format!("mcp.{}.{}", stable_segment(server_id), stable_segment(name));
    ToolId::new(id).map_err(|_| McpError::InvalidToolId(name.to_string()))
}

/// MCP servers may omit the object type declaration; the shared typed-schema
/// contract requires every tool input schema to declare `"type": "object"`.
fn normalize_input_schema(schema: serde_json::Value) -> serde_json::Value {
    match schema {
        serde_json::Value::Object(mut map) => {
            let declares_object = map
                .get("type")
                .and_then(|value| value.as_str())
                .is_some_and(|value| value == "object");
            if !declares_object {
                map.insert(
                    "type".to_string(),
                    serde_json::Value::String("object".to_string()),
                );
            }
            serde_json::Value::Object(map)
        }
        _ => serde_json::json!({"type": "object"}),
    }
}

fn stable_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_lowercase()
                || character.is_ascii_uppercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_' | '.')
            {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .replace("..", "_")
}

fn call_result(result: rmcp::model::CallToolResult) -> McpCallResult {
    McpCallResult {
        content: result
            .content
            .into_iter()
            .filter_map(|content| serde_json::to_value(content).ok())
            .collect(),
        structured_content: result.structured_content,
        is_error: result.is_error.unwrap_or(false),
    }
}

fn map_service_error(error: ServiceError) -> McpError {
    match error {
        ServiceError::McpError(error) => McpError::Protocol {
            code: error.code.0,
            message: error.message.to_string(),
        },
        ServiceError::Timeout { .. } => McpError::Timeout,
        ServiceError::Cancelled { .. } => McpError::Cancelled,
        other => McpError::Transport(other.to_string()),
    }
}
