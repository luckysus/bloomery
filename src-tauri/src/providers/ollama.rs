use crate::providers::capabilities::{
    ChatEvent, ChatProvider, ChatRequest, ChatResponse, ProviderCapabilities,
};
use crate::providers::http::{ProviderError, ProviderErrorCode};
use crate::providers::openai::OpenAiProvider;
use crate::providers::profiles::{ProviderKind, ProviderProfile};

pub struct OllamaProvider {
    inner: OpenAiProvider,
    capabilities: ProviderCapabilities,
}

impl OllamaProvider {
    pub fn new(profile: ProviderProfile) -> Result<Self, ProviderError> {
        if profile.kind != ProviderKind::Ollama {
            return Err(ProviderError::new(
                ProviderErrorCode::ProviderResponse,
                None,
                "Ollama provider requires an Ollama profile",
            ));
        }
        let endpoint = normalize_ollama_chat_url(&profile.base_url);
        let inner = OpenAiProvider::with_endpoint(profile, None, endpoint)?;
        let mut capabilities = inner.capabilities().clone();
        capabilities.tool_calls = false;
        capabilities.json_schema = false;
        Ok(Self {
            inner,
            capabilities,
        })
    }
}

impl ChatProvider for OllamaProvider {
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    async fn chat(
        &self,
        request: ChatRequest,
        on_event: &mut (dyn FnMut(ChatEvent) + Send),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<ChatResponse, ProviderError> {
        if request.tools.is_some() && !self.capabilities.tool_calls {
            return Err(ProviderError::new(
                ProviderErrorCode::UnsupportedCapability,
                None,
                "Ollama model does not declare tool-call support",
            ));
        }
        if request.response_format.is_some() && !self.capabilities.json_schema {
            return Err(ProviderError::new(
                ProviderErrorCode::UnsupportedCapability,
                None,
                "Ollama model does not declare JSON-schema support",
            ));
        }
        self.inner.chat(request, on_event, is_cancelled).await
    }
}

pub fn normalize_ollama_chat_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

pub fn default_ollama_base_url() -> &'static str {
    "http://127.0.0.1:11434"
}
