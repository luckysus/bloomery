pub(crate) mod config_commands;
pub(crate) mod logic;
pub(crate) mod runtime_commands;
pub(crate) mod types;

pub(crate) async fn connect_server(
    secrets: &crate::storage::secrets::SecretState,
    config: &crate::mcp::McpServerConfig,
) -> Result<crate::mcp::McpSupervisor, crate::mcp::McpError> {
    logic::connect(secrets, config).await
}
