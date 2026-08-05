pub mod capabilities;
pub mod http;
pub mod mineru;
pub mod ollama;
pub mod openai;
pub mod profiles;
pub mod siliconflow;

use self::capabilities::{
    ChatEvent, ChatProvider, ChatRequest, ChatResponse, ProviderCapabilities,
};
use self::http::{ProviderError, ProviderErrorCode};
use self::ollama::OllamaProvider;
use self::openai::OpenAiProvider;
use self::profiles::{ProviderCapability, ProviderKind, ProviderProfile};
use crate::storage::secrets::SecretValue;

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
