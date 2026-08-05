use super::{
    assistant_content_for_stream_result,
    cancellation::LocalAgentState,
    model::{DesktopIntentKind, StreamedLlmAnswer},
    prompt::build_local_ask_prompt,
    routing::classify_desktop_intent,
};

#[test]
fn routes_steel_requests_without_cloud_dependencies() {
    assert_eq!(
        classify_desktop_intent("search Q355B literature").intent,
        DesktopIntentKind::KnowledgeQa
    );
    assert_eq!(
        classify_desktop_intent("run process optimization").intent,
        DesktopIntentKind::OptimizationTask
    );
    assert_eq!(
        classify_desktop_intent("what is the yield strength of Q355B?").intent,
        DesktopIntentKind::OptimizationAdvice
    );
    assert_eq!(
        classify_desktop_intent("hello").intent,
        DesktopIntentKind::LocalQa
    );
}

#[test]
fn prompt_bounds_frontend_contexts() {
    let long_context = "x".repeat(super::model::LOCAL_ASK_CONTEXT_CHAR_LIMIT + 40);
    let contexts = (0..20)
        .map(|index| format!("context-{index}-{long_context}"))
        .collect::<Vec<_>>();
    let prompt = build_local_ask_prompt("answer", &contexts, "advice");
    assert!(prompt.contains("contexts_meta: showing 12 of 20"));
    assert!(!prompt.contains("context-12"));
    assert!(prompt.contains('\u{2026}'));
}

#[test]
fn cancellation_state_is_scoped_to_run_id() {
    let state = LocalAgentState::default();
    state.cancel_run("run-a").expect("cancel");
    assert!(state.is_cancelled("run-a").expect("read cancellation"));
    assert!(!state.is_cancelled("run-b").expect("read cancellation"));
    state.clear_cancelled("run-a");
    assert!(!state.is_cancelled("run-a").expect("read cancellation"));
}

#[test]
fn stopped_answers_are_marked_partial() {
    let answer = assistant_content_for_stream_result(&StreamedLlmAnswer {
        text: "partial answer".to_string(),
        stopped: true,
    });
    assert!(answer.contains("partial answer"));
    assert!(answer.contains("generation stopped"));
}
