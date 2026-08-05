use super::validation::{validate_sha256, RagTaskError};
use crate::rag::model::{DocumentVersionId, SourceDocumentId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredObjectRef {
    sha256: String,
    storage_key: String,
}

impl StoredObjectRef {
    pub fn new(
        sha256: impl Into<String>,
        storage_key: impl Into<String>,
    ) -> Result<Self, RagTaskError> {
        let value = Self {
            sha256: sha256.into(),
            storage_key: storage_key.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn storage_key(&self) -> &str {
        &self.storage_key
    }

    pub(super) fn validate(&self) -> Result<(), RagTaskError> {
        validate_sha256(&self.sha256, "invalid_stored_object")?;
        let expected = format!("objects/sha256/{}/{}", &self.sha256[..2], self.sha256);
        if self.storage_key != expected {
            return Err(RagTaskError::new(
                "invalid_stored_object",
                "storage key does not match the content digest",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinerUTaskPayload {
    pub document_id: SourceDocumentId,
    pub version_id: DocumentVersionId,
    pub provider_profile_id: String,
    #[serde(default)]
    pub provider_profile_revision: u64,
    #[serde(default)]
    pub provider_secret_generation: u64,
    #[serde(default)]
    pub embedding_profile_revision: u64,
    #[serde(default)]
    pub embedding_secret_generation: u64,
    pub source: StoredObjectRef,
    pub file_name: String,
    pub mime_type: String,
}

impl MinerUTaskPayload {
    pub fn validate(&self) -> Result<(), RagTaskError> {
        self.source
            .validate()
            .map_err(|error| RagTaskError::new("invalid_mineru_payload", error.to_string()))?;
        Uuid::parse_str(&self.provider_profile_id).map_err(|error| {
            RagTaskError::new(
                "invalid_mineru_payload",
                format!("invalid provider profile ID: {error}"),
            )
        })?;
        if self.file_name.trim() != self.file_name
            || self.file_name.is_empty()
            || self.file_name.chars().count() > 255
            || self.file_name.contains(['/', '\\'])
            || matches!(self.file_name.as_str(), "." | "..")
            || self.file_name.chars().any(char::is_control)
        {
            return Err(RagTaskError::new(
                "invalid_mineru_payload",
                "source file name is unsafe",
            ));
        }
        if self.mime_type.trim() != self.mime_type
            || self.mime_type.is_empty()
            || self.mime_type.len() > 128
            || !self.mime_type.is_ascii()
        {
            return Err(RagTaskError::new(
                "invalid_mineru_payload",
                "source MIME type is invalid",
            ));
        }
        Ok(())
    }
}
