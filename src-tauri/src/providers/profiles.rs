use crate::providers::ollama::default_ollama_base_url;
use crate::providers::openai::default_openai_base_url;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAiCompatible,
    Ollama,
    #[serde(rename = "siliconflow")]
    SiliconFlow,
    #[serde(rename = "mineru")]
    MinerU,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "open_ai_compatible",
            Self::Ollama => "ollama",
            Self::SiliconFlow => "siliconflow",
            Self::MinerU => "mineru",
        }
    }

    pub fn supports(self, capability: ProviderCapability) -> bool {
        match self {
            Self::OpenAiCompatible | Self::Ollama => {
                matches!(
                    capability,
                    ProviderCapability::Chat | ProviderCapability::Embedding
                )
            }
            Self::SiliconFlow => matches!(
                capability,
                ProviderCapability::Chat
                    | ProviderCapability::Embedding
                    | ProviderCapability::Rerank
            ),
            Self::MinerU => capability == ProviderCapability::DocumentParser,
        }
    }
}

impl FromStr for ProviderKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open_ai_compatible" => Ok(Self::OpenAiCompatible),
            "ollama" => Ok(Self::Ollama),
            "siliconflow" => Ok(Self::SiliconFlow),
            "mineru" => Ok(Self::MinerU),
            _ => Err(format!("unsupported provider kind: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    Chat,
    Embedding,
    Rerank,
    DocumentParser,
}

impl ProviderCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Rerank => "rerank",
            Self::DocumentParser => "document_parser",
        }
    }
}

impl FromStr for ProviderCapability {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "chat" => Ok(Self::Chat),
            "embedding" => Ok(Self::Embedding),
            "rerank" => Ok(Self::Rerank),
            "document_parser" => Ok(Self::DocumentParser),
            _ => Err(format!("unsupported provider capability: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: Uuid,
    pub kind: ProviderKind,
    pub display_name: String,
    pub base_url: String,
    pub model_id: Option<String>,
    pub secret_ref: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProfileRecord {
    pub profile: ProviderProfile,
    pub revision: u64,
    pub secret_generation: u64,
}

impl ProviderProfile {
    pub fn validate(mut self) -> Result<Self, String> {
        self.display_name = self.display_name.trim().to_string();
        if self.display_name.is_empty() {
            return Err("provider display name is required".to_string());
        }
        if self.display_name.chars().count() > 80 {
            return Err("provider display name is too long".to_string());
        }

        let parsed = reqwest::Url::parse(self.base_url.trim())
            .map_err(|error| format!("invalid provider base URL: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("provider base URL must use HTTP or HTTPS".to_string());
        }
        if parsed.host_str().is_none() {
            return Err("provider base URL requires a host".to_string());
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("provider base URL must not contain credentials".to_string());
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(
                "provider base URL must not contain query parameters or fragments".to_string(),
            );
        }
        self.base_url = parsed.to_string().trim_end_matches('/').to_string();
        self.model_id = normalize_optional(self.model_id);
        self.secret_ref = normalize_optional(self.secret_ref);
        Ok(self)
    }
}

pub fn resolve_chat_profile(
    provider: &str,
    base_url: &str,
    model_id: &str,
) -> Result<ProviderProfile, String> {
    let provider = provider.trim();
    let kind = if provider.eq_ignore_ascii_case("ollama") {
        ProviderKind::Ollama
    } else if provider.eq_ignore_ascii_case("siliconflow") {
        ProviderKind::SiliconFlow
    } else {
        ProviderKind::OpenAiCompatible
    };
    let base_url = if !base_url.trim().is_empty() {
        base_url.trim()
    } else if kind == ProviderKind::Ollama {
        default_ollama_base_url()
    } else {
        default_openai_base_url(provider).unwrap_or_default()
    };
    ProviderProfile {
        id: Uuid::nil(),
        kind,
        display_name: if provider.is_empty() {
            "Local LLM".to_string()
        } else {
            provider.to_string()
        },
        base_url: base_url.to_string(),
        model_id: Some(model_id.trim().to_string()),
        secret_ref: None,
        enabled: true,
    }
    .validate()
}

pub(crate) fn validate_bearer_transport(
    base_url: &str,
    has_credential: bool,
) -> Result<(), String> {
    if !has_credential {
        return Ok(());
    }
    let url = reqwest::Url::parse(base_url).map_err(|_| "invalid provider base URL".to_string())?;
    let is_loopback = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if url.scheme() != "https" && !is_loopback {
        Err("provider credentials require HTTPS or a loopback HTTP address".to_string())
    } else {
        Ok(())
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
