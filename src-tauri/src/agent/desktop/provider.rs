use super::model::{LocalLlmConfig, StreamedLlmAnswer};
use crate::providers::capabilities::{ChatEvent, ChatProvider, ChatRequest};
use crate::providers::configured_chat_provider;
use crate::providers::profiles::{
    resolve_chat_profile, ProviderCapability, ProviderKind, ProviderProfile,
};
use crate::storage::secrets::SecretValue;
use std::sync::Mutex;

pub fn load_local_llm_config(
    conn: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<LocalLlmConfig, String> {
    let raw = crate::storage::repositories::settings::get(
        conn,
        workspace_id,
        super::model::LOCAL_LLM_CONFIG_KEY,
    )?;
    raw.map(|value| {
        serde_json::from_str(&value)
            .map_err(|error| format!("parse local llm config failed: {error}"))
    })
    .transpose()
    .map(|config| config.unwrap_or_default())
}

pub fn validate_local_llm_config(config: &LocalLlmConfig) -> Result<(), String> {
    provider_profile_from_config(config).map(|_| ())
}

pub fn provider_profile_from_config(
    config: &LocalLlmConfig,
) -> Result<(ProviderProfile, Option<SecretValue>), String> {
    if config.model_name.trim().is_empty() {
        return Err("model name is required".to_string());
    }
    let profile = resolve_chat_profile(&config.provider, &config.base_url, &config.model_name)
        .map_err(|error| {
            if config.base_url.trim().is_empty() {
                "provider base URL is required".to_string()
            } else {
                error
            }
        })?;
    if config.api_key.trim().is_empty() && profile.kind != ProviderKind::Ollama {
        return Err("API key is required for the configured provider".to_string());
    }
    let credential = (!config.api_key.trim().is_empty())
        .then(|| SecretValue::new(config.api_key.trim()).map_err(|error| error.to_string()))
        .transpose()?;
    Ok((profile, credential))
}

pub async fn stream_llm_answer_core<IsCancelled, OnDelta>(
    config: &LocalLlmConfig,
    context_prompt: &str,
    user_message: &str,
    is_cancelled: IsCancelled,
    mut on_delta: OnDelta,
) -> Result<StreamedLlmAnswer, String>
where
    IsCancelled: Fn() -> Result<bool, String> + Send + Sync,
    OnDelta: FnMut(&str) + Send,
{
    if is_cancelled()? {
        return Err("LLM run cancelled".to_string());
    }
    let (profile, credential) = provider_profile_from_config(config)?;
    let request = ChatRequest::single_turn(context_prompt, user_message);
    let cancellation_error = Mutex::new(None);
    let cancelled = || match is_cancelled() {
        Ok(cancelled) => cancelled,
        Err(error) => {
            if let Ok(mut slot) = cancellation_error.lock() {
                *slot = Some(error);
            }
            true
        }
    };
    let mut on_event = |event| {
        if let ChatEvent::TextDelta(delta) = event {
            on_delta(&delta);
        }
    };
    let provider =
        configured_chat_provider(profile, credential).map_err(|error| error.to_string())?;
    provider
        .capabilities()
        .require(ProviderCapability::Chat)
        .map_err(|error| error.to_string())?;
    let response = provider.chat(request, &mut on_event, &cancelled).await;
    if let Some(error) = cancellation_error
        .lock()
        .map_err(|_| "local agent cancellation state poisoned".to_string())?
        .take()
    {
        return Err(error);
    }
    let response = response.map_err(|error| error.to_string())?;
    Ok(StreamedLlmAnswer {
        text: response.text,
        stopped: response.cancelled,
    })
}
