use std::time::Duration;

#[derive(Debug, Clone)]
pub struct McpClientConfig {
    pub server_id: String,
    pub client_name: String,
    pub client_version: String,
    pub request_timeout: Duration,
}

impl Default for McpClientConfig {
    fn default() -> Self {
        Self {
            server_id: "mcp-server".to_string(),
            client_name: "Bloomery".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            request_timeout: Duration::from_secs(30),
        }
    }
}
