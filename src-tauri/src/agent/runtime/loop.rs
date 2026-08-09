mod execution;
mod generation;
mod helpers;
mod types;

pub use types::{
    AgentEventSink, AgentLoop, AgentLoopError, AgentLoopRequest, AgentLoopResult,
    CancellationToken, ContextEntry, DenyPermissions, EvidenceAttachment, NoopToolExecutor,
    PermissionFuture, PermissionRequest, PermissionResolver, ToolExecutionError, ToolExecutor, ToolFuture,
    ToolHandler, ToolInvocation, ToolRegistration,
};

use types::AgentLoop as AgentLoopType;

impl<'a, M: ?Sized, T: ?Sized, P: ?Sized> AgentLoopType<'a, M, T, P> {
    pub fn new(model: &'a M, tools: &'a T, permissions: &'a P) -> Self {
        Self {
            model,
            tools,
            permissions,
        }
    }
}
