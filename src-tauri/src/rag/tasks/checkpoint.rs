use super::payload::StoredObjectRef;
use super::validation::{
    invalid_checkpoint, invalid_transition, validate_remote_task_id, validate_sha256, RagTaskError,
};
use crate::rag::model::DocumentVersionId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinerUStage {
    SourceStored,
    Submitting,
    BatchCreated,
    Submitted,
    Polling,
    ArtifactDownloaded,
    Parsed,
    Chunked,
    Embedded,
    Indexed,
    Activated,
}

impl MinerUStage {
    const fn rank(self) -> u8 {
        match self {
            Self::SourceStored => 0,
            Self::Submitting => 1,
            Self::BatchCreated => 2,
            Self::Submitted => 3,
            Self::Polling => 4,
            Self::ArtifactDownloaded => 5,
            Self::Parsed => 6,
            Self::Chunked => 7,
            Self::Embedded => 8,
            Self::Indexed => 9,
            Self::Activated => 10,
        }
    }

    pub const fn progress(self) -> u8 {
        match self {
            Self::SourceStored => 5,
            Self::Submitting => 10,
            Self::BatchCreated => 12,
            Self::Submitted => 15,
            Self::Polling => 25,
            Self::ArtifactDownloaded => 40,
            Self::Parsed => 55,
            Self::Chunked => 70,
            Self::Embedded => 85,
            Self::Indexed => 95,
            Self::Activated => 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinerUCheckpoint {
    stage: MinerUStage,
    source: StoredObjectRef,
    submit_request_sha256: Option<String>,
    remote_task_id: Option<String>,
    artifact: Option<StoredObjectRef>,
    parsed_ast: Option<StoredObjectRef>,
    chunk_manifest_sha256: Option<String>,
    embedding_manifest_sha256: Option<String>,
    index_manifest_sha256: Option<String>,
    activated_version_id: Option<DocumentVersionId>,
}

impl MinerUCheckpoint {
    pub fn source_stored(source: StoredObjectRef) -> Self {
        Self {
            stage: MinerUStage::SourceStored,
            source,
            submit_request_sha256: None,
            remote_task_id: None,
            artifact: None,
            parsed_ast: None,
            chunk_manifest_sha256: None,
            embedding_manifest_sha256: None,
            index_manifest_sha256: None,
            activated_version_id: None,
        }
    }

    pub const fn stage(&self) -> MinerUStage {
        self.stage
    }

    pub const fn progress(&self) -> u8 {
        self.stage.progress()
    }

    pub fn source(&self) -> &StoredObjectRef {
        &self.source
    }

    pub fn remote_task_id(&self) -> Option<&str> {
        self.remote_task_id.as_deref()
    }

    pub fn submit_request_sha256(&self) -> Option<&str> {
        self.submit_request_sha256.as_deref()
    }

    pub fn artifact(&self) -> Option<&StoredObjectRef> {
        self.artifact.as_ref()
    }

    pub fn parsed_ast(&self) -> Option<&StoredObjectRef> {
        self.parsed_ast.as_ref()
    }

    pub fn mark_submitting(
        mut self,
        request_sha256: impl Into<String>,
    ) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::SourceStored, MinerUStage::Submitting)?;
        let request_sha256 = request_sha256.into();
        validate_sha256(&request_sha256, "invalid_mineru_checkpoint")?;
        self.submit_request_sha256 = Some(request_sha256);
        self.stage = MinerUStage::Submitting;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_batch_created(
        mut self,
        remote_task_id: impl Into<String>,
    ) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::Submitting, MinerUStage::BatchCreated)?;
        let remote_task_id = remote_task_id.into();
        validate_remote_task_id(&remote_task_id)?;
        self.remote_task_id = Some(remote_task_id);
        self.stage = MinerUStage::BatchCreated;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_submitted(mut self) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::BatchCreated, MinerUStage::Submitted)?;
        self.stage = MinerUStage::Submitted;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_polling(mut self) -> Result<Self, RagTaskError> {
        if !matches!(self.stage, MinerUStage::Submitted | MinerUStage::Polling) {
            return Err(invalid_transition(self.stage, MinerUStage::Polling));
        }
        self.stage = MinerUStage::Polling;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_artifact_downloaded(
        mut self,
        artifact: StoredObjectRef,
    ) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::Polling, MinerUStage::ArtifactDownloaded)?;
        artifact.validate()?;
        self.artifact = Some(artifact);
        self.stage = MinerUStage::ArtifactDownloaded;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_parsed(mut self, parsed_ast: StoredObjectRef) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::ArtifactDownloaded, MinerUStage::Parsed)?;
        parsed_ast.validate()?;
        self.parsed_ast = Some(parsed_ast);
        self.stage = MinerUStage::Parsed;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_chunked(
        mut self,
        manifest_sha256: impl Into<String>,
    ) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::Parsed, MinerUStage::Chunked)?;
        self.chunk_manifest_sha256 = Some(manifest_sha256.into());
        self.stage = MinerUStage::Chunked;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_embedded(
        mut self,
        manifest_sha256: impl Into<String>,
    ) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::Chunked, MinerUStage::Embedded)?;
        self.embedding_manifest_sha256 = Some(manifest_sha256.into());
        self.stage = MinerUStage::Embedded;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_indexed(
        mut self,
        manifest_sha256: impl Into<String>,
    ) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::Embedded, MinerUStage::Indexed)?;
        self.index_manifest_sha256 = Some(manifest_sha256.into());
        self.stage = MinerUStage::Indexed;
        self.validate()?;
        Ok(self)
    }

    pub fn mark_activated(mut self, version_id: DocumentVersionId) -> Result<Self, RagTaskError> {
        self.require_stage(MinerUStage::Indexed, MinerUStage::Activated)?;
        self.activated_version_id = Some(version_id);
        self.stage = MinerUStage::Activated;
        self.validate()?;
        Ok(self)
    }

    fn require_stage(
        &self,
        expected: MinerUStage,
        target: MinerUStage,
    ) -> Result<(), RagTaskError> {
        if self.stage == expected {
            Ok(())
        } else {
            Err(invalid_transition(self.stage, target))
        }
    }

    fn validate(&self) -> Result<(), RagTaskError> {
        self.source.validate().map_err(invalid_checkpoint)?;
        match (self.stage, self.submit_request_sha256.as_deref()) {
            (MinerUStage::SourceStored, None) => {}
            (MinerUStage::Submitting, Some(value)) => {
                validate_sha256(value, "invalid_mineru_checkpoint")?;
            }
            (MinerUStage::SourceStored | MinerUStage::Submitting, _) => {
                return Err(RagTaskError::new(
                    "invalid_mineru_checkpoint",
                    "submit_request_sha256 does not match the checkpoint stage",
                ));
            }
            (_, Some(value)) => validate_sha256(value, "invalid_mineru_checkpoint")?,
            (_, None) => {}
        }
        validate_optional(
            self.stage,
            MinerUStage::BatchCreated,
            self.remote_task_id.as_deref(),
            "remote_task_id",
            validate_remote_task_id,
        )?;
        validate_object_field(
            self.stage,
            MinerUStage::ArtifactDownloaded,
            self.artifact.as_ref(),
            "artifact",
        )?;
        validate_object_field(
            self.stage,
            MinerUStage::Parsed,
            self.parsed_ast.as_ref(),
            "parsed_ast",
        )?;
        for (minimum, value, name) in [
            (
                MinerUStage::Chunked,
                self.chunk_manifest_sha256.as_deref(),
                "chunk_manifest_sha256",
            ),
            (
                MinerUStage::Embedded,
                self.embedding_manifest_sha256.as_deref(),
                "embedding_manifest_sha256",
            ),
            (
                MinerUStage::Indexed,
                self.index_manifest_sha256.as_deref(),
                "index_manifest_sha256",
            ),
        ] {
            validate_optional(self.stage, minimum, value, name, |value| {
                validate_sha256(value, "invalid_mineru_checkpoint")
            })?;
        }
        let activated_required = self.stage.rank() >= MinerUStage::Activated.rank();
        if activated_required != self.activated_version_id.is_some() {
            return Err(RagTaskError::new(
                "invalid_mineru_checkpoint",
                "activated_version_id does not match the checkpoint stage",
            ));
        }
        Ok(())
    }
}

pub fn decode_mineru_checkpoint(value: &str) -> Result<MinerUCheckpoint, RagTaskError> {
    let checkpoint: MinerUCheckpoint = serde_json::from_str(value).map_err(|error| {
        RagTaskError::new(
            "invalid_mineru_checkpoint",
            format!("checkpoint JSON is invalid: {error}"),
        )
    })?;
    checkpoint.validate()?;
    Ok(checkpoint)
}

fn validate_object_field(
    stage: MinerUStage,
    minimum: MinerUStage,
    value: Option<&StoredObjectRef>,
    name: &str,
) -> Result<(), RagTaskError> {
    validate_optional(stage, minimum, value, name, |value| {
        value.validate().map_err(invalid_checkpoint)
    })
}

fn validate_optional<T>(
    stage: MinerUStage,
    minimum: MinerUStage,
    value: Option<T>,
    name: &str,
    validate: impl FnOnce(T) -> Result<(), RagTaskError>,
) -> Result<(), RagTaskError> {
    let required = stage.rank() >= minimum.rank();
    if required != value.is_some() {
        return Err(RagTaskError::new(
            "invalid_mineru_checkpoint",
            format!("{name} does not match the checkpoint stage"),
        ));
    }
    if let Some(value) = value {
        validate(value)?;
    }
    Ok(())
}
