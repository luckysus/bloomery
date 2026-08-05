use super::AgentError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEventEnvelope {
    pub protocol_version: u16,
    pub event_id: Uuid,
    pub run_id: Uuid,
    pub conversation_id: Uuid,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub data: AgentEventData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum AgentEventData {
    RunCreated(RunCreated),
    RunStateChanged(RunStateChanged),
    MessageDelta(MessageDelta),
    MessageCompleted(MessageCompleted),
    ToolRequested(ToolRequested),
    ToolStarted(ToolStarted),
    ToolProgress(ToolProgress),
    ToolCompleted(ToolCompleted),
    PermissionRequested(PermissionRequested),
    PermissionResolved(PermissionResolved),
    EvidenceAttached(EvidenceAttached),
    UsageUpdated(UsageUpdated),
    TaskProgress(TaskProgress),
    RunCompleted(RunCompleted),
    ErrorRaised(ErrorRaised),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunState {
    Created,
    Preparing,
    Generating,
    AwaitingPermission,
    ExecutingTools,
    Verifying,
    Completing,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRisk {
    Automatic,
    ConfirmationRequired,
    Dangerous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    AllowOnce,
    AllowSession,
    AllowAlways,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProgressState {
    Queued,
    Running,
    WaitingExternal,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCreated {
    pub state: AgentRunState,
    pub user_message_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStateChanged {
    pub previous: AgentRunState,
    pub current: AgentRunState,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageDelta {
    pub message_id: Uuid,
    pub role: AgentMessageRole,
    pub delta: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageCompleted {
    pub message_id: Uuid,
    pub role: AgentMessageRole,
    pub content: String,
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolRequested {
    pub tool_call_id: Uuid,
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolStarted {
    pub tool_call_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProgress {
    pub tool_call_id: Uuid,
    pub progress: u8,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCompleted {
    pub tool_call_id: Uuid,
    pub outcome: ToolOutcome,
    pub output: Option<Value>,
    pub error: Option<AgentError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequested {
    pub permission_id: Uuid,
    pub tool_call_id: Uuid,
    pub risk: PermissionRisk,
    pub reason: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionResolved {
    pub permission_id: Uuid,
    pub decision: PermissionDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceAttached {
    pub evidence_pack_id: Uuid,
    pub citation_numbers: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageUpdated {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_id: Uuid,
    pub kind: String,
    pub state: TaskProgressState,
    pub progress: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCompleted {
    pub outcome: RunOutcome,
    pub assistant_message_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorRaised {
    pub error: AgentError,
    pub fatal: bool,
}
