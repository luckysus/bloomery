use crate::agent::protocol::{AgentEventData, AgentEventEnvelope, RunOutcome};
use serde_json::{json, Value};
use uuid::Uuid;

pub(super) fn details(
    child_turn_id: Uuid,
    outcome: Option<RunOutcome>,
    events: &[AgentEventEnvelope],
) -> Value {
    let evidence = events
        .iter()
        .filter_map(|event| match &event.data {
            AgentEventData::EvidenceAttached(evidence) => Some(json!({
                "evidence_pack_id": evidence.evidence_pack_id,
                "citation_numbers": evidence.citation_numbers,
            })),
            _ => None,
        })
        .collect::<Vec<_>>();
    let errors = events
        .iter()
        .filter_map(|event| match &event.data {
            AgentEventData::ErrorRaised(error) => serde_json::to_value(error).ok(),
            _ => None,
        })
        .collect::<Vec<_>>();
    json!({
        "child_turn_id": child_turn_id,
        "outcome": outcome,
        "evidence": evidence,
        "errors": errors,
    })
}

pub(super) fn completed(
    child_turn_id: Uuid,
    agent_result: &crate::agent::runtime::AgentLoopResult,
    events: &[AgentEventEnvelope],
) -> Value {
    let mut summary = details(child_turn_id, Some(agent_result.outcome), events);
    summary["conclusion"] = Value::String(agent_result.answer.clone());
    summary
}
