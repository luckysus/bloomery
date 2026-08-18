use serde::{Deserialize, Serialize};
use std::fmt;

pub const AUTO_MEMORY_WRITE_SETTING: &str = "memory.auto_write_candidates";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Pending,
    Confirmed,
    Rejected,
}

impl MemoryStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }

    pub fn from_storage(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "confirmed" => Some(Self::Confirmed),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCandidate {
    pub scope: String,
    pub memory_type: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub tags_json: String,
    pub reason: String,
    pub source_message_id: String,
    pub source_run_id: Option<String>,
    pub confidence: f64,
    pub dedup_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryCandidateError {
    InvalidConfidence,
    MissingSourceMessage,
    InvalidSourceRun,
}

impl fmt::Display for MemoryCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidConfidence => "memory confidence must be finite and between 0 and 1",
            Self::MissingSourceMessage => "memory source message is required",
            Self::InvalidSourceRun => "memory source run cannot be empty",
        })
    }
}

impl std::error::Error for MemoryCandidateError {}

pub fn extract_memory_candidate(
    content: &str,
    source_message_id: &str,
    source_run_id: Option<String>,
    confidence: f64,
) -> Result<Option<MemoryCandidate>, MemoryCandidateError> {
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(MemoryCandidateError::InvalidConfidence);
    }
    let source_message_id = source_message_id.trim();
    if source_message_id.is_empty() {
        return Err(MemoryCandidateError::MissingSourceMessage);
    }
    let source_run_id = match source_run_id {
        Some(value) if value.trim().is_empty() => {
            return Err(MemoryCandidateError::InvalidSourceRun)
        }
        Some(value) => Some(value.trim().to_string()),
        None => None,
    };
    let text = compact_whitespace(content);
    if !(4..=2_000).contains(&text.chars().count()) {
        return Ok(None);
    }
    let Some((statement, reason)) = extract_statement(&text) else {
        return Ok(None);
    };
    let dedup_key = normalize_memory_key(&statement);
    if dedup_key.is_empty() {
        return Ok(None);
    }
    let memory_type = infer_type(&statement);
    let scope = infer_scope(&statement, memory_type);
    Ok(Some(MemoryCandidate {
        scope: scope.to_string(),
        memory_type: memory_type.to_string(),
        title: truncate_chars(&statement, 64),
        description: statement.clone(),
        body: statement,
        tags_json: "[\"suggested\"]".to_string(),
        reason,
        source_message_id: source_message_id.to_string(),
        source_run_id,
        confidence,
        dedup_key,
    }))
}

pub fn normalize_memory_key(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if separator && !output.is_empty() {
                output.push(' ');
            }
            output.push(character);
            separator = false;
        } else if character.is_whitespace() {
            separator = true;
        }
    }
    output
}

fn extract_statement(text: &str) -> Option<(String, String)> {
    // Chinese markers are CJK and unaffected by case folding, so match the
    // original text directly. This keeps byte offsets consistent with `text`
    // even when the input also contains characters whose lowercase form has a
    // different byte length.
    let chinese_markers = [
        ("记住", "explicit remember request"),
        ("以后", "future-facing preference"),
        ("始终", "persistent working rule"),
        ("总是", "persistent working rule"),
        ("每次", "repeated workflow preference"),
        ("默认", "default behavior preference"),
        ("不要", "negative working preference"),
        ("偏好", "user preference"),
        ("规则", "durable rule"),
        ("约定", "project convention"),
    ];

    for (marker, reason) in chinese_markers {
        if let Some(index) = text.find(marker) {
            let statement = text[index + marker.len()..]
                .trim_start_matches(|character| matches!(character, ':' | '：' | '-' | ' '))
                .trim();
            if !statement.is_empty() {
                return Some((statement.to_string(), reason.to_string()));
            }
        }
    }

    // English markers require ASCII word boundaries so partial matches inside
    // longer words do not trigger: "prefer" must not fire inside "preferred",
    // "preference" must not fire inside "preferences", and "never" must not fire
    // inside "whenever". Because every marker is ASCII, we match case
    // insensitively against byte slices of the original text and inspect the
    // neighbouring characters. Operating on `text` (never a lowercased copy)
    // avoids mixing byte and char indices, which is the root cause of the
    // multi-byte UTF-8 miscalculation.
    let english_markers = [
        ("remember", "explicit remember request"),
        ("always", "persistent working rule"),
        ("never", "negative working preference"),
        ("prefer", "user preference"),
        ("preference", "user preference"),
    ];

    for (marker, reason) in english_markers {
        if let Some(index) = find_word_boundary_marker(text, marker) {
            let statement = text[index + marker.len()..]
                .trim_start_matches(|character| matches!(character, ':' | '：' | '-' | ' '))
                .trim();
            if !statement.is_empty() {
                return Some((statement.to_string(), reason.to_string()));
            }
        }
    }

    None
}

/// Finds the byte offset of `marker` (an ASCII, already-lowercase word) inside
/// `text`, matched case-insensitively and only at ASCII word boundaries.
///
/// The scan walks char boundaries of the original `text`, so slice indexing is
/// always valid for multi-byte UTF-8. A match is accepted only when the
/// character immediately before and after the marker is not ASCII alphanumeric,
/// which keeps markers from firing inside longer words while still allowing
/// non-ASCII neighbours (for example a trailing CJK character) to act as
/// boundaries.
fn find_word_boundary_marker(text: &str, marker: &str) -> Option<usize> {
    let marker_len = marker.len();
    for (index, _) in text.char_indices() {
        let Some(slice) = text.get(index..index + marker_len) else {
            continue;
        };
        if !slice.eq_ignore_ascii_case(marker) {
            continue;
        }
        let prefix_is_boundary = text[..index]
            .chars()
            .next_back()
            .map_or(true, |character| !character.is_ascii_alphanumeric());
        let suffix_is_boundary = text[index + marker_len..]
            .chars()
            .next()
            .map_or(true, |character| !character.is_ascii_alphanumeric());
        if prefix_is_boundary && suffix_is_boundary {
            return Some(index);
        }
    }
    None
}

fn infer_type(statement: &str) -> &'static str {
    let lower = statement.to_lowercase();
    if has_any(
        &lower,
        &[
            "错误",
            "错了",
            "修正",
            "纠正",
            "失败",
            "复盘",
            "反思",
            "mistake",
            "wrong",
            "correction",
            "fix this",
            "failed",
        ],
    ) {
        "reflection_memory"
    } else if has_any(
        &lower,
        &[
            "下次",
            "继续",
            "待办",
            "任务",
            "进度",
            "todo",
            "next time",
            "follow up",
            "continue",
        ],
    ) {
        "task_memory"
    } else if has_any(
        &lower,
        &[
            "项目",
            "课题",
            "repo",
            "仓库",
            "工艺",
            "steel",
            "alloy",
            "heat treatment",
            "钢",
            "合金",
            "热处理",
            "热轧",
            "冷轧",
            "轧制",
            "屈服",
            "强度",
            "成分",
            "温度",
            "mpa",
        ],
    ) {
        "domain_memory"
    } else {
        "user_profile"
    }
}

fn infer_scope(statement: &str, memory_type: &str) -> &'static str {
    if memory_type == "domain_memory" {
        "domain"
    } else if memory_type == "task_memory" {
        "project"
    } else {
        let lower = statement.to_lowercase();
        if has_any(
            &lower,
            &[
                "steel",
                "alloy",
                "heat treatment",
                "钢",
                "合金",
                "热处理",
                "相变",
                "轧制",
                "热轧",
                "冷轧",
                "淬火",
                "回火",
                "成分",
                "屈服",
                "强度",
                "mpa",
            ],
        ) {
            "domain"
        } else {
            "global"
        }
    }
}

fn compact_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn has_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(content: &str) -> MemoryCandidate {
        extract_memory_candidate(content, "message-1", None, 0.8)
            .expect("valid candidate")
            .expect("candidate")
    }

    #[test]
    fn user_preference_becomes_user_profile_memory() {
        let memory = candidate("记住：我偏好简洁中文回答");

        assert_eq!(memory.memory_type, "user_profile");
        assert_eq!(memory.scope, "global");
    }

    #[test]
    fn steel_fact_becomes_domain_memory() {
        let memory = candidate("记住：Q355B 热轧屈服波动和终轧温度有关");

        assert_eq!(memory.memory_type, "domain_memory");
        assert_eq!(memory.scope, "domain");
    }

    #[test]
    fn unfinished_work_becomes_task_memory() {
        let memory = candidate("记住：下次继续处理钢铁数据集清洗任务");

        assert_eq!(memory.memory_type, "task_memory");
    }

    #[test]
    fn correction_becomes_reflection_memory() {
        let memory = candidate("记住：刚才的计算错误，以后遇到成分单位必须先确认");

        assert_eq!(memory.memory_type, "reflection_memory");
    }
}
