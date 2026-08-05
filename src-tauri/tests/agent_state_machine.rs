use bloomery::agent::protocol::AgentRunState;
use bloomery::agent::runtime::state_machine::{RunGuards, RunStateMachine};

const NONTERMINAL_STATES: [AgentRunState; 7] = [
    AgentRunState::Created,
    AgentRunState::Preparing,
    AgentRunState::Generating,
    AgentRunState::AwaitingPermission,
    AgentRunState::ExecutingTools,
    AgentRunState::Verifying,
    AgentRunState::Completing,
];

const TERMINAL_STATES: [AgentRunState; 4] = [
    AgentRunState::Completed,
    AgentRunState::Cancelled,
    AgentRunState::Failed,
    AgentRunState::Interrupted,
];

const ALL_STATES: [AgentRunState; 11] = [
    AgentRunState::Created,
    AgentRunState::Preparing,
    AgentRunState::Generating,
    AgentRunState::AwaitingPermission,
    AgentRunState::ExecutingTools,
    AgentRunState::Verifying,
    AgentRunState::Completing,
    AgentRunState::Completed,
    AgentRunState::Cancelled,
    AgentRunState::Failed,
    AgentRunState::Interrupted,
];

#[test]
fn permissioned_tool_run_follows_the_complete_state_path() {
    let mut machine = RunStateMachine::new();

    transition(&mut machine, AgentRunState::Preparing, RunGuards::default());
    transition(
        &mut machine,
        AgentRunState::Generating,
        RunGuards::default(),
    );
    transition(
        &mut machine,
        AgentRunState::AwaitingPermission,
        guards(1, 1, 0),
    );
    transition(&mut machine, AgentRunState::ExecutingTools, guards(1, 0, 1));
    transition(&mut machine, AgentRunState::Verifying, RunGuards::default());
    transition(
        &mut machine,
        AgentRunState::Completing,
        RunGuards::default(),
    );
    transition(&mut machine, AgentRunState::Completed, RunGuards::default());

    assert_eq!(machine.state(), AgentRunState::Completed);
}

#[test]
fn direct_answer_can_skip_permission_and_tool_states() {
    let mut machine = RunStateMachine::new();

    transition(&mut machine, AgentRunState::Preparing, RunGuards::default());
    transition(
        &mut machine,
        AgentRunState::Generating,
        RunGuards::default(),
    );
    transition(&mut machine, AgentRunState::Verifying, RunGuards::default());
    transition(
        &mut machine,
        AgentRunState::Completing,
        RunGuards::default(),
    );
    transition(&mut machine, AgentRunState::Completed, RunGuards::default());

    assert_eq!(machine.state(), AgentRunState::Completed);
}

#[test]
fn transition_matrix_accepts_only_declared_nonterminal_edges() {
    for previous in NONTERMINAL_STATES {
        for target in ALL_STATES {
            let mut machine = RunStateMachine::restore(previous);
            let result = machine.transition(target, valid_guards(previous, target));

            assert_eq!(
                result.is_ok(),
                declared_edge(previous, target),
                "unexpected transition result for {previous:?} -> {target:?}"
            );
            if let Ok(changed) = result {
                assert_eq!(changed.previous, previous);
                assert_eq!(changed.current, target);
                assert_eq!(machine.state(), target);
            } else {
                assert_eq!(machine.state(), previous);
            }
        }
    }
}

#[test]
fn every_terminal_state_rejects_every_exit_and_keeps_its_state() {
    for terminal in TERMINAL_STATES {
        for target in ALL_STATES {
            let mut machine = RunStateMachine::restore(terminal);

            let error = machine
                .transition(target, valid_guards(terminal, target))
                .unwrap_err();

            assert_eq!(error.code(), "invalid_run_transition");
            assert_eq!(machine.state(), terminal);
        }
    }
}

#[test]
fn entering_permission_wait_requires_a_pending_permission() {
    let mut machine = RunStateMachine::restore(AgentRunState::Generating);

    let error = machine
        .transition(AgentRunState::AwaitingPermission, guards(1, 0, 0))
        .unwrap_err();

    assert_eq!(error.code(), "invalid_run_transition");
    assert_eq!(machine.state(), AgentRunState::Generating);
}

#[test]
fn pending_permission_blocks_every_exit_from_permission_wait() {
    for target in [AgentRunState::Generating, AgentRunState::ExecutingTools] {
        let mut machine = RunStateMachine::restore(AgentRunState::AwaitingPermission);

        let error = machine
            .transition(
                target,
                guards(1, 1, usize::from(target == AgentRunState::ExecutingTools)),
            )
            .unwrap_err();

        assert_eq!(error.code(), "invalid_run_transition");
        assert_eq!(machine.state(), AgentRunState::AwaitingPermission);
    }
}

#[test]
fn denied_permission_cannot_execute_the_tool_and_returns_to_generation() {
    let mut machine = RunStateMachine::restore(AgentRunState::AwaitingPermission);

    let error = machine
        .transition(AgentRunState::ExecutingTools, guards(1, 0, 0))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_run_transition");
    assert_eq!(machine.state(), AgentRunState::AwaitingPermission);

    transition(&mut machine, AgentRunState::Generating, guards(1, 0, 0));
}

#[test]
fn automatic_tool_can_execute_without_a_permission_round_trip() {
    let mut machine = RunStateMachine::restore(AgentRunState::Generating);

    transition(&mut machine, AgentRunState::ExecutingTools, guards(1, 0, 1));
}

#[test]
fn pending_permission_blocks_automatic_tool_execution_in_a_mixed_batch() {
    let mut machine = RunStateMachine::restore(AgentRunState::Generating);

    let error = machine
        .transition(AgentRunState::ExecutingTools, guards(2, 1, 1))
        .unwrap_err();

    assert_eq!(error.code(), "invalid_run_transition");
    assert_eq!(machine.state(), AgentRunState::Generating);
}

#[test]
fn tool_observation_can_return_execution_to_generation() {
    let mut machine = RunStateMachine::restore(AgentRunState::ExecutingTools);

    transition(
        &mut machine,
        AgentRunState::Generating,
        RunGuards::default(),
    );
}

#[test]
fn completion_rejects_unresolved_calls_and_permissions() {
    let mut verifying = RunStateMachine::restore(AgentRunState::Verifying);
    let pending_call = verifying
        .transition(AgentRunState::Completing, guards(1, 0, 0))
        .unwrap_err();
    assert_eq!(pending_call.code(), "invalid_run_transition");
    assert_eq!(verifying.state(), AgentRunState::Verifying);

    let mut completing = RunStateMachine::restore(AgentRunState::Completing);
    let pending_permission = completing
        .transition(AgentRunState::Completed, guards(1, 1, 0))
        .unwrap_err();
    assert_eq!(pending_permission.code(), "invalid_run_transition");
    assert_eq!(completing.state(), AgentRunState::Completing);
}

#[test]
fn every_nonterminal_state_can_end_as_cancelled_failed_or_interrupted() {
    for previous in NONTERMINAL_STATES {
        for terminal in [
            AgentRunState::Cancelled,
            AgentRunState::Failed,
            AgentRunState::Interrupted,
        ] {
            let mut machine = RunStateMachine::restore(previous);

            transition(&mut machine, terminal, RunGuards::default());
        }
    }
}

fn guards(
    unresolved_tool_calls: usize,
    unresolved_permissions: usize,
    executable_tool_calls: usize,
) -> RunGuards {
    RunGuards {
        unresolved_tool_calls,
        unresolved_permissions,
        executable_tool_calls,
    }
}

fn valid_guards(previous: AgentRunState, target: AgentRunState) -> RunGuards {
    match (previous, target) {
        (AgentRunState::Generating, AgentRunState::AwaitingPermission) => guards(1, 1, 0),
        (
            AgentRunState::Generating | AgentRunState::AwaitingPermission,
            AgentRunState::ExecutingTools,
        ) => guards(1, 0, 1),
        _ => RunGuards::default(),
    }
}

fn declared_edge(previous: AgentRunState, target: AgentRunState) -> bool {
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

fn transition(machine: &mut RunStateMachine, target: AgentRunState, guards: RunGuards) {
    let previous = machine.state();
    let changed = machine.transition(target, guards).unwrap();
    assert_eq!(changed.previous, previous);
    assert_eq!(changed.current, target);
    assert_eq!(machine.state(), target);
}
