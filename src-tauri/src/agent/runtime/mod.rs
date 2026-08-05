mod r#loop;
pub mod model_adapter;
pub mod persistence;
pub mod recovery;
pub mod state_machine;

pub use model_adapter::{ModelAdapter, ModelFuture, ProviderModelAdapter};
pub use persistence::{AgentEventPublisher, NoopAgentEventPublisher, SqliteAgentEventSink};
pub use r#loop::{
    AgentEventSink, AgentLoop, AgentLoopError, AgentLoopRequest, AgentLoopResult,
    CancellationToken, ContextEntry, DenyPermissions, EvidenceAttachment, NoopToolExecutor,
    PermissionRequest, PermissionResolver, ToolExecutionError, ToolExecutor, ToolFuture,
    ToolHandler, ToolInvocation, ToolRegistration,
};
pub use recovery::{
    AgentRecoveryService, PendingPermission, RecoveredRun, RecoveryAction, RunCommandResult,
    ToolCheckpoint,
};
