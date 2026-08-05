use super::model::{DesktopIntentKind, DesktopRoute};
use serde_json::{json, Value};

pub fn classify_desktop_intent(message: &str) -> DesktopRoute {
    let text = message.trim();
    let lower = text.to_lowercase();
    if text.is_empty() {
        return route(DesktopIntentKind::Clarify, 1.0, "empty message");
    }
    if has_any(
        &lower,
        &[
            "training",
            "retrain",
            "fine tune",
            "finetune",
            "训练",
            "模型训练",
        ],
    ) && has_any(&lower, &["start", "run", "execute", "开始", "启动", "重新"])
    {
        return route(
            DesktopIntentKind::TrainingTask,
            0.9,
            "explicit training task request",
        );
    }
    if has_any(&lower, &["pdf", "literature", "文献", "解析", "入库"])
        && has_any(
            &lower,
            &["upload", "process", "parse", "上传", "处理", "解析", "入库"],
        )
    {
        return route(
            DesktopIntentKind::LiteratureTask,
            0.9,
            "explicit literature processing request",
        );
    }
    if has_any(&lower, &["optimize", "optimization", "优化", "工艺优化"])
        && has_any(&lower, &["start", "run", "execute", "开始", "启动", "运行"])
    {
        return route(
            DesktopIntentKind::OptimizationTask,
            0.88,
            "explicit optimization task request",
        );
    }
    if has_any(
        &lower,
        &[
            "knowledge base",
            "literature",
            "paper",
            "search",
            "rag",
            "标准",
            "文献",
            "检索",
            "知识库",
        ],
    ) {
        return route(
            DesktopIntentKind::KnowledgeQa,
            0.86,
            "knowledge or citation retrieval requested",
        );
    }
    if has_any(
        &lower,
        &[
            "optimize",
            "optimization",
            "process",
            "yield",
            "tensile",
            "工艺",
            "热轧",
            "屈服",
            "强度",
            "成分",
        ],
    ) {
        return route(
            DesktopIntentKind::OptimizationAdvice,
            0.74,
            "steel process advice question",
        );
    }
    route(DesktopIntentKind::LocalQa, 0.7, "default local desktop QA")
}

fn route(intent: DesktopIntentKind, confidence: f32, reason: &'static str) -> DesktopRoute {
    let unavailable_capability = match intent {
        DesktopIntentKind::KnowledgeQa => Some("local_rag"),
        DesktopIntentKind::TrainingTask => Some("local_training"),
        DesktopIntentKind::OptimizationTask => Some("local_optimization"),
        DesktopIntentKind::LiteratureTask => Some("local_literature"),
        _ => None,
    };
    DesktopRoute {
        intent,
        confidence,
        reason,
        unavailable_capability,
    }
}

pub fn route_to_json(route: &DesktopRoute) -> Value {
    json!({
        "intent": route.intent.as_str(),
        "confidence": route.confidence,
        "reason": route.reason,
        "unavailable_capability": route.unavailable_capability,
    })
}

pub fn build_agent_response_json(
    run_id: &str,
    conversation_id: &str,
    answer: &str,
    config_provider: &str,
    config_model: &str,
    has_api_key: bool,
    route: &DesktopRoute,
) -> Value {
    json!({
        "run_id": run_id,
        "session_id": conversation_id,
        "status": "completed",
        "answer": answer,
        "follow_up_questions": [],
        "plan_steps": [],
        "tool_calls": [],
        "evidence": [],
        "recommendations": [],
        "verification": {
            "confidence": "medium",
            "citation_count": 0,
            "missing_citations": [],
            "numeric_warnings": [],
            "unsupported_claims": [],
            "summary": "Generated locally from the current desktop context."
        },
        "memory": {"session_id": conversation_id, "notes": []},
        "pending_confirmations": [],
        "intent": {
            "intent_type": route.intent.as_str(),
            "domain": "steel",
            "risk_level": "medium",
            "needs_evidence": false,
            "needs_tools": [],
            "unavailable_capability": route.unavailable_capability,
            "missing_slots": [],
            "answer_policy": "desktop_local",
            "reason": route.reason
        },
        "workflow_trace": {
            "route": "desktop_local_agent",
            "tools_selected": [],
            "tools_skipped": [],
            "evidence_policy": "local_context_first",
            "answer_policy": "desktop_local",
            "model_provider": config_provider,
            "model_name": config_model,
            "notes": ["Context is assembled from local SQLite data."]
        },
        "workflow": {
            "run_id": run_id,
            "state": "completed",
            "nodes": [],
            "edges": [],
            "events": [],
            "summary": "desktop local agent",
            "started_at": null,
            "ended_at": null
        },
        "llm_config": {
            "provider": config_provider,
            "base_url": "",
            "model_name": config_model,
            "has_api_key": has_api_key
        }
    })
}

pub fn build_capability_unavailable_response_json(
    run_id: &str,
    conversation_id: &str,
    route: &DesktopRoute,
) -> Value {
    let capability = route.unavailable_capability.unwrap_or("unknown");
    json!({
        "run_id": run_id,
        "session_id": conversation_id,
        "status": "capability_unavailable",
        "answer": format!("Local capability is not available yet: {capability}."),
        "follow_up_questions": [],
        "plan_steps": [],
        "tool_calls": [],
        "evidence": [],
        "recommendations": [],
        "verification": {"confidence": "high", "citation_count": 0, "missing_citations": [], "numeric_warnings": [], "unsupported_claims": [], "summary": "The requested local capability is unavailable."},
        "memory": {"session_id": conversation_id, "notes": []},
        "pending_confirmations": [],
        "intent": {"intent_type": route.intent.as_str(), "domain": "steel", "risk_level": "low", "needs_evidence": false, "needs_tools": [], "unavailable_capability": capability, "missing_slots": [], "answer_policy": "capability_unavailable", "reason": route.reason},
        "workflow_trace": {"route": "desktop_local_agent", "tools_selected": [], "tools_skipped": [capability], "evidence_policy": "none", "answer_policy": "capability_unavailable", "model_provider": "", "model_name": "", "notes": []},
        "workflow": {"run_id": run_id, "state": "blocked", "nodes": [], "edges": [], "events": [], "summary": "local capability unavailable", "started_at": null, "ended_at": null},
        "llm_config": {"provider": "", "base_url": "", "model_name": "", "has_api_key": false}
    })
}

fn has_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}
