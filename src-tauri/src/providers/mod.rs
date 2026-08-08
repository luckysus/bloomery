pub mod capabilities;
pub mod http;
pub mod mineru;
pub mod ollama;
pub mod openai;
pub mod profiles;
pub mod siliconflow;

use self::capabilities::{
    ChatEvent, ChatProvider, ChatRequest, ChatResponse, EmbeddingProvider, EmbeddingResponse,
    ProviderCapabilities, RerankDocument, RerankProvider, RerankResult,
};
use self::http::{ProviderError, ProviderErrorCode};
use self::ollama::OllamaProvider;
use self::openai::OpenAiProvider;
use self::profiles::{ProviderCapability, ProviderKind, ProviderProfile};
pub use self::siliconflow::SiliconFlowPlan;
use self::siliconflow::SiliconFlowProvider;
use crate::storage::secrets::SecretValue;

pub enum ConfiguredEmbeddingProvider {
    SiliconFlow(SiliconFlowProvider),
}

pub enum ConfiguredRerankProvider {
    SiliconFlow(SiliconFlowProvider),
}

pub fn configured_embedding_provider(
    profile: ProviderProfile,
    credential: Option<SecretValue>,
    plan: SiliconFlowPlan,
    model: Option<String>,
) -> Result<ConfiguredEmbeddingProvider, ProviderError> {
    SiliconFlowProvider::with_models(profile, credential, plan, model, None)
        .map(ConfiguredEmbeddingProvider::SiliconFlow)
}

pub fn configured_rerank_provider(
    profile: ProviderProfile,
    credential: Option<SecretValue>,
    plan: SiliconFlowPlan,
    model: Option<String>,
) -> Result<ConfiguredRerankProvider, ProviderError> {
    SiliconFlowProvider::with_models(profile, credential, plan, None, model)
        .map(ConfiguredRerankProvider::SiliconFlow)
}

impl EmbeddingProvider for ConfiguredEmbeddingProvider {
    fn capabilities(&self) -> &ProviderCapabilities {
        match self {
            Self::SiliconFlow(provider) => EmbeddingProvider::capabilities(provider),
        }
    }

    async fn embed(&self, inputs: Vec<String>) -> Result<EmbeddingResponse, ProviderError> {
        match self {
            Self::SiliconFlow(provider) => provider.embed(inputs).await,
        }
    }
}

impl RerankProvider for ConfiguredRerankProvider {
    fn capabilities(&self) -> &ProviderCapabilities {
        match self {
            Self::SiliconFlow(provider) => RerankProvider::capabilities(provider),
        }
    }

    async fn rerank(
        &self,
        query: String,
        documents: Vec<RerankDocument>,
    ) -> Result<Vec<RerankResult>, ProviderError> {
        match self {
            Self::SiliconFlow(provider) => provider.rerank(query, documents).await,
        }
    }
}

pub enum ConfiguredChatProvider {
    OpenAi(OpenAiProvider),
    Ollama(OllamaProvider),
}

pub fn configured_chat_provider(
    profile: ProviderProfile,
    credential: Option<SecretValue>,
) -> Result<ConfiguredChatProvider, ProviderError> {
    match profile.kind {
        ProviderKind::Ollama => OllamaProvider::new(profile).map(ConfiguredChatProvider::Ollama),
        _ if profile.kind.supports(ProviderCapability::Chat) => {
            OpenAiProvider::new(profile, credential).map(ConfiguredChatProvider::OpenAi)
        }
        _ => Err(ProviderError::new(
            ProviderErrorCode::UnsupportedCapability,
            None,
            "provider profile does not support chat",
        )),
    }
}

impl ChatProvider for ConfiguredChatProvider {
    fn capabilities(&self) -> &ProviderCapabilities {
        match self {
            Self::OpenAi(provider) => provider.capabilities(),
            Self::Ollama(provider) => provider.capabilities(),
        }
    }

    async fn chat(
        &self,
        request: ChatRequest,
        on_event: &mut (dyn FnMut(ChatEvent) + Send),
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<ChatResponse, ProviderError> {
        match self {
            Self::OpenAi(provider) => provider.chat(request, on_event, is_cancelled).await,
            Self::Ollama(provider) => provider.chat(request, on_event, is_cancelled).await,
        }
    }
}
