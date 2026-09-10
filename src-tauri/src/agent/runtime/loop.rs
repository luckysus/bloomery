mod execution;
mod generation;
mod helpers;
mod types;

pub use types::{
    AgentEventSink, AgentLoop, AgentLoopAttachment, AgentLoopError, AgentLoopRequest,
    AgentLoopResult, CancellationToken, ContextEntry, DenyPermissions, EvidenceAttachment,
    AgentHooks, HookDecision, NoopAgentHooks, NoopToolExecutor, PermissionFuture,
    PermissionRequest, PermissionResolver, ToolExecutionError, ToolExecutor, ToolFuture,
    ToolHandler, ToolInvocation, ToolRegistration,
};

use types::AgentLoop as AgentLoopType;

impl<'a, M: ?Sized, T: ?Sized, P: ?Sized> AgentLoopType<'a, M, T, P> {
    pub fn new(model: &'a M, tools: &'a T, permissions: &'a P) -> Self {
        Self {
            model,
            tools,
            permissions,
            hooks: &NoopAgentHooks,
            artifact_store: None,
        }
    }

    pub fn new_with_hooks(
        model: &'a M,
        tools: &'a T,
        permissions: &'a P,
        hooks: &'a dyn types::AgentHooks,
    ) -> Self {
        Self {
            model,
            tools,
            permissions,
            hooks,
            artifact_store: None,
        }
    }

    pub fn new_with_hooks_and_artifact_store(
        model: &'a M,
        tools: &'a T,
        permissions: &'a P,
        hooks: &'a dyn types::AgentHooks,
        artifact_store: &'a dyn crate::tools::ArtifactStore,
    ) -> Self {
        Self {
            model,
            tools,
            permissions,
            hooks,
            artifact_store: Some(artifact_store),
        }
    }
}
