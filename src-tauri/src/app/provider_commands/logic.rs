// Provider profile domain logic and probe implementation.
use crate::providers::http::{build_client, HttpClientConfig, ProviderError, ProviderErrorCode};
use crate::providers::profiles::{
    validate_bearer_transport, ProviderCapability, ProviderKind, ProviderProfile,
    ProviderProfileRecord,
};
use crate::storage::repositories::provider_profiles;
use crate::storage::secrets::{status, SecretRef, SecretStore, SecretValue, MAX_SECRET_GENERATION};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ProviderProfileInput {
    pub id: Option<String>,
    pub kind: ProviderKind,
    pub display_name: String,
    pub base_url: String,
    pub model_id: Option<String>,
    pub credential_name: Option<String>,
    pub enabled: bool,
}

impl ProviderProfileInput {
    fn into_profile(self) -> Result<ProviderProfile, String> {
        let id = self
            .id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|error| format!("invalid provider profile ID: {error}"))?
            .unwrap_or_else(Uuid::new_v4);
        let secret_ref = self
            .credential_name
            .map(|name| {
                SecretRef::new(id, name.as_str())
                    .map(|_| name.trim().to_string())
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        ProviderProfile {
            id,
            kind: self.kind,
            display_name: self.display_name,
            base_url: self.base_url,
            model_id: self.model_id,
            secret_ref,
            enabled: self.enabled,
        }
        .validate()
    }
}

#[derive(Debug, Serialize)]
pub struct ProviderProfileResponse {
    pub id: Uuid,
    pub kind: ProviderKind,
    pub display_name: String,
    pub base_url: String,
    pub model_id: Option<String>,
    pub enabled: bool,
    pub revision: u64,
    pub secret_generation: u64,
    pub secret_configured: bool,
}

#[derive(Debug, Serialize)]
pub struct ProviderProbeResponse {
    pub ok: bool,
    pub status_code: Option<u16>,
    pub error_code: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(Debug)]
struct ProbeFailure {
    code: String,
    status_code: Option<u16>,
}

impl ProbeFailure {
    fn new(code: impl Into<String>, status_code: Option<u16>) -> Self {
        Self {
            code: code.into(),
            status_code,
        }
    }
}

impl From<ProviderError> for ProbeFailure {
    fn from(error: ProviderError) -> Self {
        Self::new(error.code().as_str(), error.status())
    }
}

fn profile_response(
    record: ProviderProfileRecord,
    store: &dyn SecretStore,
) -> Result<ProviderProfileResponse, String> {
    let profile = record.profile;
    let secret_configured = match profile.secret_ref.as_deref() {
        Some(name) => {
            status(
                store,
                &SecretRef::at_generation(profile.id, name, record.secret_generation)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?
            .configured
        }
        None => false,
    };
    Ok(ProviderProfileResponse {
        id: profile.id,
        kind: profile.kind,
        display_name: profile.display_name,
        base_url: profile.base_url,
        model_id: profile.model_id,
        enabled: profile.enabled,
        revision: record.revision,
        secret_generation: record.secret_generation,
        secret_configured,
    })
}

pub(crate) fn list_profile_responses(
    connection: &Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
) -> Result<Vec<ProviderProfileResponse>, String> {
    provider_profiles::list_records(connection, workspace_id)?
        .into_iter()
        .map(|record| profile_response(record, store))
        .collect()
}

pub(crate) fn save_profile(
    connection: &mut Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    input: ProviderProfileInput,
) -> Result<ProviderProfileResponse, String> {
    let profile = input.into_profile()?;
    let previous = provider_profiles::get_record(connection, workspace_id, profile.id)?;
    let old_secret = previous.as_ref().and_then(|previous| {
        (previous.profile.secret_ref != profile.secret_ref).then_some((
            previous.profile.id,
            previous.profile.secret_ref.clone()?,
            previous.secret_generation,
        ))
    });
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let saved = provider_profiles::save_record(&transaction, workspace_id, profile)?;
    let deleted = if let Some((profile_id, old_name, generation)) = old_secret {
        delete_secret_generations(store, profile_id, &old_name, generation)?
    } else {
        Vec::new()
    };
    if let Err(error) = transaction.commit() {
        return Err(restore_deleted_generations(
            store,
            &deleted,
            error.to_string(),
        ));
    }
    profile_response(saved, store)
}

pub(crate) fn set_default_profile(
    connection: &mut Connection,
    workspace_id: &str,
    capability: ProviderCapability,
    profile_id: Option<Uuid>,
) -> Result<(), String> {
    provider_profiles::set_default(connection, workspace_id, capability, profile_id)
}

pub(crate) fn profile_credential(
    profile: &ProviderProfile,
    secret_generation: u64,
    store: &dyn SecretStore,
) -> Result<Option<SecretValue>, String> {
    let Some(name) = profile.secret_ref.as_deref() else {
        return Ok(None);
    };
    let reference = SecretRef::at_generation(profile.id, name, secret_generation)
        .map_err(|error| error.to_string())?;
    match store.get(&reference) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn delete_profile(
    connection: &mut Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    id: Uuid,
) -> Result<(), String> {
    let record = provider_profiles::get_record(connection, workspace_id, id)?
        .ok_or_else(|| "provider profile not found".to_string())?;
    let deleted = match record.profile.secret_ref.as_deref() {
        Some(name) => {
            delete_secret_generations(store, record.profile.id, name, record.secret_generation)?
        }
        None => Vec::new(),
    };
    if let Err(error) = provider_profiles::delete(connection, workspace_id, id) {
        return Err(restore_deleted_generations(store, &deleted, error));
    }
    Ok(())
}

fn delete_secret_generations(
    store: &dyn SecretStore,
    profile_id: Uuid,
    credential_name: &str,
    maximum_generation: u64,
) -> Result<Vec<(SecretRef, SecretValue)>, String> {
    if maximum_generation > MAX_SECRET_GENERATION {
        return Err("provider secret generation is too large".to_string());
    }
    let mut deleted = Vec::new();
    let mut generation = 0_u64;
    loop {
        let reference = SecretRef::at_generation(profile_id, credential_name, generation)
            .map_err(|error| error.to_string())?;
        let value = match store.get(&reference) {
            Ok(value) => value,
            Err(error) if error.is_not_found() => {
                if generation == maximum_generation {
                    break;
                }
                generation += 1;
                continue;
            }
            Err(error) => {
                return Err(restore_deleted_generations(
                    store,
                    &deleted,
                    error.to_string(),
                ))
            }
        };
        match store.delete(&reference) {
            Ok(()) => deleted.push((reference, value)),
            Err(error) if error.is_not_found() => {}
            Err(error) => {
                return Err(restore_deleted_generations(
                    store,
                    &deleted,
                    error.to_string(),
                ))
            }
        }
        if generation == maximum_generation {
            break;
        }
        generation += 1;
    }
    Ok(deleted)
}

fn restore_deleted_generations(
    store: &dyn SecretStore,
    deleted: &[(SecretRef, SecretValue)],
    primary_error: String,
) -> String {
    let mut restore_errors = Vec::new();
    for (reference, value) in deleted {
        if let Err(error) = store.set(reference, value) {
            restore_errors.push(error.to_string());
        }
    }
    if restore_errors.is_empty() {
        primary_error
    } else {
        format!(
            "{primary_error}; credential rollback failed: {}",
            restore_errors.join("; ")
        )
    }
}

pub(crate) async fn probe_provider(
    profile: ProviderProfile,
    credential: Option<SecretValue>,
    capability: Option<ProviderCapability>,
) -> ProviderProbeResponse {
    let started = Instant::now();
    let result = async {
        if profile.kind == ProviderKind::SiliconFlow {
            if let Some(capability) = capability {
                return crate::providers::probe_siliconflow(profile, credential, capability)
                    .await
                    .map_err(ProbeFailure::from);
            }
        }
        probe_base_url(&profile, credential).await
    }
    .await;
    let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    match result {
        Ok(status_code) => ProviderProbeResponse {
            ok: true,
            status_code: Some(status_code),
            error_code: None,
            elapsed_ms,
        },
        Err(error) => ProviderProbeResponse {
            ok: false,
            status_code: error.status_code,
            error_code: Some(error.code),
            elapsed_ms,
        },
    }
}

async fn probe_base_url(
    profile: &ProviderProfile,
    credential: Option<SecretValue>,
) -> Result<u16, ProbeFailure> {
    let result = async {
        validate_bearer_transport(&profile.base_url, credential.is_some())
            .map_err(|_| ProbeFailure::new("insecure_transport", None))?;
        let client = build_client(&HttpClientConfig {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(5),
            proxy_url: None,
        })
        .map_err(ProbeFailure::from)?;
        let mut request = client.get(&profile.base_url);
        if let Some(value) = credential.as_ref() {
            request = request.bearer_auth(value.expose());
        }
        request.send().await.map_err(|error| {
            if error.is_timeout() {
                ProbeFailure::new(ProviderErrorCode::Timeout.as_str(), None)
            } else {
                ProbeFailure::new(ProviderErrorCode::Network.as_str(), None)
            }
        })
    }
    .await;
    let response = result?;
    let status = response.status();
    if status.is_success()
        || matches!(
            status,
            reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
        )
    {
        Ok(status.as_u16())
    } else {
        let code = match status {
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                ProviderErrorCode::Authentication
            }
            reqwest::StatusCode::TOO_MANY_REQUESTS => ProviderErrorCode::Quota,
            _ => ProviderErrorCode::ProviderResponse,
        };
        Err(ProbeFailure::new(code.as_str(), Some(status.as_u16())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::profiles::{ProviderCapability, ProviderKind};
    use crate::storage::migrations::migrate;
    use crate::storage::secrets::{SecretError, SecretRef, SecretStore, SecretValue};
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct MemorySecretStore {
        values: Mutex<HashMap<String, SecretValue>>,
        fail_delete: bool,
        fail_delete_on_call: Option<usize>,
        delete_calls: Mutex<usize>,
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
            let mut calls = self
                .delete_calls
                .lock()
                .map_err(|_| SecretError::backend("memory secret store poisoned"))?;
            *calls += 1;
            let call_number = *calls;
            drop(calls);
            if self.fail_delete {
                return Err(SecretError::backend("injected delete failure"));
            }
            if self.fail_delete_on_call == Some(call_number) {
                return Err(SecretError::backend("injected delete failure"));
            }
            self.values
                .lock()
                .map_err(|_| SecretError::backend("memory secret store poisoned"))?
                .remove(&reference.account())
                .map(|_| ())
                .ok_or_else(SecretError::not_found)
        }
    }

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open database");
        migrate(&mut connection).expect("migrate database");
        connection
    }

    fn input(base_url: String) -> ProviderProfileInput {
        ProviderProfileInput {
            id: None,
            kind: ProviderKind::OpenAiCompatible,
            display_name: "Primary provider".to_string(),
            base_url,
            model_id: Some("test-model".to_string()),
            credential_name: Some("api_key".to_string()),
            enabled: true,
        }
    }

    #[test]
    fn profile_responses_report_secret_status_without_exposing_secret_metadata() {
        let mut connection = database();
        let store = MemorySecretStore::default();
        let saved = save_profile(
            &mut connection,
            "workspace-a",
            &store,
            input("https://provider.example/v1".to_string()),
        )
        .expect("save profile");
        assert!(!saved.secret_configured);
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.secret_generation, 0);
        assert!(list_profile_responses(&connection, "workspace-b", &store)
            .expect("list other workspace")
            .is_empty());

        let reference = SecretRef::new(saved.id, "api_key").expect("secret reference");
        store
            .set(
                &reference,
                &SecretValue::new("sk-test-secret").expect("secret value"),
            )
            .expect("set secret");
        let listed =
            list_profile_responses(&connection, "workspace-a", &store).expect("list profiles");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].secret_configured);

        let generation_one = SecretRef::at_generation(saved.id, "api_key", 1).unwrap();
        store
            .set(
                &generation_one,
                &SecretValue::new("sk-generation-one").unwrap(),
            )
            .unwrap();
        provider_profiles::activate_secret_generation(
            &connection,
            "workspace-a",
            saved.id,
            "api_key",
            0,
        )
        .unwrap();
        store.delete(&reference).unwrap();
        let listed =
            list_profile_responses(&connection, "workspace-a", &store).expect("list profiles");
        assert!(listed[0].secret_configured);
        assert_eq!(listed[0].secret_generation, 1);

        let value = serde_json::to_value(&listed[0]).expect("serialize profile response");
        assert_eq!(value["secret_configured"], true);
        assert_eq!(value["revision"], 1);
        assert_eq!(value["secret_generation"], 1);
        let serialized = serde_json::to_string(&value).expect("serialize JSON");
        for forbidden in [
            "secret_ref",
            "credential_name",
            "sk-test-secret",
            "secret_value",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn rejects_invalid_credential_names_before_saving() {
        let mut connection = database();
        let store = MemorySecretStore::default();
        let mut profile = input("https://provider.example/v1".to_string());
        profile.credential_name = Some("../api-key".to_string());
        assert!(
            save_profile(&mut connection, "workspace-a", &store, profile)
                .unwrap_err()
                .contains("invalid_secret_reference")
        );
    }

    #[test]
    fn empty_profile_id_generates_a_uuid() {
        let input: ProviderProfileInput = serde_json::from_value(serde_json::json!({
            "id": "",
            "kind": "open_ai_compatible",
            "display_name": "Primary provider",
            "base_url": "https://provider.example/v1",
            "model_id": "test-model",
            "credential_name": "api_key",
            "enabled": true
        }))
        .expect("deserialize profile input");

        assert_ne!(input.into_profile().expect("profile").id, Uuid::nil());
    }

    #[test]
    fn selecting_a_provider_persists_the_capability_default() {
        let mut connection = database();
        let store = MemorySecretStore::default();
        let saved = save_profile(
            &mut connection,
            "workspace-a",
            &store,
            input("https://provider.example/v1".to_string()),
        )
        .expect("save profile");

        set_default_profile(
            &mut connection,
            "workspace-a",
            ProviderCapability::Chat,
            Some(saved.id),
        )
        .expect("set default profile");

        assert_eq!(
            provider_profiles::get_default(&connection, "workspace-a", ProviderCapability::Chat)
                .expect("read default")
                .expect("default profile")
                .id,
            saved.id
        );
    }

    #[test]
    fn deleting_a_profile_removes_its_configured_secret() {
        let mut connection = database();
        let store = MemorySecretStore::default();
        let saved = save_profile(
            &mut connection,
            "workspace-a",
            &store,
            input("https://provider.example/v1".to_string()),
        )
        .expect("save profile");
        let reference = SecretRef::new(saved.id, "api_key").expect("secret reference");
        store
            .set(
                &reference,
                &SecretValue::new("sk-test-secret").expect("secret value"),
            )
            .expect("set secret");

        delete_profile(&mut connection, "workspace-a", &store, saved.id).expect("delete profile");

        assert!(provider_profiles::get(&connection, "workspace-a", saved.id)
            .expect("get deleted profile")
            .is_none());
        assert!(store.get(&reference).unwrap_err().is_not_found());
    }

    #[test]
    fn changing_a_credential_name_removes_all_old_generations() {
        let mut connection = database();
        let store = MemorySecretStore::default();
        let saved = save_profile(
            &mut connection,
            "workspace-a",
            &store,
            input("https://provider.example/v1".to_string()),
        )
        .expect("save profile");
        let first = SecretRef::at_generation(saved.id, "api_key", 0).unwrap();
        store
            .set(&first, &SecretValue::new("old-generation-zero").unwrap())
            .unwrap();
        provider_profiles::activate_secret_generation(
            &connection,
            "workspace-a",
            saved.id,
            "api_key",
            0,
        )
        .unwrap();
        let second = SecretRef::at_generation(saved.id, "api_key", 1).unwrap();
        store
            .set(&second, &SecretValue::new("old-generation-one").unwrap())
            .unwrap();

        let renamed = ProviderProfileInput {
            id: Some(saved.id.to_string()),
            credential_name: Some("token".to_string()),
            ..input("https://provider.example/v2".to_string())
        };
        let renamed = save_profile(&mut connection, "workspace-a", &store, renamed)
            .expect("rename provider credential");

        assert_eq!(renamed.secret_generation, 0);
        assert!(store.get(&first).unwrap_err().is_not_found());
        assert!(store.get(&second).unwrap_err().is_not_found());
    }

    #[test]
    fn failed_old_credential_cleanup_rolls_back_profile_rename() {
        let mut connection = database();
        let store = MemorySecretStore::default();
        let saved = save_profile(
            &mut connection,
            "workspace-a",
            &store,
            input("https://provider.example/v1".to_string()),
        )
        .expect("save profile");
        let reference = SecretRef::at_generation(saved.id, "api_key", 0).unwrap();
        store
            .set(&reference, &SecretValue::new("old-secret").unwrap())
            .unwrap();
        let failing_store = MemorySecretStore {
            fail_delete: true,
            ..store
        };
        let renamed = ProviderProfileInput {
            id: Some(saved.id.to_string()),
            credential_name: Some("token".to_string()),
            ..input("https://provider.example/v2".to_string())
        };

        let error = save_profile(&mut connection, "workspace-a", &failing_store, renamed)
            .expect_err("credential cleanup failure must fail the save");

        assert!(error.contains("injected delete failure"));
        let record = provider_profiles::get_record(&connection, "workspace-a", saved.id)
            .unwrap()
            .unwrap();
        assert_eq!(record.profile.secret_ref.as_deref(), Some("api_key"));
        assert_eq!(record.profile.base_url, "https://provider.example/v1");
    }

    #[test]
    fn oversized_secret_generation_is_rejected_before_cleanup_loop() {
        let mut connection = database();
        let store = MemorySecretStore::default();
        let saved = save_profile(
            &mut connection,
            "workspace-a",
            &store,
            input("https://provider.example/v1".to_string()),
        )
        .expect("save profile");
        connection
            .execute(
                "UPDATE provider_profiles SET secret_generation = ?1 WHERE id = ?2",
                rusqlite::params![4097_i64, saved.id.to_string()],
            )
            .unwrap();
        let renamed = ProviderProfileInput {
            id: Some(saved.id.to_string()),
            credential_name: Some("token".to_string()),
            ..input("https://provider.example/v2".to_string())
        };

        let error = save_profile(&mut connection, "workspace-a", &store, renamed)
            .expect_err("corrupt secret generation must be rejected");

        assert!(error.contains("provider secret generation is too large"));
    }

    #[test]
    fn partial_old_credential_cleanup_restores_deleted_generations() {
        let mut connection = database();
        let store = MemorySecretStore::default();
        let saved = save_profile(
            &mut connection,
            "workspace-a",
            &store,
            input("https://provider.example/v1".to_string()),
        )
        .expect("save profile");
        let first = SecretRef::at_generation(saved.id, "api_key", 0).unwrap();
        store
            .set(&first, &SecretValue::new("old-generation-zero").unwrap())
            .unwrap();
        provider_profiles::activate_secret_generation(
            &connection,
            "workspace-a",
            saved.id,
            "api_key",
            0,
        )
        .unwrap();
        let second = SecretRef::at_generation(saved.id, "api_key", 1).unwrap();
        store
            .set(&second, &SecretValue::new("old-generation-one").unwrap())
            .unwrap();

        let failing_store = MemorySecretStore {
            fail_delete_on_call: Some(2),
            ..store
        };
        let renamed = ProviderProfileInput {
            id: Some(saved.id.to_string()),
            credential_name: Some("token".to_string()),
            ..input("https://provider.example/v2".to_string())
        };

        save_profile(&mut connection, "workspace-a", &failing_store, renamed)
            .expect_err("partial credential cleanup must fail the profile update");

        assert_eq!(
            failing_store.get(&first).unwrap(),
            SecretValue::new("old-generation-zero").unwrap()
        );
        assert_eq!(
            failing_store.get(&second).unwrap(),
            SecretValue::new("old-generation-one").unwrap()
        );
        let record = provider_profiles::get_record(&connection, "workspace-a", saved.id)
            .unwrap()
            .unwrap();
        assert_eq!(record.profile.secret_ref.as_deref(), Some("api_key"));
        assert_eq!(record.profile.base_url, "https://provider.example/v1");
    }

    #[test]
    fn missing_profile_secret_is_treated_as_an_uncredentialed_probe() {
        let store = MemorySecretStore::default();
        let profile = input("https://provider.example/v1".to_string())
            .into_profile()
            .expect("profile");

        assert!(profile_credential(&profile, 0, &store)
            .expect("load optional credential")
            .is_none());
    }

    #[test]
    fn provider_probe_is_bounded_and_never_returns_the_bearer_credential() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider probe");
        listener
            .set_nonblocking(true)
            .expect("set listener nonblocking");
        let address = listener.local_addr().expect("provider address");
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .expect("set probe stream blocking");
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .expect("set read timeout");
                        let mut request = [0u8; 4096];
                        let size = stream.read(&mut request).expect("read probe request");
                        stream
                            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                            .expect("write probe response");
                        return String::from_utf8_lossy(&request[..size]).to_string();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "probe connection timed out");
                        thread::yield_now();
                    }
                    Err(error) => panic!("accept provider probe: {error}"),
                }
            }
        });
        let profile = input(format!("http://{address}"));
        let credential = SecretValue::new("sk-test-secret").expect("secret value");
        let result = tauri::async_runtime::block_on(probe_provider(
            profile.into_profile().expect("profile"),
            Some(credential),
            None,
        ));
        assert!(result.ok);
        assert_eq!(result.status_code, Some(204));
        assert_eq!(result.error_code, None);
        let request = server.join().expect("join provider server");
        assert!(
            request.contains("authorization: Bearer sk-test-secret")
                || request.contains("Authorization: Bearer sk-test-secret")
        );
        assert!(!serde_json::to_string(&result)
            .expect("serialize probe result")
            .contains("sk-test-secret"));
    }

    #[test]
    fn provider_probe_reports_authentication_without_reading_a_body() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider probe");
        let address = listener.local_addr().expect("provider address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept provider probe");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).expect("read probe request");
            stream
                .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 999999\r\n\r\n")
                .expect("write probe response");
        });
        let profile = input(format!("http://{address}"));
        let result = tauri::async_runtime::block_on(probe_provider(
            profile.into_profile().expect("profile"),
            None,
            None,
        ));
        server.join().expect("join provider server");
        assert!(!result.ok);
        assert_eq!(result.status_code, Some(401));
        assert_eq!(result.error_code.as_deref(), Some("authentication"));
    }

    #[test]
    fn siliconflow_embedding_probe_calls_the_real_embedding_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind SiliconFlow probe");
        let address = listener.local_addr().expect("provider address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept provider probe");
            let mut request = [0u8; 4096];
            let size = stream.read(&mut request).expect("read probe request");
            let request = String::from_utf8_lossy(&request[..size]);
            assert!(request.starts_with("POST /v1/embeddings "));
            assert!(request.contains("authorization: Bearer sk-siliconflow"));
            let body = r#"{"data":[{"index":0,"embedding":[0.1,0.2]}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write probe response");
        });
        let profile = ProviderProfile {
            id: Uuid::new_v4(),
            kind: ProviderKind::SiliconFlow,
            display_name: "SiliconFlow".to_string(),
            base_url: format!("http://{address}/v1"),
            model_id: Some("BAAI/bge-m3".to_string()),
            secret_ref: Some("api_key".to_string()),
            enabled: true,
        };
        let result = tauri::async_runtime::block_on(probe_provider(
            profile,
            Some(SecretValue::new("sk-siliconflow").expect("secret")),
            Some(ProviderCapability::Embedding),
        ));
        server.join().expect("join provider probe");
        assert!(result.ok);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.error_code, None);
    }

    #[test]
    fn provider_probe_uses_stable_codes_for_transport_and_http_statuses() {
        let insecure = input("http://provider.example/v1".to_string());
        let result = tauri::async_runtime::block_on(probe_provider(
            insecure.into_profile().expect("insecure profile"),
            Some(SecretValue::new("sk-test-secret").expect("secret value")),
            None,
        ));
        assert!(!result.ok);
        assert_eq!(result.status_code, None);
        assert_eq!(result.error_code.as_deref(), Some("insecure_transport"));

        for (status, expected_ok, expected_error) in [
            ("404 Not Found", true, None),
            ("429 Too Many Requests", false, Some("quota")),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider probe");
            let address = listener.local_addr().expect("provider address");
            let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept provider probe");
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request).expect("read probe request");
                stream
                    .write_all(response.as_bytes())
                    .expect("write probe response");
            });
            let profile = input(format!("http://{address}"));
            let result = tauri::async_runtime::block_on(probe_provider(
                profile.into_profile().expect("profile"),
                None,
                None,
            ));
            server.join().expect("join provider server");
            assert_eq!(result.ok, expected_ok);
            assert_eq!(result.error_code.as_deref(), expected_error);
        }
    }
}
