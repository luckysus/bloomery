use super::model::{LocalLlmConfig, StreamedLlmAnswer};
use crate::providers::capabilities::{ChatEvent, ChatProvider, ChatRequest};
use crate::providers::configured_chat_provider;
use crate::providers::profiles::{
    resolve_chat_profile, ProviderCapability, ProviderKind, ProviderProfile, ProviderProfileRecord,
};
use crate::storage::repositories::{provider_profiles, settings};
use crate::storage::secrets::{SecretStore, SecretValue};
use std::sync::Mutex;

pub fn load_local_llm_config(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    secrets: &dyn SecretStore,
) -> Result<LocalLlmConfig, String> {
    let raw = settings::get(conn, workspace_id, super::model::LOCAL_LLM_CONFIG_KEY)?;
    let legacy = raw
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| format!("parse local llm config failed: {error}"))
        })
        .transpose()?;
    if let Some(config) = legacy.filter(|config: &LocalLlmConfig| {
        !config.provider.trim().is_empty() && !config.model_name.trim().is_empty()
    }) {
        return Ok(config);
    }

    let record =
        provider_profiles::get_default_record(conn, workspace_id, ProviderCapability::Chat)?
            .ok_or_else(|| "default chat provider is not configured".to_string())?;
    let model_name = record
        .profile
        .model_id
        .clone()
        .ok_or_else(|| "default chat provider model is not configured".to_string())?;
    let credential = profile_credential(&record, secrets)?;
    Ok(LocalLlmConfig {
        provider: record.profile.kind.as_str().to_string(),
        base_url: record.profile.base_url,
        model_name,
        api_key: String::new(),
        credential,
    })
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
    let credential = match config.credential.clone() {
        Some(value) => Some(value),
        None => (!config.api_key.trim().is_empty())
            .then(|| SecretValue::new(config.api_key.trim()).map_err(|error| error.to_string()))
            .transpose()?,
    };
    if credential.is_none() && profile.kind != ProviderKind::Ollama {
        return Err("API key is required for the configured provider".to_string());
    }
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
        tool_calls: Vec::new(),
    })
}

fn profile_credential(
    record: &ProviderProfileRecord,
    secrets: &dyn SecretStore,
) -> Result<Option<SecretValue>, String> {
    let Some(name) = record.profile.secret_ref.as_deref() else {
        return Ok(None);
    };
    let reference = crate::storage::secrets::SecretRef::at_generation(
        record.profile.id,
        name,
        record.secret_generation,
    )
    .map_err(|error| error.to_string())?;
    match secrets.get(&reference) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::profiles::{ProviderCapability, ProviderKind, ProviderProfile};
    use crate::storage::migrations::migrate;
    use crate::storage::repositories::provider_profiles;
    use crate::storage::secrets::{SecretError, SecretRef, SecretStore};
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemorySecretStore {
        values: Mutex<HashMap<String, SecretValue>>,
    }

    impl SecretStore for MemorySecretStore {
        fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError> {
            self.values
                .lock()
                .map_err(|_| SecretError::backend("memory secret store poisoned"))?
                .insert(reference.account(), value.clone());
            Ok(())
        }

        fn get(&self, reference: &SecretRef) -> Result<SecretValue, SecretError> {
            self.values
                .lock()
                .map_err(|_| SecretError::backend("memory secret store poisoned"))?
                .get(&reference.account())
                .cloned()
                .ok_or_else(SecretError::not_found)
        }

        fn delete(&self, reference: &SecretRef) -> Result<(), SecretError> {
            self.values
                .lock()
                .map_err(|_| SecretError::backend("memory secret store poisoned"))?
                .remove(&reference.account())
                .map(|_| ())
                .ok_or_else(SecretError::not_found)
        }
    }

    #[test]
    fn default_chat_profile_loads_metadata_and_secret_from_local_stores() {
        let mut connection = Connection::open_in_memory().expect("open database");
        migrate(&mut connection).expect("migrate database");
        let store = MemorySecretStore::default();
        let profile_id = uuid::Uuid::new_v4();
        provider_profiles::save(
            &mut connection,
            "workspace-a",
            ProviderProfile {
                id: profile_id,
                kind: ProviderKind::OpenAiCompatible,
                display_name: "Steel LLM".to_string(),
                base_url: "https://provider.example/v1".to_string(),
                model_id: Some("steel-model".to_string()),
                secret_ref: Some("api_key".to_string()),
                enabled: true,
            },
        )
        .expect("save profile");
        provider_profiles::set_default(
            &mut connection,
            "workspace-a",
            ProviderCapability::Chat,
            Some(profile_id),
        )
        .expect("set chat default");
        store
            .set(
                &SecretRef::new(profile_id, "api_key").expect("secret ref"),
                &SecretValue::new("sk-secret").expect("secret"),
            )
            .expect("save secret");

        let config =
            load_local_llm_config(&connection, "workspace-a", &store).expect("load config");

        assert_eq!(config.provider, "open_ai_compatible");
        assert_eq!(config.base_url, "https://provider.example/v1");
        assert_eq!(config.model_name, "steel-model");
        assert!(config.api_key.is_empty());
        assert_eq!(
            config.credential.as_ref(),
            Some(&SecretValue::new("sk-secret").expect("secret"))
        );
        assert!(config.has_credential());
    }
}
