use crate::agent::context::{SummaryMessage, SummaryPlan};
use serde::{Deserialize, Serialize};

pub const LOCAL_LLM_CONFIG_KEY: &str = "local_llm_config";
pub const LOCAL_ASK_CONTEXT_LIMIT: usize = 12;
pub const LOCAL_ASK_CONTEXT_CHAR_LIMIT: usize = 1800;
pub const LOCAL_SUMMARY_CONTEXT_LIMIT: usize = 64;
pub const LOCAL_SUMMARY_CONTEXT_CHAR_LIMIT: usize = 2500;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAgentChatRequest {
    pub session_id: Option<String>,
    pub message: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAskRequest {
    pub query: String,
    pub contexts: Vec<String>,
    pub mode: Option<String>,
    pub run_id: Option<String>,
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
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalAgentDelta {
    pub run_id: String,
    pub delta: String,
}

#[derive(Debug, Clone)]
pub struct ExistingSummary {
    pub summary: String,
    pub covered_message_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StreamedLlmAnswer {
    pub text: String,
    pub stopped: bool,
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
