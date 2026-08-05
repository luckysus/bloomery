use super::MinerUStage;
use std::fmt;

pub(super) fn validate_remote_task_id(value: &str) -> Result<(), RagTaskError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(RagTaskError::new(
            "invalid_mineru_checkpoint",
            "remote task ID is invalid",
        ));
    }
    Ok(())
}

pub(super) fn validate_sha256(value: &str, code: &'static str) -> Result<(), RagTaskError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(RagTaskError::new(
            code,
            "value is not a canonical SHA-256 digest",
        ));
    }
    Ok(())
}

pub(super) fn invalid_transition(current: MinerUStage, target: MinerUStage) -> RagTaskError {
    RagTaskError::new(
        "invalid_mineru_transition",
        format!("cannot move from {current:?} to {target:?}"),
    )
}

pub(super) fn invalid_checkpoint(error: RagTaskError) -> RagTaskError {
    RagTaskError::new("invalid_mineru_checkpoint", error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RagTaskError {
    code: &'static str,
    message: String,
}

impl RagTaskError {
    pub(super) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for RagTaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for RagTaskError {}
