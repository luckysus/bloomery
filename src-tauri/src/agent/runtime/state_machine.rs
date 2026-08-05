use crate::agent::protocol::{AgentRunState, RunStateChanged};
use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunGuards {
    pub unresolved_tool_calls: usize,
    pub unresolved_permissions: usize,
    pub executable_tool_calls: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunStateMachine {
    state: AgentRunState,
}

impl Default for RunStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl RunStateMachine {
    pub const fn new() -> Self {
        Self {
            state: AgentRunState::Created,
        }
    }

    pub const fn restore(state: AgentRunState) -> Self {
        Self { state }
    }

    pub const fn state(&self) -> AgentRunState {
        self.state
    }

    pub fn transition(
        &mut self,
        target: AgentRunState,
        guards: RunGuards,
    ) -> Result<RunStateChanged, InvalidRunTransition> {
        let previous = self.state;
        if is_terminal(previous) {
            return Err(InvalidRunTransition::new(
                previous,
                target,
                "terminal runs cannot transition",
            ));
        }
        if !legal_edge(previous, target) {
            return Err(InvalidRunTransition::new(
                previous,
                target,
                "state edge is not allowed",
            ));
        }
        if target == AgentRunState::AwaitingPermission && guards.unresolved_permissions == 0 {
            return Err(InvalidRunTransition::new(
                previous,
                target,
                "no tool permission is pending",
            ));
        }
        if previous == AgentRunState::AwaitingPermission
            && target == AgentRunState::Generating
            && guards.unresolved_permissions != 0
        {
            return Err(InvalidRunTransition::new(
                previous,
                target,
                "tool permissions are unresolved",
            ));
        }
        if target == AgentRunState::ExecutingTools && guards.unresolved_permissions != 0 {
            return Err(InvalidRunTransition::new(
                previous,
                target,
                "tool permissions are unresolved",
            ));
        }
        if target == AgentRunState::ExecutingTools && guards.executable_tool_calls == 0 {
            return Err(InvalidRunTransition::new(
                previous,
                target,
                "no tool call is approved for execution",
            ));
        }
        if matches!(target, AgentRunState::Completing | AgentRunState::Completed)
            && (guards.unresolved_tool_calls != 0 || guards.unresolved_permissions != 0)
        {
            return Err(InvalidRunTransition::new(
                previous,
                target,
                "run work is unresolved",
            ));
        }
        self.state = target;
        Ok(RunStateChanged {
            previous,
            current: target,
            reason: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidRunTransition {
    previous: AgentRunState,
    target: AgentRunState,
    reason: &'static str,
}

impl InvalidRunTransition {
    fn new(previous: AgentRunState, target: AgentRunState, reason: &'static str) -> Self {
        Self {
            previous,
            target,
            reason,
        }
    }

    pub const fn code(&self) -> &'static str {
        "invalid_run_transition"
    }
}

impl fmt::Display for InvalidRunTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {:?} -> {:?}: {}",
            self.code(),
            self.previous,
            self.target,
            self.reason
        )
    }
}

impl std::error::Error for InvalidRunTransition {}

const fn legal_edge(previous: AgentRunState, target: AgentRunState) -> bool {
    matches!(
        (previous, target),
        (AgentRunState::Created, AgentRunState::Preparing)
            | (AgentRunState::Preparing, AgentRunState::Generating)
            | (
                AgentRunState::Generating,
                AgentRunState::AwaitingPermission
                    | AgentRunState::ExecutingTools
                    | AgentRunState::Verifying
            )
            | (
                AgentRunState::AwaitingPermission,
                AgentRunState::Generating | AgentRunState::ExecutingTools
            )
            | (
                AgentRunState::ExecutingTools,
                AgentRunState::Generating | AgentRunState::Verifying
            )
            | (
                AgentRunState::Verifying,
                AgentRunState::Generating | AgentRunState::Completing
            )
            | (AgentRunState::Completing, AgentRunState::Completed)
    ) || matches!(
        target,
        AgentRunState::Cancelled | AgentRunState::Failed | AgentRunState::Interrupted
    )
}

const fn is_terminal(state: AgentRunState) -> bool {
    matches!(
        state,
        AgentRunState::Completed
            | AgentRunState::Cancelled
            | AgentRunState::Failed
            | AgentRunState::Interrupted
    )
}
