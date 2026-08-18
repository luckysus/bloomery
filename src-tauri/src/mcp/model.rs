use serde::Serialize;
use serde_json::Value;
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpServerIdentity {
    pub name: String,
    pub version: String,
}

impl McpServerIdentity {
    pub fn ensure_same(&self, observed: &Self) -> Result<(), McpError> {
        if self == observed {
            Ok(())
        } else {
            Err(McpError::ServerVersionChanged {
                expected: self.clone(),
                observed: observed.clone(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpCapabilities {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    pub read_only_hint: bool,
    pub destructive_hint: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct McpCallResult {
    pub content: Vec<Value>,
    pub structured_content: Option<Value>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpError {
    InvalidConfiguration(String),
    InvalidTransport(String),
    InvalidArguments,
    Initialization(String),
    Transport(String),
    Protocol {
        code: i32,
        message: String,
    },
    Timeout,
    Cancelled,
    ServerIdentityMissing,
    InvalidServerVersion(String),
    InvalidToolId(String),
    ServerVersionChanged {
        expected: McpServerIdentity,
        observed: McpServerIdentity,
    },
}

impl fmt::Display for McpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => {
                write!(formatter, "invalid MCP configuration: {message}")
            }
            Self::InvalidTransport(message) => {
                write!(formatter, "invalid MCP transport: {message}")
            }
            Self::InvalidArguments => {
                formatter.write_str("MCP tool arguments must be a JSON object")
            }
            Self::Initialization(message) => {
                write!(formatter, "MCP initialization failed: {message}")
            }
            Self::Transport(message) => write!(formatter, "MCP transport failed: {message}"),
            Self::Protocol { code, message } => {
                write!(formatter, "MCP protocol error {code}: {message}")
            }
            Self::Timeout => formatter.write_str("MCP request timed out"),
            Self::Cancelled => formatter.write_str("MCP request was cancelled"),
            Self::ServerIdentityMissing => formatter.write_str("MCP server identity is missing"),
            Self::InvalidServerVersion(version) => {
                write!(formatter, "MCP server version is not semantic: {version}")
            }
            Self::InvalidToolId(name) => write!(
                formatter,
                "MCP tool name cannot become a stable tool id: {name}"
            ),
            Self::ServerVersionChanged { expected, observed } => write!(
                formatter,
                "MCP server changed from {} {} to {} {}",
                expected.name, expected.version, observed.name, observed.version
            ),
        }
    }
}

impl Error for McpError {}
