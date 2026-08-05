use super::model::{
    LocalLlmConfig, StreamedLlmAnswer, LOCAL_ASK_CONTEXT_CHAR_LIMIT, LOCAL_ASK_CONTEXT_LIMIT,
    LOCAL_SUMMARY_CONTEXT_CHAR_LIMIT, LOCAL_SUMMARY_CONTEXT_LIMIT,
};
use serde_json::Value;

pub fn assistant_content_for_stream_result(answer: &StreamedLlmAnswer) -> String {
    if !answer.stopped {
        return answer.text.clone();
    }
    let text = answer.text.trim_end();
    if text.is_empty() {
        "[generation stopped]".to_string()
    } else {
        format!("{text}\n\n[generation stopped]")
    }
}

pub fn conversation_title(message: &str) -> String {
    let title = message.trim().chars().take(28).collect::<String>();
    if title.is_empty() {
        "New conversation".to_string()
    } else {
        title
    }
}

pub fn build_desktop_context_prompt(packet: &Value) -> String {
    let mut sections = vec![
        "You are Bloomery, a local-first steel research desktop agent. Prefer the local context, long-term memory, conversation history, and session summary before making claims.".to_string(),
        "Answer directly and professionally. State uncertainty when evidence is missing; never invent sources or measurements.".to_string(),
    ];
    push_json_section(
        &mut sections,
        "conversation_summary",
        packet.get("conversation_summary"),
    );
    push_json_section(
        &mut sections,
        "selected_memories",
        packet.get("selected_memories"),
    );
    push_json_section(&mut sections, "history_hits", packet.get("history_hits"));
    push_json_section(
        &mut sections,
        "recent_messages",
        packet.get("recent_messages"),
    );
    push_json_section(&mut sections, "desktop_meta", packet.get("desktop_meta"));
    sections.join("\n\n")
}

pub fn build_local_ask_prompt(query: &str, contexts: &[String], mode: &str) -> String {
    let mode = mode.trim();
    let compacted = compact_local_ask_contexts(contexts, mode);
    let mut sections = vec![
        "You are Bloomery's local desktop answer service. Use the configured local or compatible provider directly; do not rely on a Web backend.".to_string(),
        format!("mode: {mode}"),
        "Answer from the supplied context. Preserve steel grades, compositions, temperatures, units, dates, and task IDs exactly. Explain uncertainty when context is insufficient.".to_string(),
    ];
    if !compacted.is_empty() {
        sections.push(format!(
            "contexts_meta: showing {} of {}; char_limit={}",
            compacted.len(),
            contexts.len(),
            local_ask_context_char_limit(mode)
        ));
        sections.push(
            compacted
                .iter()
                .enumerate()
                .map(|(index, item)| format!("[context {}]\n{}", index + 1, item))
                .collect::<Vec<_>>()
                .join("\n\n"),
        );
    }
    sections.push(format!("user question:\n{}", query.trim()));
    sections.join("\n\n")
}

fn compact_local_ask_contexts(contexts: &[String], mode: &str) -> Vec<String> {
    let limit = if mode.trim() == "summary" {
        LOCAL_SUMMARY_CONTEXT_LIMIT
    } else {
        LOCAL_ASK_CONTEXT_LIMIT
    };
    let char_limit = local_ask_context_char_limit(mode);
    contexts
        .iter()
        .take(limit)
        .map(|item| truncate_text(item, char_limit))
        .collect()
}

fn local_ask_context_char_limit(mode: &str) -> usize {
    if mode.trim() == "summary" {
        LOCAL_SUMMARY_CONTEXT_CHAR_LIMIT
    } else {
        LOCAL_ASK_CONTEXT_CHAR_LIMIT
    }
}

fn push_json_section(sections: &mut Vec<String>, title: &str, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    if value.is_null() || value == "" || value.as_array().is_some_and(|items| items.is_empty()) {
        return;
    }
    sections.push(format!("{title}:\n{value}"));
}

pub(crate) fn truncate_text(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push('\u{2026}');
    }
    result
}

#[allow(dead_code)]
fn _config_is_local(config: &LocalLlmConfig) -> bool {
    config.provider.eq_ignore_ascii_case("ollama")
}
