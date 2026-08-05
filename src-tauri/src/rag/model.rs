use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

macro_rules! typed_uuid {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }
    };
}

typed_uuid!(KnowledgeBaseId);
typed_uuid!(SourceDocumentId);
typed_uuid!(DocumentVersionId);
typed_uuid!(AssetId);
typed_uuid!(IngestAttemptId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChunkId(String);

impl ChunkId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim() != value
            || value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err("invalid chunk ID".to_string());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChunkId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ChunkId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceLocation {
    PdfPage { page: u32, bbox: Option<Rect> },
    SheetRange { sheet: String, range: String },
    Heading { path: Vec<String> },
    TextOffsets { start: u64, end: u64 },
}

impl SourceLocation {
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::PdfPage { page, bbox } => {
                if *page == 0 {
                    return Err("PDF page must be at least one".to_string());
                }
                if let Some(rect) = bbox {
                    let values = [rect.x, rect.y, rect.width, rect.height];
                    if values.iter().any(|value| !value.is_finite())
                        || rect.width < 0.0
                        || rect.height < 0.0
                    {
                        return Err("invalid PDF bounding box".to_string());
                    }
                }
            }
            Self::SheetRange { sheet, range } => {
                required("sheet", sheet)?;
                required("range", range)?;
            }
            Self::Heading { path } => {
                if path.is_empty() || path.iter().any(|item| item.trim().is_empty()) {
                    return Err("heading path is required".to_string());
                }
            }
            Self::TextOffsets { start, end } if start >= end => {
                return Err("text offsets must be increasing".to_string());
            }
            Self::TextOffsets { .. } => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NewSourceDocument {
    pub knowledge_base_id: KnowledgeBaseId,
    pub display_name: String,
    pub source_kind: String,
}

#[derive(Debug, Clone)]
pub struct NewDocumentVersion {
    pub document_id: SourceDocumentId,
    pub content_sha256: String,
    pub mime_type: String,
    pub parser: String,
    pub parser_version: String,
    pub chunk_policy_version: String,
    pub embedding_profile_id: String,
    pub embedding_model_id: String,
    pub embedding_dimension: u32,
    pub expected_asset_count: u32,
    pub expected_chunk_count: u32,
}

#[derive(Debug, Clone)]
pub struct NewAsset {
    pub version_id: DocumentVersionId,
    pub kind: String,
    pub storage_key: String,
    pub sha256: String,
    pub media_type: String,
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Clone)]
pub struct NewChunk {
    pub id: ChunkId,
    pub version_id: DocumentVersionId,
    pub ordinal: u32,
    pub text: String,
    pub source_location: SourceLocation,
    pub content_sha256: String,
    pub policy_version: String,
}

#[derive(Debug, Clone)]
pub struct NewChunkEmbedding {
    pub version_id: DocumentVersionId,
    pub chunk_id: ChunkId,
    pub provider_profile_id: String,
    pub model_id: String,
    pub dimension: u32,
    pub normalized_text_sha256: String,
    pub policy_version: String,
    pub vector_key: String,
}

#[derive(Debug, Clone)]
pub struct ChunkEmbeddingSource {
    pub id: ChunkId,
    pub ordinal: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingIdentity {
    pub provider_profile_id: String,
    pub model_id: String,
    pub dimension: u32,
    pub normalized_text_sha256: String,
    pub policy_version: String,
}

#[derive(Debug, Clone)]
pub struct EmbeddingVectorBatch {
    pub vector_key: String,
    pub identity: EmbeddingIdentity,
    pub vector_blob: Vec<u8>,
    pub vector_sha256: String,
    pub chunk_ids: Vec<ChunkId>,
}

#[derive(Debug, Clone)]
pub struct VectorWatermark {
    pub version_id: DocumentVersionId,
    pub provider_profile_id: String,
    pub model_id: String,
    pub dimension: u32,
    pub expected_count: u32,
    pub indexed_count: u32,
    pub index_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestAttemptState {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl IngestAttemptState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

impl FromStr for IngestAttemptState {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(format!("unknown ingest attempt state: {value}")),
        }
    }
}

pub(crate) fn required(name: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.trim() != value || value.chars().count() > 512 {
        Err(format!("{name} is invalid"))
    } else {
        Ok(())
    }
}

pub(crate) fn sha256(name: &str, value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("{name} must be a SHA-256 hex digest"))
    }
}
