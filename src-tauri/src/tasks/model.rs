use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

const MAX_IDENTIFIER_LENGTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    WaitingExternal,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl TaskState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingExternal => "waiting_external",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Queued, Self::Running | Self::Paused | Self::Cancelled)
                | (
                    Self::Running,
                    Self::WaitingExternal
                        | Self::Paused
                        | Self::Completed
                        | Self::Failed
                        | Self::Cancelled
                        | Self::Interrupted
                )
                | (
                    Self::WaitingExternal,
                    Self::Running
                        | Self::Paused
                        | Self::Completed
                        | Self::Failed
                        | Self::Cancelled
                        | Self::Interrupted
                )
                | (Self::Paused, Self::Queued | Self::Cancelled)
                | (Self::Failed, Self::Queued)
                | (Self::Cancelled, Self::Queued)
                | (Self::Interrupted, Self::Queued | Self::Cancelled)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTaskState(String);

impl fmt::Display for InvalidTaskState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown task state: {}", self.0)
    }
}

impl std::error::Error for InvalidTaskState {}

impl FromStr for TaskState {
    type Err = InvalidTaskState;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "waiting_external" => Ok(Self::WaitingExternal),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            value => Err(InvalidTaskState(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskError {
    code: &'static str,
    message: String,
}

impl TaskError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for TaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for TaskError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: Uuid,
    pub workspace_id: String,
    pub kind: String,
    pub state: TaskState,
    pub payload_json: String,
    pub checkpoint_json: Option<String>,
    pub attempt: u32,
    pub next_run_at: Option<String>,
    pub progress: u8,
    pub error_code: Option<String>,
    pub cancel_requested: bool,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTask {
    pub workspace_id: String,
    pub kind: String,
    pub payload_json: String,
    pub checkpoint_json: Option<String>,
    pub next_run_at: Option<String>,
    pub progress: u8,
}

impl NewTask {
    pub fn validate(&self) -> Result<(), TaskError> {
        validate_identifier("workspace_id", &self.workspace_id)?;
        validate_identifier("kind", &self.kind)?;
        validate_json("payload_json", &self.payload_json)?;
        if let Some(checkpoint) = &self.checkpoint_json {
            validate_json("checkpoint_json", checkpoint)?;
        }
        if let Some(next_run_at) = &self.next_run_at {
            validate_timestamp("next_run_at", next_run_at)?;
        }
        validate_progress(self.progress)
    }
}

impl TaskRecord {
    pub(crate) fn validate(&self) -> Result<(), TaskError> {
        validate_identifier("workspace_id", &self.workspace_id)?;
        validate_identifier("kind", &self.kind)?;
        validate_json("payload_json", &self.payload_json)?;
        if let Some(checkpoint) = &self.checkpoint_json {
            validate_json("checkpoint_json", checkpoint)?;
        }
        if let Some(next_run_at) = &self.next_run_at {
            validate_timestamp("next_run_at", next_run_at)?;
        }
        validate_progress(self.progress)?;
        validate_timestamp("created_at", &self.created_at)?;
        validate_timestamp("updated_at", &self.updated_at)?;
        if let Some(started_at) = &self.started_at {
            validate_timestamp("started_at", started_at)?;
        }
        if let Some(finished_at) = &self.finished_at {
            validate_timestamp("finished_at", finished_at)?;
        }
        if self.state == TaskState::Completed
            && (self.progress != 100 || self.next_run_at.is_some())
        {
            return Err(TaskError::new(
                "invalid_task",
                "completed tasks require 100 progress and no next run time",
            ));
        }
        match (self.state, self.error_code.as_deref()) {
            (TaskState::Failed, Some(error_code)) => {
                validate_identifier("error_code", error_code)?;
            }
            (TaskState::Failed, None) => {
                return Err(TaskError::new(
                    "invalid_task",
                    "failed tasks require an error_code",
                ));
            }
            (_, Some(_)) => {
                return Err(TaskError::new(
                    "invalid_task",
                    "error_code is only valid for failed tasks",
                ));
            }
            (_, None) => {}
        }
        Ok(())
    }
}

pub(crate) fn validate_timestamp(name: &str, value: &str) -> Result<(), TaskError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|error| TaskError::new("invalid_task", format!("{name} must be RFC3339: {error}")))
}

pub(crate) fn validate_identifier(name: &str, value: &str) -> Result<(), TaskError> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.chars().count() > MAX_IDENTIFIER_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(TaskError::new(
            "invalid_task",
            format!(
                "{name} must use 1-{MAX_IDENTIFIER_LENGTH} ASCII letters, digits, '.', '_' or '-'"
            ),
        ));
    }
    Ok(())
}

pub(crate) fn validate_json(name: &str, value: &str) -> Result<(), TaskError> {
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|error| TaskError::new("invalid_task", format!("{name} must be JSON: {error}")))
}

pub(crate) fn validate_progress(progress: u8) -> Result<(), TaskError> {
    if progress > 100 {
        return Err(TaskError::new(
            "invalid_task",
            "progress must be between 0 and 100",
        ));
    }
    Ok(())
}
