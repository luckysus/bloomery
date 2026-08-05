use super::ChunkError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkPolicy {
    pub version: String,
    pub target_tokens: usize,
    pub max_tokens: usize,
    pub overlap_tokens: usize,
    pub table_header_rows: usize,
}

impl Default for ChunkPolicy {
    fn default() -> Self {
        Self {
            version: "steel-v1".to_string(),
            target_tokens: 384,
            max_tokens: 512,
            overlap_tokens: 64,
            table_header_rows: 1,
        }
    }
}

impl ChunkPolicy {
    pub fn validate(&self) -> Result<(), ChunkError> {
        if self.version.trim() != self.version
            || self.version.is_empty()
            || self.version.len() > 64
            || !self
                .version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || self.target_tokens == 0
            || self.target_tokens > self.max_tokens
            || self.overlap_tokens >= self.target_tokens
            || self.table_header_rows == 0
        {
            return Err(ChunkError::new(
                "invalid_chunk_policy",
                "chunk policy is invalid or cannot make forward progress",
            ));
        }
        Ok(())
    }
}
