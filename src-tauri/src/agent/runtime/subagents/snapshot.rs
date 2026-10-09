use super::super::{
    AgentLoopLimits, CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture,
    ToolInvocation, ToolRegistration,
};
use std::collections::BTreeSet;

pub struct SnapshotToolExecutor {
    registrations: Vec<ToolRegistration>,
}

impl SnapshotToolExecutor {
    pub fn from(source: &dyn ToolExecutor) -> Self {
        Self {
            registrations: source
                .registrations()
                .iter()
                .filter(|registration| {
                    registration.spec.id != super::TASK_TOOL_ID
                        && registration.spec.name != super::TASK_TOOL_NAME
                })
                .cloned()
                .collect(),
        }
    }

    pub fn intersect(profile: &dyn ToolExecutor, parent: &dyn ToolExecutor) -> Self {
        let mut snapshot = Self::from(profile);
        snapshot.registrations.retain(|registration| {
            parent.registrations().iter().any(|allowed| {
                allowed.spec.id == registration.spec.id
                    && allowed.spec.name == registration.spec.name
                    && allowed.version == registration.version
                    && allowed.source == registration.source
                    && allowed.spec.risk == registration.spec.risk
                    && allowed.read_only == registration.read_only
            })
        });
        snapshot
    }

    pub fn restrict_to_ids(&mut self, ids: &[String]) {
        let ids = ids.iter().collect::<BTreeSet<_>>();
        self.registrations
            .retain(|registration| ids.contains(&registration.spec.id));
    }
}

impl ToolExecutor for SnapshotToolExecutor {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        let Some(registration) = self.registrations.iter().find(|registration| {
            registration.spec.id == invocation.tool_id
                && registration.spec.name == invocation.tool_name
        }) else {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "tool is not registered",
                ))
            });
        };
        registration
            .handler
            .execute_for_invocation(invocation, cancellation)
    }
}

pub fn child_limits(
    parent: Option<&AgentLoopLimits>,
    profile: Option<&AgentLoopLimits>,
) -> AgentLoopLimits {
    let mut limits = parent.cloned().unwrap_or_default();
    let profile = profile.cloned().unwrap_or_else(|| limits.clone());
    fn bounded(left: Option<usize>, right: Option<usize>, cap: usize) -> Option<usize> {
        Some(left.unwrap_or(cap).min(right.unwrap_or(cap)).min(cap))
    }
    limits.max_model_calls = bounded(
        limits.max_model_calls,
        profile.max_model_calls,
        super::MAX_SUBAGENT_MODEL_CALLS,
    );
    limits.max_tool_calls = bounded(
        limits.max_tool_calls,
        profile.max_tool_calls,
        super::MAX_SUBAGENT_TOOL_CALLS,
    );
    limits.max_tool_rounds = bounded(
        limits.max_tool_rounds,
        profile.max_tool_rounds,
        super::MAX_SUBAGENT_TOOL_ROUNDS,
    );
    limits.context_budget = match (limits.context_budget, profile.context_budget) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    };
    limits.max_recovery_attempts = limits
        .max_recovery_attempts
        .min(profile.max_recovery_attempts);
    limits.save_checkpoints &= profile.save_checkpoints;
    limits.stream_output &= profile.stream_output;
    limits.deadline_ms = match (limits.deadline_ms, profile.deadline_ms) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    };
    limits
}
