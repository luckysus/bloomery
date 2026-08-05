use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

pub const MAX_INLINE_OUTPUT_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub model_output: Value,
    pub artifact: Option<ArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
    pub id: String,
    pub path: PathBuf,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl ToolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Value,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn cancelled() -> Self {
        Self::new("cancelled", "tool execution was cancelled")
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ToolError {}

pub trait ArtifactStore: Send + Sync {
    fn store(&self, bytes: &[u8]) -> Result<ArtifactRef, ToolError>;
}

#[derive(Debug, Clone)]
pub struct FileArtifactStore {
    root: PathBuf,
}

impl FileArtifactStore {
    pub fn new(root: PathBuf) -> Result<Self, ToolError> {
        fs::create_dir_all(&root).map_err(|error| {
            ToolError::new(
                "artifact_store_unavailable",
                format!("cannot create artifact directory: {error}"),
            )
        })?;
        Ok(Self { root })
    }
}

impl ArtifactStore for FileArtifactStore {
    fn store(&self, bytes: &[u8]) -> Result<ArtifactRef, ToolError> {
        let digest = Sha256::digest(bytes);
        let digest = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let id = format!("artifact-{digest}");
        let path = self.root.join(format!("{id}.bin"));
        fs::write(&path, bytes).map_err(|error| {
            ToolError::new(
                "artifact_store_failed",
                format!("cannot write tool artifact: {error}"),
            )
        })?;
        Ok(ArtifactRef {
            id,
            path,
            size_bytes: bytes.len(),
        })
    }
}

pub fn bound_output(value: Value, store: &dyn ArtifactStore) -> Result<ToolOutput, ToolError> {
    let bytes = serde_json::to_vec(&value).map_err(|error| {
        ToolError::new(
            "tool_output_serialization_failed",
            format!("cannot serialize tool output: {error}"),
        )
    })?;
    if bytes.len() <= MAX_INLINE_OUTPUT_BYTES {
        return Ok(ToolOutput {
            model_output: value,
            artifact: None,
        });
    }
    let artifact = store.store(&bytes)?;
    Ok(ToolOutput {
        model_output: json!({
            "truncated": true,
            "artifact_id": artifact.id,
            "size_bytes": artifact.size_bytes,
        }),
        artifact: Some(artifact),
    })
}
