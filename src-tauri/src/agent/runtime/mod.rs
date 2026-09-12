mod composite;
mod domain_tools;
mod r#loop;
pub mod model_adapter;
pub mod persistence;
pub mod recovery;
mod skills_tool;
pub mod state_machine;
mod subagents;
mod tasks_tool;
mod todos;

pub use composite::CompositeToolExecutor;
pub use domain_tools::DomainToolExecutor;
pub use model_adapter::{ModelAdapter, ModelFuture, ProviderModelAdapter};
pub use persistence::{AgentEventPublisher, NoopAgentEventPublisher, SqliteAgentEventSink};
pub use r#loop::{
    AgentEventSink, AgentHooks, AgentLoop, AgentLoopAttachment, AgentLoopError, AgentLoopRequest,
    AgentLoopResult, CancellationToken, ContextEntry, DenyPermissions, EvidenceAttachment,
    HookDecision, NoopAgentHooks, NoopToolExecutor, PermissionFuture, PermissionRequest,
    PermissionResolver, ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation,
    ToolRegistration,
};
pub use recovery::{
    AgentRecoveryService, PendingPermission, RecoveredRun, RecoveryAction, RunCommandResult,
    ToolCheckpoint,
};
pub use skills_tool::SkillTool;
pub use subagents::{SnapshotToolExecutor, SubagentTool, MAX_SUBAGENT_TOOL_ROUNDS};
pub use tasks_tool::BackgroundTasksTool;
pub use todos::TodoTracker;
