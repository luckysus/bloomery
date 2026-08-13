use super::service::build_agent_loop_request;
use super::{
    assistant_content_for_stream_result,
    cancellation::LocalAgentState,
    model::{DesktopIntentKind, LocalAgentChatRequest, StreamedLlmAnswer},
    prompt::{build_desktop_context_prompt_for_domains, build_local_ask_prompt},
    routing::classify_desktop_intent,
};
use serde_json::json;
use uuid::Uuid;

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
fn cloned_cancellation_state_observes_the_same_run() {
    let state = LocalAgentState::default();
    let clone = state.clone();

    state.cancel_run("run-a").expect("cancel");

    assert!(clone.is_cancelled("run-a").expect("shared cancellation"));
}

#[test]
fn cancellation_token_reads_the_shared_run_state() {
    let state = LocalAgentState::default();
    let token = state.cancellation_token("run-a");
    assert!(!token.is_cancelled());

    state.cancel_run("run-a").expect("cancel");

    assert!(token.is_cancelled());
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

#[test]
fn chat_request_accepts_evidence_pack_reference() {
    let request: LocalAgentChatRequest = serde_json::from_value(json!({
        "message": "Q355B strength",
        "evidencePackId": "audit-1"
    }))
    .expect("deserialize desktop chat request");

    assert_eq!(request.evidence_pack_id.as_deref(), Some("audit-1"));
}

#[test]
fn desktop_prompt_includes_bounded_evidence_context() {
    let prompt = build_desktop_context_prompt_for_domains(
        &json!({
            "evidence_pack": {
                "id": "audit-1",
                "evidence": [{
                    "citation_number": 1,
                    "chunk": {
                        "source_name": "GB 50017",
                        "source_location": {"kind": "pdf_page", "page": 12, "bbox": null},
                        "text": "Q355B has a nominal yield strength of 355 MPa."
                    }
                }]
            }
        }),
        &[],
    );

    assert!(prompt.contains("evidence_pack:"));
    assert!(prompt.contains("355 MPa"));
}

#[test]
fn desktop_prompt_includes_enabled_skills() {
    let prompt = build_desktop_context_prompt_for_domains(
        &json!({
            "skills": {
                "enabled_versions": ["steel-review@1.0.0#abc123"],
                "prompt": "enabled_skills:\n\n## steel-review (v1.0.0)\nUse source evidence."
            }
        }),
        &[],
    );

    assert!(prompt.contains("skills:"));
    assert!(prompt.contains("steel-review@1.0.0#abc123"));
    assert!(prompt.contains("Use source evidence."));
}

#[test]
fn desktop_prompt_injects_active_domain_manifest() {
    let manifest: crate::domains::DomainManifest = serde_json::from_value(json!({
        "id": "steel",
        "version": "1.0.0",
        "compatibility": {"min_app_version": "0.1.0", "max_app_version": null},
        "author": "Bloomery contributors",
        "license": "Apache-2.0",
        "prompts": {"system": "Use steel terminology.", "workflow": "Cite the source."},
        "terminology": {"Q355B": "Chinese structural steel grade"},
        "retrieval": {"required_tags": [], "citation_required": true, "max_evidence_items": 8}
    }))
    .expect("deserialize manifest");

    let prompt =
        build_desktop_context_prompt_for_domains(&json!({}), std::slice::from_ref(&manifest));

    assert!(prompt.contains("domain_system:\nUse steel terminology."));
    assert!(prompt.contains("domain_workflow:\nCite the source."));
    assert!(prompt.contains("domain_terminology:"));
    assert!(prompt.contains("- Q355B: Chinese structural steel grade"));
    assert!(prompt.contains("domain_citation_policy:"));
}

#[test]
fn desktop_prompt_includes_all_active_domain_manifests() {
    let steel: crate::domains::DomainManifest = serde_json::from_value(json!({
        "id": "steel",
        "version": "1.0.0",
        "compatibility": {"min_app_version": "0.1.0", "max_app_version": null},
        "author": "Bloomery contributors",
        "license": "Apache-2.0",
        "prompts": {"system": "Use steel terminology.", "workflow": "Cite steel sources."},
        "terminology": {"Q355B": "Structural steel grade"},
        "retrieval": {"required_tags": [], "citation_required": true, "max_evidence_items": 8}
    }))
    .expect("deserialize steel manifest");
    let materials: crate::domains::DomainManifest = serde_json::from_value(json!({
        "id": "materials",
        "version": "1.0.0",
        "compatibility": {"min_app_version": "0.1.0", "max_app_version": null},
        "author": "Bloomery contributors",
        "license": "Apache-2.0",
        "prompts": {"system": "Use materials terminology.", "workflow": "Cite materials sources."},
        "terminology": {"YS": "Yield strength"},
        "retrieval": {"required_tags": [], "citation_required": true, "max_evidence_items": 6}
    }))
    .expect("deserialize materials manifest");

    let prompt =
        super::prompt::build_desktop_context_prompt_for_domains(&json!({}), &[steel, materials]);

    assert!(prompt.contains("Use steel terminology."));
    assert!(prompt.contains("Use materials terminology."));
    assert!(prompt.contains("Q355B: Structural steel grade"));
    assert!(prompt.contains("YS: Yield strength"));
}

#[test]
fn desktop_prompt_omits_domain_sections_without_active_package() {
    let prompt = build_desktop_context_prompt_for_domains(&json!({}), &[]);

    assert!(!prompt.contains("domain_system:"));
    assert!(!prompt.contains("domain_terminology:"));
    assert!(!prompt.contains("domain_citation_policy:"));
}

#[test]
fn desktop_chat_builds_a_standard_agent_loop_request() {
    let request = build_agent_loop_request(
        Uuid::new_v4(),
        "system prompt",
        "calculate carbon equivalent",
        None,
    );

    assert_eq!(request.context.len(), 2);
    assert_eq!(request.context[0].item.content, "system prompt");
    assert_eq!(
        request.context[1].item.content,
        "calculate carbon equivalent"
    );
    assert!(matches!(
        request.context[0].item.source,
        crate::agent::context::ContextSource::System
    ));
    assert!(matches!(
        request.context[1].item.source,
        crate::agent::context::ContextSource::CurrentRequest
    ));
    assert!(request.evidence.is_none());
}
