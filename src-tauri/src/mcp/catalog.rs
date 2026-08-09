use super::{McpError, McpHttpConfig, McpLegacySseConfig, McpStdioConfig, McpTransportConfig};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};
use uuid::Uuid;

const MIN_TIMEOUT_MS: u64 = 100;
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const MAX_TEXT_LENGTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransportKind {
    Stdio,
    StreamableHttp,
    Sse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerConfig {
    pub id: Uuid,
    pub display_name: String,
    pub server_id: String,
    pub transport: McpTransportKind,
    pub url: Option<String>,
    pub executable: Option<PathBuf>,
    pub args: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub inherited_env: Vec<String>,
    pub env_names: Vec<String>,
    pub timeout: Duration,
    pub enabled: bool,
}

impl McpServerConfig {
    pub fn normalize(&mut self) -> Result<(), McpError> {
        self.display_name = self.display_name.trim().to_string();
        self.server_id = self.server_id.trim().to_string();
        self.url = self.url.take().map(|value| value.trim().to_string());
        self.executable = self
            .executable
            .take()
            .map(|value| PathBuf::from(value.to_string_lossy().trim().to_string()));
        self.working_directory = self
            .working_directory
            .take()
            .map(|value| PathBuf::from(value.to_string_lossy().trim().to_string()));
        self.env_names = normalize_env_names(std::mem::take(&mut self.env_names));
        self.validate()
    }

    pub fn validate(&self) -> Result<(), McpError> {
        if self.display_name.is_empty() || self.server_id.is_empty() {
            return Err(McpError::InvalidConfiguration(
                "MCP display name and server id are required".to_string(),
            ));
        }
        if self.display_name.len() > MAX_TEXT_LENGTH || self.server_id.len() > MAX_TEXT_LENGTH {
            return Err(McpError::InvalidConfiguration(
                "MCP display name and server id are too long".to_string(),
            ));
        }
        let timeout_ms = self.timeout.as_millis();
        if !(MIN_TIMEOUT_MS as u128..=MAX_TIMEOUT_MS as u128).contains(&timeout_ms) {
            return Err(McpError::InvalidConfiguration(format!(
                "MCP timeout must be between {MIN_TIMEOUT_MS}ms and {MAX_TIMEOUT_MS}ms"
            )));
        }
        for name in self.env_names.iter().chain(self.inherited_env.iter()) {
            validate_env_name(name)?;
        }
        match self.transport {
            McpTransportKind::Stdio => {
                if self
                    .executable
                    .as_ref()
                    .is_none_or(|value| value.as_os_str().is_empty())
                {
                    return Err(McpError::InvalidTransport(
                        "stdio executable is required".to_string(),
                    ));
                }
                if self.url.is_some() {
                    return Err(McpError::InvalidConfiguration(
                        "stdio MCP servers must not define a URL".to_string(),
                    ));
                }
            }
            McpTransportKind::StreamableHttp => {
                let Some(url) = self.url.as_deref() else {
                    return Err(McpError::InvalidTransport(
                        "streamable HTTP URL is required".to_string(),
                    ));
                };
                McpHttpConfig::new(url).validate()?;
                if self.executable.is_some() {
                    return Err(McpError::InvalidConfiguration(
                        "HTTP MCP servers must not define an executable".to_string(),
                    ));
                }
            }
            McpTransportKind::Sse => {
                let Some(url) = self.url.as_deref() else {
                    return Err(McpError::InvalidTransport(
                        "legacy SSE URL is required".to_string(),
                    ));
                };
                McpLegacySseConfig::new(url).validate()?;
                if self.executable.is_some() {
                    return Err(McpError::InvalidConfiguration(
                        "legacy SSE MCP servers must not define an executable".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn transport(
        &self,
        bearer_token: Option<String>,
        env: std::collections::BTreeMap<String, String>,
    ) -> Result<McpTransportConfig, McpError> {
        self.validate()?;
        match self.transport {
            McpTransportKind::Stdio => Ok(McpTransportConfig::Stdio(McpStdioConfig {
                executable: self.executable.clone().expect("validated executable"),
                args: self.args.clone(),
                working_directory: self.working_directory.clone(),
                inherited_env: self.inherited_env.clone(),
                env,
                max_stderr_bytes: 1024 * 1024,
            })),
            McpTransportKind::StreamableHttp => {
                let mut config = McpHttpConfig::new(self.url.clone().expect("validated URL"));
                if let Some(token) = bearer_token {
                    config = config.with_bearer_token(token);
                }
                Ok(McpTransportConfig::Http(config))
            }
            McpTransportKind::Sse => {
                let mut config = McpLegacySseConfig::new(self.url.clone().expect("validated URL"));
                if let Some(token) = bearer_token {
                    config = config.with_bearer_token(token);
                }
                Ok(McpTransportConfig::LegacySse(config))
            }
        }
    }

    pub fn timeout_ms(&self) -> Result<u64, McpError> {
        u64::try_from(self.timeout.as_millis()).map_err(|_| {
            McpError::InvalidConfiguration("MCP timeout does not fit in milliseconds".to_string())
        })
    }
}

fn validate_env_name(name: &str) -> Result<(), McpError> {
    if name.is_empty()
        || name.len() > 50
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(McpError::InvalidTransport(
            "MCP environment names must contain only letters, digits, and underscores".to_string(),
        ));
    }
    Ok(())
}

pub fn normalize_env_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    names
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
