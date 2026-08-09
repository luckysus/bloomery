use crate::mcp::McpTransportKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct McpServerInput {
    pub id: Option<String>,
    pub display_name: String,
    pub server_id: String,
    pub transport: McpTransportKind,
    pub url: Option<String>,
    pub executable: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub working_directory: Option<String>,
    #[serde(default)]
    pub inherited_env: Vec<String>,
    #[serde(default)]
    pub env_values: BTreeMap<String, String>,
    #[serde(default)]
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub clear_bearer_token: bool,
    pub timeout_ms: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct McpServerSummary {
    pub id: String,
    pub display_name: String,
    pub server_id: String,
    pub transport: McpTransportKind,
    pub url: Option<String>,
    pub executable: Option<String>,
    pub args: Vec<String>,
    pub working_directory: Option<String>,
    pub env_names: Vec<String>,
    pub timeout_ms: u64,
    pub enabled: bool,
    pub secret_configured: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub last_checked_at: Option<String>,
    pub tool_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct McpToolSummary {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct McpHealth {
    pub status: String,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub tool_count: usize,
    pub resource_count: usize,
    pub prompt_count: usize,
    pub tools: Vec<McpToolSummary>,
    pub error: Option<String>,
    pub checked_at: String,
}
