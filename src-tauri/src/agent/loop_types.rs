use crate::agent::context::{ContextItem, ContextReport, ContextSource};
use crate::agent::protocol::{
    AgentError, AgentEventData, AgentEventEnvelope, AgentMessageRole, AgentRunState,
    PermissionDecision, PermissionRisk, RunOutcome, RunStateChanged,
};
use crate::agent::tool_repair::ToolSpec;
use crate::providers::capabilities::ChatUsage;
use crate::providers::http::ProviderError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub(super) const MAX_TOOL_OUTPUT_BYTES: usize = 32 * 1024;

/// Per-turn execution budget. Model and tool call limits are opt-in; tool
/// limits are explicit; an omitted limit is intentionally uncapped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLoopLimits {
    pub max_model_calls: Option<usize>,
    pub max_tool_calls: Option<usize>,
    pub max_tool_rounds: Option<usize>,
    pub max_recovery_attempts: usize,
    pub context_checkpoint_timeout_ms: u64,
    pub deadline_ms: Option<u64>,
}

impl Default for AgentLoopLimits {
    fn default() -> Self {
        Self {
            max_model_calls: None,
            max_tool_calls: None,
            max_tool_rounds: None,
            max_recovery_attempts: 2,
            context_checkpoint_timeout_ms: 300_000,
            deadline_ms: None,
        }
    }
}

impl AgentLoopLimits {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_model_calls == Some(0) {
            return Err("max_model_calls must be greater than zero".to_string());
        }
        if self.max_tool_calls == Some(0) {
            return Err("max_tool_calls must be greater than zero".to_string());
        }
        if self.max_tool_rounds == Some(0) {
            return Err("max_tool_rounds must be greater than zero".to_string());
        }
        if self.context_checkpoint_timeout_ms == 0 {
            return Err("context_checkpoint_timeout_ms must be greater than zero".to_string());
        }
        if self.deadline_ms == Some(0) {
            return Err("deadline_ms must be greater than zero".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentInputKind {
    Steering,
    FollowUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentInputQueueMode {
    All,
    OneAtATime,
}

impl Default for AgentInputQueueMode {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Debug, Default)]
struct AgentInputQueueState {
    steering: VecDeque<ContextEntry>,
    follow_up: VecDeque<ContextEntry>,
    steering_mode: AgentInputQueueMode,
    follow_up_mode: AgentInputQueueMode,
}

/// Kernel-owned input queues used while a turn is running.
#[derive(Debug, Clone, Default)]
pub struct AgentInputQueue {
    state: Arc<Mutex<AgentInputQueueState>>,
}

impl AgentInputQueue {
    pub fn enqueue(&self, kind: AgentInputKind, entry: ContextEntry) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "agent input queue is poisoned".to_string())?;
        match kind {
            AgentInputKind::Steering => state.steering.push_back(entry),
            AgentInputKind::FollowUp => state.follow_up.push_back(entry),
        }
        Ok(())
    }

    pub fn enqueue_steering(&self, entry: ContextEntry) -> Result<(), String> {
        self.enqueue(AgentInputKind::Steering, entry)
    }

    pub fn enqueue_follow_up(&self, entry: ContextEntry) -> Result<(), String> {
        self.enqueue(AgentInputKind::FollowUp, entry)
    }

    pub fn set_modes(
        &self,
        steering: AgentInputQueueMode,
        follow_up: AgentInputQueueMode,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "agent input queue is poisoned".to_string())?;
        state.steering_mode = steering;
        state.follow_up_mode = follow_up;
        Ok(())
    }

    pub fn take(&self, kind: AgentInputKind) -> Result<Vec<ContextEntry>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "agent input queue is poisoned".to_string())?;
        let mode = match kind {
            AgentInputKind::Steering => state.steering_mode,
            AgentInputKind::FollowUp => state.follow_up_mode,
        };
        let queue = match kind {
            AgentInputKind::Steering => &mut state.steering,
            AgentInputKind::FollowUp => &mut state.follow_up,
        };
        if mode == AgentInputQueueMode::OneAtATime {
            return Ok(queue.pop_front().into_iter().collect());
        }
        Ok(queue.drain(..).collect())
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "agent input queue is poisoned".to_string())?;
        state.steering.clear();
        state.follow_up.clear();
        Ok(())
    }

    pub fn is_empty(&self) -> Result<bool, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "agent input queue is poisoned".to_string())?;
        Ok(state.steering.is_empty() && state.follow_up.is_empty())
    }
}

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

    pub fn with_deadline(&self, deadline: Duration) -> Self {
        let parent = self.clone();
        let expires_at = Instant::now() + deadline;
        Self::new(move || parent.is_cancelled() || Instant::now() >= expires_at)
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
    pub version: crate::tools::ToolVersion,
    pub source: crate::tools::ToolSource,
    pub concurrency: crate::tools::ConcurrencyPolicy,
    pub timeout: std::time::Duration,
    pub idempotent: bool,
    pub retryable: bool,
}

impl ToolRegistration {
    pub fn new(spec: ToolSpec, read_only: bool, handler: Arc<dyn ToolHandler>) -> Self {
        Self {
            spec,
            read_only,
            handler,
            version: crate::tools::ToolVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            source: crate::tools::ToolSource::Builtin,
            concurrency: if read_only {
                crate::tools::ConcurrencyPolicy::ParallelRead
            } else {
                crate::tools::ConcurrencyPolicy::SerialWrite
            },
            timeout: std::time::Duration::from_secs(30),
            idempotent: read_only,
            retryable: read_only,
        }
    }

    /// Validate the Runtime registration through the shared tool definition
    /// contract. The runtime keeps the handler here, while the registry owns
    /// identity, schema, source, and lifecycle metadata validation.
    pub fn validate(&self) -> Result<(), String> {
        self.spec.validate_schema()?;
        let definition = self.definition()?;
        let mut registry = crate::tools::ToolRegistry::new();
        registry
            .register(definition)
            .map_err(|error| error.to_string())
    }

    pub fn definition(&self) -> Result<crate::tools::ToolDefinition, String> {
        let id = crate::tools::ToolId::new(self.spec.id.clone())
            .map_err(|error| format!("invalid tool id {}: {error}", self.spec.id))?;
        Ok(crate::tools::ToolDefinition {
            id,
            version: self.version,
            name: self.spec.name.clone(),
            description: format!("Bloomery tool {}", self.spec.name),
            input_schema: self.spec.input_schema.clone(),
            output_schema: serde_json::json!({"type": "object"}),
            risk: self.spec.risk,
            read_only: self.read_only,
            concurrency: self.concurrency,
            timeout: self.timeout,
            source: self.source.clone(),
            domains: BTreeSet::new(),
        })
    }
}

/// Immutable tool set captured at the beginning of a model/tool turn.
/// Handlers are cloned by `Arc`; metadata and ordering cannot change while a
/// turn is executing or being recovered.
#[derive(Clone)]
pub struct RuntimeToolSnapshot {
    registrations: Vec<ToolRegistration>,
}

impl RuntimeToolSnapshot {
    pub fn capture(registrations: &[ToolRegistration]) -> Result<Self, String> {
        let mut registrations = registrations.to_vec();
        let mut ids = BTreeSet::new();
        for registration in &registrations {
            registration.validate()?;
            if !ids.insert(registration.spec.id.clone()) {
                return Err(format!(
                    "tool id is already registered: {}",
                    registration.spec.id
                ));
            }
        }
        registrations.sort_by(|left, right| {
            left.spec
                .id
                .cmp(&right.spec.id)
                .then_with(|| left.spec.name.cmp(&right.spec.name))
        });
        Ok(Self { registrations })
    }

    pub fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }
}

pub trait ToolHandler: Send + Sync {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture;
}

pub trait ToolExecutor: Send + Sync {
    fn registrations(&self) -> &[ToolRegistration];

    fn snapshot(&self) -> Result<RuntimeToolSnapshot, String> {
        RuntimeToolSnapshot::capture(self.registrations())
    }

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

/// 附着在 agent 请求上的多模态附件(如图片),由前端以 base64 提供。
#[derive(Debug, Clone)]
pub struct AgentLoopAttachment {
    pub data: String,
    pub mime: String,
    pub name: String,
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
    pub attachments: Vec<AgentLoopAttachment>,
    pub limits: AgentLoopLimits,
    pub input_queue: AgentInputQueue,
    pub resume: Option<AgentLoopResume>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextCheckpointReason {
    ModelCall,
    AssistantResult,
    AssistantError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContextCheckpoint {
    pub reason: ContextCheckpointReason,
    pub model_call_index: usize,
    pub model_calls: usize,
    pub tool_calls: usize,
    pub tool_round: usize,
    pub recovery_attempt: usize,
    pub messages: Vec<crate::providers::capabilities::ChatMessage>,
}

/// Inputs needed to continue a non-terminal run from a durable checkpoint.
/// The checkpoint owns the provider-facing message view while the state keeps
/// the persisted RunStateMachine aligned with the next legal transition.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentLoopResume {
    pub checkpoint: AgentContextCheckpoint,
    pub state: AgentRunState,
    pub assistant_result_recorded: bool,
    pub pending_tools: Vec<ResumableToolCall>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResumableToolCall {
    pub tool_call_id: Uuid,
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub permission_id: Option<Uuid>,
    pub decision: Option<PermissionDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLoopResult {
    pub outcome: RunOutcome,
    pub answer: String,
    pub reasoning: String,
    pub reasoning_ms: u64,
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

    fn checkpoint(&mut self, _checkpoint: AgentContextCheckpoint) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum AgentLoopError {
    Context(crate::agent::context::ContextBudgetError),
    Provider(ProviderError),
    Capability(String),
    ToolRepair(crate::agent::tool_repair::ToolRepairError),
    Tool(String),
    Citation(String),
    Limit {
        kind: &'static str,
        limit: usize,
        observed: usize,
    },
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
            Self::Limit {
                kind,
                limit,
                observed,
            } => write!(
                formatter,
                "agent loop limit exceeded: {kind} observed {observed}, limit {limit}"
            ),
            Self::ToolRepair(error) => write!(formatter, "tool repair: {error}"),
        }
    }
}

impl std::error::Error for AgentLoopError {}

pub struct AgentLoop<'a, M: ?Sized, T: ?Sized, P: ?Sized> {
    pub(super) model: &'a M,
    pub(super) tools: &'a T,
    pub(super) permissions: &'a P,
    pub(super) hooks: &'a dyn AgentHooks,
    pub(super) artifact_store: Option<&'a dyn crate::tools::ArtifactStore>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HookDecision {
    Continue,
    Replace(Value),
    Block(String),
}

pub trait AgentHooks: Send + Sync {
    fn before_model(&self) -> Option<crate::providers::capabilities::ChatMessage> {
        None
    }

    fn after_tool_round(&self, _tool_names: &[String]) {}

    fn pre_tool_use(&self, _call: &ToolInvocation) -> Result<HookDecision, String> {
        Ok(HookDecision::Continue)
    }

    fn post_tool_use(
        &self,
        _call: &ToolInvocation,
        _result: &Result<Value, ToolExecutionError>,
    ) -> Result<HookDecision, String> {
        Ok(HookDecision::Continue)
    }
}

pub struct NoopAgentHooks;

impl AgentHooks for NoopAgentHooks {}

#[derive(Debug, Clone)]
pub(super) struct PreparedToolCall {
    pub(super) model_call_id: String,
    pub(super) tool_call_id: Uuid,
    pub(super) tool_id: String,
    pub(super) tool_name: String,
    pub(super) arguments: Value,
    pub(super) risk: PermissionRisk,
    pub(super) concurrency: crate::tools::ConcurrencyPolicy,
    pub(super) timeout: std::time::Duration,
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
