use crate::agent::context::{ContextItem, ContextReport, ContextSource};
use crate::agent::protocol::{
    AgentError, AgentEventData, AgentEventEnvelope, AgentMessageRole, AgentRunState,
    PermissionDecision, PermissionRisk, RunOutcome, RunStateChanged,
};
use crate::agent::tool_repair::ToolSpec;
use crate::providers::capabilities::ChatUsage;
use crate::providers::http::ProviderError;
use serde_json::Value;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

pub(super) const MAX_TOOL_ROUNDS: usize = 8;
pub(super) const MAX_TOOL_OUTPUT_BYTES: usize = 32 * 1024;

pub type ToolFuture =
    Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + 'static>>;
pub type PermissionFuture = Pin<Box<dyn Future<Output = PermissionDecision> + Send + 'static>>;

#[derive(Clone)]
pub struct CancellationToken(Arc<dyn Fn() -> bool + Send + Sync>);

impl CancellationToken {
    pub fn new(callback: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(callback))
    }

    pub fn is_cancelled(&self) -> bool {
        (self.0)()
    }

    pub(super) fn callback(&self) -> &(dyn Fn() -> bool + Send + Sync) {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionError {
    pub code: String,
    pub message: String,
    pub cancelled: bool,
}

impl ToolExecutionError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            cancelled: false,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            code: "cancelled".to_string(),
            message: "tool execution was cancelled".to_string(),
            cancelled: true,
        }
    }
}

#[derive(Clone)]
pub struct ToolRegistration {
    pub spec: ToolSpec,
    pub read_only: bool,
    pub handler: Arc<dyn ToolHandler>,
}

impl ToolRegistration {
    pub fn new(spec: ToolSpec, read_only: bool, handler: Arc<dyn ToolHandler>) -> Self {
        Self {
            spec,
            read_only,
            handler,
        }
    }
}

pub trait ToolHandler: Send + Sync {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture;
}

pub trait ToolExecutor: Send + Sync {
    fn registrations(&self) -> &[ToolRegistration];

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture;
}

#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub tool_call_id: Uuid,
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone)]
pub struct EvidenceAttachment {
    pub evidence_pack_id: Uuid,
    pub citation_numbers: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct ContextEntry {
    pub item: ContextItem,
    pub role: AgentMessageRole,
}

impl ContextEntry {
    pub fn new(item: ContextItem) -> Self {
        let role = match item.source {
            ContextSource::CurrentRequest | ContextSource::RecentTurn { .. } => {
                AgentMessageRole::User
            }
            _ => AgentMessageRole::System,
        };
        Self { item, role }
    }

    pub fn with_role(item: ContextItem, role: AgentMessageRole) -> Self {
        Self { item, role }
    }
}

#[derive(Debug, Clone)]
pub struct AgentLoopRequest {
    pub assistant_message_id: Uuid,
    pub context: Vec<ContextEntry>,
    pub output_reservation: usize,
    pub evidence: Option<EvidenceAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLoopResult {
    pub outcome: RunOutcome,
    pub answer: String,
    pub usage: Option<ChatUsage>,
    pub context: ContextReport,
}

pub trait PermissionResolver: Send + Sync {
    fn decide(
        &self,
        request: PermissionRequest,
        cancellation: CancellationToken,
    ) -> PermissionFuture;
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub permission_id: Uuid,
    pub tool_call_id: Uuid,
    pub tool_id: String,
    pub tool_name: String,
    pub risk: PermissionRisk,
    pub arguments: Value,
}

pub struct DenyPermissions;

impl PermissionResolver for DenyPermissions {
    fn decide(
        &self,
        _request: PermissionRequest,
        _cancellation: CancellationToken,
    ) -> PermissionFuture {
        Box::pin(async { PermissionDecision::Deny })
    }
}

pub struct NoopToolExecutor;

impl ToolExecutor for NoopToolExecutor {
    fn registrations(&self) -> &[ToolRegistration] {
        &[]
    }

    fn execute(&self, _invocation: ToolInvocation, _cancellation: CancellationToken) -> ToolFuture {
        Box::pin(async {
            Err(ToolExecutionError::new(
                "tool_not_registered",
                "no local tools are registered",
            ))
        })
    }
}

pub trait AgentEventSink: Send {
    fn record(&mut self, data: AgentEventData) -> Result<AgentEventEnvelope, String>;

    fn transition(&mut self, changed: RunStateChanged) -> Result<AgentEventEnvelope, String>;

    fn finish(
        &mut self,
        changed: RunStateChanged,
        outcome: RunOutcome,
        assistant_message_id: Option<Uuid>,
    ) -> Result<Vec<AgentEventEnvelope>, String>;
}

#[derive(Debug)]
pub enum AgentLoopError {
    Context(crate::agent::context::ContextBudgetError),
    Provider(ProviderError),
    Capability(String),
    ToolRepair(crate::agent::tool_repair::ToolRepairError),
    Tool(String),
    Citation(String),
    EventSink(String),
    Internal(String),
}

impl fmt::Display for AgentLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(error) => write!(formatter, "context: {error}"),
            Self::Provider(error) => write!(formatter, "provider: {error}"),
            Self::Capability(message)
            | Self::Tool(message)
            | Self::Citation(message)
            | Self::EventSink(message)
            | Self::Internal(message) => formatter.write_str(message),
            Self::ToolRepair(error) => write!(formatter, "tool repair: {error}"),
        }
    }
}

impl std::error::Error for AgentLoopError {}

pub struct AgentLoop<'a, M: ?Sized, T: ?Sized, P: ?Sized> {
    pub(super) model: &'a M,
    pub(super) tools: &'a T,
    pub(super) permissions: &'a P,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedToolCall {
    pub(super) model_call_id: String,
    pub(super) tool_call_id: Uuid,
    pub(super) tool_id: String,
    pub(super) tool_name: String,
    pub(super) arguments: Value,
    pub(super) risk: PermissionRisk,
    pub(super) read_only: bool,
}

pub(super) struct RepairedToolBatch {
    pub(super) model_calls: Vec<crate::providers::capabilities::ChatToolCall>,
    pub(super) calls: Vec<PreparedToolCall>,
}

#[allow(dead_code)]
pub(super) fn _agent_error_marker(_error: AgentError) -> bool {
    true
}

#[allow(dead_code)]
pub(super) fn _agent_state_marker(_state: AgentRunState) -> bool {
    true
}
