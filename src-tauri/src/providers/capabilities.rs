use crate::providers::http::{ProviderError, ProviderErrorCode};
use crate::providers::profiles::{ProviderCapability, ProviderKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider_kind: ProviderKind,
    pub model_id: String,
    pub capabilities: Vec<ProviderCapability>,
    pub context_window: Option<usize>,
    pub streaming: bool,
    pub tool_calls: bool,
    pub json_schema: bool,
    pub max_batch_size: Option<usize>,
}

impl ProviderCapabilities {
    pub fn chat(provider_kind: ProviderKind, model_id: impl Into<String>) -> Self {
        Self {
            provider_kind,
            model_id: model_id.into(),
            capabilities: vec![ProviderCapability::Chat],
            context_window: None,
            streaming: true,
            tool_calls: true,
            json_schema: true,
            max_batch_size: None,
        }
    }

    pub fn supports(&self, capability: ProviderCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn require(&self, capability: ProviderCapability) -> Result<(), ProviderError> {
        if self.supports(capability) {
            Ok(())
        } else {
            Err(ProviderError::new(
                ProviderErrorCode::UnsupportedCapability,
                None,
                format!(
                    "model {} does not support {}",
                    self.model_id,
                    capability.as_str()
                ),
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub tools: Option<Value>,
    pub response_format: Option<Value>,
}

impl ChatRequest {
    pub fn single_turn(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system.into(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user.into(),
                },
            ],
            temperature: 0.2,
            tools: None,
            response_format: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ChatEvent {
    TextDelta(String),
    ToolCallDelta(ToolCallDelta),
    Usage(ChatUsage),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub text: String,
    pub tool_calls: Vec<ChatToolCall>,
    pub usage: Option<ChatUsage>,
    pub finish_reason: Option<String>,
    pub cancelled: bool,
}

pub trait ChatProvider: Send + Sync {
    fn capabilities(&self) -> &ProviderCapabilities;

    fn chat(
        &self,
        request: ChatRequest,
        on_event: &mut (dyn FnMut(ChatEvent) + Send),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> impl Future<Output = Result<ChatResponse, ProviderError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub model_id: String,
    pub vectors: Vec<Vec<f32>>,
}

pub trait EmbeddingProvider: Send + Sync {
    fn capabilities(&self) -> &ProviderCapabilities;
    fn embed(
        &self,
        inputs: Vec<String>,
    ) -> impl Future<Output = Result<EmbeddingResponse, ProviderError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerankDocument {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankResult {
    pub id: String,
    pub score: f32,
}

pub trait RerankProvider: Send + Sync {
    fn capabilities(&self) -> &ProviderCapabilities;
    fn rerank(
        &self,
        query: String,
        documents: Vec<RerankDocument>,
    ) -> impl Future<Output = Result<Vec<RerankResult>, ProviderError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentParseRequest {
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTaskId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentTaskState {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentTaskStatus {
    pub id: RemoteTaskId,
    pub state: DocumentTaskState,
    pub progress_percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedDocumentArtifact {
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

pub trait DocumentParserProvider: Send + Sync {
    fn capabilities(&self) -> &ProviderCapabilities;
    fn submit(
        &self,
        request: DocumentParseRequest,
    ) -> impl Future<Output = Result<RemoteTaskId, ProviderError>> + Send;
    fn poll(
        &self,
        id: &RemoteTaskId,
    ) -> impl Future<Output = Result<DocumentTaskStatus, ProviderError>> + Send;
    fn download(
        &self,
        id: &RemoteTaskId,
    ) -> impl Future<Output = Result<ParsedDocumentArtifact, ProviderError>> + Send;
    fn cancel(&self, id: &RemoteTaskId) -> impl Future<Output = Result<(), ProviderError>> + Send;
}
