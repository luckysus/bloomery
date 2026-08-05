use crate::agent::protocol::PermissionRisk;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, fmt, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolId(String);

impl ToolId {
    pub fn new(value: impl Into<String>) -> Result<Self, ToolIdError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(ToolIdError::InvalidFormat);
        }
        if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
            return Err(ToolIdError::InvalidFormat);
        }
        if !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        }) {
            return Err(ToolIdError::InvalidFormat);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ToolId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolIdError {
    InvalidFormat,
}

impl fmt::Display for ToolIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("tool id must use lowercase stable segments separated by dots")
    }
}

impl std::error::Error for ToolIdError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ToolVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl ToolVersion {
    pub fn parse(value: &str) -> Result<Self, ToolVersionError> {
        let parts = value.split('.').collect::<Vec<_>>();
        if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
            return Err(ToolVersionError::InvalidFormat);
        }
        if parts
            .iter()
            .any(|part| part.len() > 1 && part.starts_with('0'))
        {
            return Err(ToolVersionError::InvalidFormat);
        }
        let [major, minor, patch] = parts.as_slice() else {
            return Err(ToolVersionError::InvalidFormat);
        };
        Ok(Self {
            major: major.parse().map_err(|_| ToolVersionError::InvalidFormat)?,
            minor: minor.parse().map_err(|_| ToolVersionError::InvalidFormat)?,
            patch: patch.parse().map_err(|_| ToolVersionError::InvalidFormat)?,
        })
    }
}

impl fmt::Display for ToolVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolVersionError {
    InvalidFormat,
}

impl fmt::Display for ToolVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("tool version must use MAJOR.MINOR.PATCH")
    }
}

impl std::error::Error for ToolVersionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcurrencyPolicy {
    ParallelRead,
    SerialWrite,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolSource {
    Builtin,
    Mcp {
        server_id: String,
        server_version: ToolVersion,
    },
    Domain {
        package_id: String,
        package_version: ToolVersion,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDefinition {
    pub id: ToolId,
    pub version: ToolVersion,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub risk: PermissionRisk,
    pub read_only: bool,
    pub concurrency: ConcurrencyPolicy,
    pub timeout: Duration,
    pub source: ToolSource,
    pub domains: BTreeSet<String>,
}
