use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorCategory {
    Configuration,
    Authentication,
    Quota,
    Network,
    Parsing,
    Indexing,
    ModelCapability,
    ToolPermission,
    Mcp,
    Database,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentError {
    pub code: String,
    pub category: AgentErrorCategory,
    pub message: String,
    pub retryable: bool,
    pub details: Option<Value>,
}
