use crate::agent::context::{SummaryMessage, SummaryPlan};
use crate::agent::runtime::AgentLoopLimits;
use crate::storage::repositories::settings;
use crate::storage::secrets::SecretValue;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub const LOCAL_LLM_CONFIG_KEY: &str = "local_llm_config";
pub const AGENT_PREFERENCES_KEY: &str = "agent.preferences";
pub const LOCAL_SUMMARY_CONTEXT_LIMIT: usize = 64;
pub const LOCAL_SUMMARY_CONTEXT_CHAR_LIMIT: usize = 2500;
const DEFAULT_AGENT_SYSTEM_PROMPT: &str =
    "你是 Suna 的钢铁材料研发智能体。回答必须区分事实、推断和待验证内容。";
const MIN_CONTEXT_BUDGET: usize = 1_024;
const MAX_CONTEXT_BUDGET: usize = 262_144;
const MAX_SYSTEM_PROMPT_CHARS: usize = 16_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentPreferences {
    pub default_agent: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub context_budget: usize,
    pub retries: usize,
    pub save_checkpoints: bool,
    pub allow_recovery: bool,
    pub confirm_dangerous: bool,
    pub allow_file_access: bool,
    pub allow_shell: bool,
    pub allow_network: bool,
    pub allow_mcp: bool,
}

impl Default for AgentPreferences {
    fn default() -> Self {
        Self {
            default_agent: "steel-research".to_string(),
            system_prompt: DEFAULT_AGENT_SYSTEM_PROMPT.to_string(),
            max_turns: 20,
            context_budget: 32_768,
            retries: 2,
            save_checkpoints: true,
            allow_recovery: true,
            confirm_dangerous: true,
            allow_file_access: true,
            allow_shell: false,
            allow_network: true,
            allow_mcp: true,
        }
    }
}

impl AgentPreferences {
    pub fn from_json(raw: Option<&str>) -> Self {
        let mut preferences: Self = raw
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default();
        preferences.normalize();
        preferences
    }

    pub fn normalize(&mut self) {
        let defaults = Self::default();
        if self.default_agent.trim().is_empty() {
            self.default_agent = defaults.default_agent;
        } else {
            self.default_agent = self.default_agent.trim().to_string();
        }
        self.system_prompt = self
            .system_prompt
            .trim()
            .chars()
            .take(MAX_SYSTEM_PROMPT_CHARS)
            .collect();
        if self.system_prompt.is_empty() {
            self.system_prompt = defaults.system_prompt;
        }
        self.max_turns = self.max_turns.clamp(1, 100);
        self.context_budget = self
            .context_budget
            .clamp(MIN_CONTEXT_BUDGET, MAX_CONTEXT_BUDGET);
        self.retries = self.retries.min(10);
    }

    pub fn loop_limits(&self) -> AgentLoopLimits {
        AgentLoopLimits {
            max_model_calls: Some(self.max_turns),
            max_tool_rounds: Some(self.max_turns),
            max_recovery_attempts: self.retries,
            context_budget: Some(self.context_budget),
            save_checkpoints: self.save_checkpoints,
            ..AgentLoopLimits::default()
        }
    }
}

pub fn load_agent_preferences(
    connection: &Connection,
    workspace_id: &str,
) -> Result<AgentPreferences, String> {
    let raw = settings::get(connection, workspace_id, AGENT_PREFERENCES_KEY)?;
    Ok(AgentPreferences::from_json(raw.as_deref()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAgentChatRequest {
    pub session_id: Option<String>,
    pub message: String,
    pub run_id: Option<String>,
    pub evidence_pack_id: Option<String>,
    #[serde(default)]
    pub smart_search_enabled: bool,
    #[serde(default)]
    pub attachments: Vec<LocalAgentAttachment>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAgentAttachment {
    pub data: String,
    pub mime: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizeConversationRequest {
    pub conversation_id: String,
    pub covered_message_id: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizeConversationResponse {
    pub summarized: bool,
    pub summary: Option<String>,
    pub covered_message_id: Option<String>,
    pub total_tokens: usize,
    pub folded_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalLlmConfig {
    pub provider: String,
    pub base_url: String,
    pub model_name: String,
    pub api_key: String,
    #[serde(skip)]
    pub(crate) credential: Option<SecretValue>,
}

impl LocalLlmConfig {
    pub(crate) fn has_credential(&self) -> bool {
        self.credential.is_some() || !self.api_key.trim().is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ExistingSummary {
    pub summary: String,
    pub covered_message_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StreamedLlmAnswer {
    pub text: String,
    pub reasoning: String,
    pub reasoning_ms: u64,
    pub stopped: bool,
    pub tool_calls: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::AgentPreferences;

    #[test]
    fn invalid_agent_preferences_use_safe_bounds_and_defaults() {
        let preferences = AgentPreferences::from_json(Some(
            r#"{"systemPrompt":"   ","maxTurns":0,"contextBudget":1,"retries":99,"allowMcp":false}"#,
        ));

        assert_eq!(
            preferences.system_prompt,
            AgentPreferences::default().system_prompt
        );
        assert_eq!(preferences.max_turns, 1);
        assert_eq!(preferences.context_budget, 1_024);
        assert_eq!(preferences.retries, 10);
        assert!(!preferences.allow_mcp);
    }

    #[test]
    fn preferences_map_to_loop_limits() {
        let preferences = AgentPreferences::from_json(Some(
            r#"{"maxTurns":7,"contextBudget":4096,"retries":3,"saveCheckpoints":false}"#,
        ));
        let limits = preferences.loop_limits();

        assert_eq!(limits.max_model_calls, Some(7));
        assert_eq!(limits.max_tool_rounds, Some(7));
        assert_eq!(limits.max_recovery_attempts, 3);
        assert_eq!(limits.context_budget, Some(4_096));
        assert!(!limits.save_checkpoints);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopIntentKind {
    LocalQa,
    KnowledgeQa,
    OptimizationAdvice,
    OptimizationTask,
    TrainingTask,
    LiteratureTask,
    Clarify,
}

impl DesktopIntentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalQa => "local_qa",
            Self::KnowledgeQa => "knowledge_qa",
            Self::OptimizationAdvice => "optimization_advice",
            Self::OptimizationTask => "optimization_task",
            Self::TrainingTask => "training_task",
            Self::LiteratureTask => "literature_task",
            Self::Clarify => "clarify",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DesktopRoute {
    pub intent: DesktopIntentKind,
    pub confidence: f32,
    pub reason: &'static str,
    pub unavailable_capability: Option<&'static str>,
}

#[derive(Debug)]
pub(crate) struct SummaryPreparation {
    pub config: LocalLlmConfig,
    pub prompt: String,
    pub plan: SummaryPlan,
}

#[allow(dead_code)]
pub(crate) type SummaryMessageList = Vec<SummaryMessage>;
