use crate::{
    agent::protocol::PermissionRisk,
    app::mcp_runtime::McpRuntimeState,
    db::{current_workspace_id, with_conn, DbState},
    mcp::{McpError, McpServerConfig, McpSupervisor, McpTool, McpTransportKind},
    storage::{
        repositories::mcp as mcp_repository,
        secrets::{SecretRef, SecretState, SecretStore, SecretValue},
    },
};
use chrono::Utc;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use uuid::Uuid;

use super::types::{McpDiagnostic, McpHealth, McpServerInput, McpServerSummary, McpToolSummary};

pub(super) fn parse_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value.trim()).map_err(|_| "MCP server id must be a UUID".to_string())
}

pub(super) fn load_config(
    db: &tauri::State<'_, DbState>,
    id: Uuid,
) -> Result<McpServerConfig, String> {
    with_conn(db, |connection| {
        mcp_repository::get(connection, current_workspace_id(), id)?
            .ok_or_else(|| "MCP server not found".to_string())
    })
}

pub(super) fn summary(
    config: McpServerConfig,
    secrets: &SecretState,
) -> Result<McpServerSummary, String> {
    let bearer_configured = read_secret(secrets.store(), config.id, "bearer")?.is_some();
    let mut all_environment_secrets_configured = !config.env_names.is_empty();
    for name in &config.env_names {
        if read_secret(secrets.store(), config.id, &env_credential_name(name))?.is_none() {
            all_environment_secrets_configured = false;
            break;
        }
    }
    let secret_configured = bearer_configured || all_environment_secrets_configured;
    let timeout_ms = config.timeout_ms().map_err(|error| error.to_string())?;
    let env_names = config.env_names.clone();
    Ok(McpServerSummary {
        id: config.id.to_string(),
        display_name: config.display_name,
        server_id: config.server_id,
        transport: config.transport,
        url: config.url,
        executable: config
            .executable
            .map(|value| value.to_string_lossy().to_string()),
        args: config.args,
        working_directory: config
            .working_directory
            .map(|value| value.to_string_lossy().to_string()),
        inherited_env: config.inherited_env,
        env_names,
        timeout_ms,
        enabled: config.enabled,
        secret_configured,
        status: "unknown".to_string(),
        last_error: None,
        last_checked_at: None,
        tool_count: 0,
    })
}

pub(super) fn input_config(
    input: McpServerInput,
    existing: Option<&McpServerConfig>,
) -> Result<McpServerConfig, String> {
    let id = input
        .id
        .as_deref()
        .map(parse_id)
        .transpose()?
        .or_else(|| existing.map(|value| value.id))
        .unwrap_or_else(Uuid::new_v4);
    let env_names = if input.clear_environment_credentials || !input.env_values.is_empty() {
        input.env_values.keys().cloned().collect()
    } else {
        existing
            .map(|value| value.env_names.clone())
            .unwrap_or_default()
    };
    let inherited_env = if input.replace_inherited_env || existing.is_none() {
        input.inherited_env
    } else {
        existing
            .map(|value| value.inherited_env.clone())
            .unwrap_or_default()
    };
    let timeout = Duration::from_millis(input.timeout_ms);
    let mut config = McpServerConfig {
        id,
        display_name: input.display_name,
        server_id: input.server_id,
        transport: input.transport,
        url: input.url,
        executable: input.executable.map(Into::into),
        args: input.args,
        working_directory: input.working_directory.map(Into::into),
        inherited_env,
        env_names,
        timeout,
        enabled: input.enabled,
    };
    config.normalize().map_err(|error| error.to_string())?;
    Ok(config)
}

pub(super) fn credentials(
    secrets: &SecretState,
    config: &McpServerConfig,
) -> Result<(Option<String>, BTreeMap<String, String>), String> {
    let bearer = read_secret(secrets.store(), config.id, "bearer")?;
    let mut env = BTreeMap::new();
    for name in &config.env_names {
        match read_secret(secrets.store(), config.id, &env_credential_name(name))? {
            Some(value) => {
                env.insert(name.clone(), value);
            }
            None if matches!(config.transport, McpTransportKind::Stdio) => {
                return Err(format!(
                    "MCP environment credential is not configured: {name}"
                ));
            }
            None => {}
        }
    }
    Ok((bearer, env))
}

pub(super) async fn connect(
    secrets: &SecretState,
    config: &McpServerConfig,
) -> Result<McpSupervisor, McpError> {
    let (bearer, env) = credentials(secrets, config).map_err(McpError::Transport)?;
    let transport = config.transport(bearer, env)?;
    McpSupervisor::connect(
        transport,
        crate::mcp::McpClientConfig {
            server_id: config.server_id.clone(),
            request_timeout: config.timeout,
            ..crate::mcp::McpClientConfig::default()
        },
    )
    .await
}

pub(super) async fn inspect(
    supervisor: &McpSupervisor,
    configured_server_id: &str,
) -> Result<McpHealth, String> {
    let client = supervisor.client().map_err(|error| error.to_string())?;
    let identity = client.server_identity().clone();
    let capabilities = client.capabilities().clone();
    let tools = client
        .list_tools()
        .await
        .map_err(|error| error.to_string())?;
    let resource_count = if capabilities.resources {
        client
            .list_resources()
            .await
            .map_err(|error| error.to_string())?
            .len()
    } else {
        0
    };
    let prompt_count = if capabilities.prompts {
        client
            .list_prompts()
            .await
            .map_err(|error| error.to_string())?
            .len()
    } else {
        0
    };
    Ok(health_from_tools(
        configured_server_id.to_string(),
        identity.name,
        identity.version,
        tools,
        resource_count,
        prompt_count,
    ))
}

pub(super) fn health_from_error(error: impl Into<String>) -> McpHealth {
    let error = error.into();
    McpHealth {
        status: "failed".to_string(),
        server_name: None,
        server_version: None,
        tool_count: 0,
        resource_count: 0,
        prompt_count: 0,
        tools: Vec::new(),
        error: Some(error.clone()),
        diagnostic: Some(diagnostic_from_error(&error)),
        checked_at: Utc::now().to_rfc3339(),
    }
}

fn diagnostic_from_error(error: &str) -> McpDiagnostic {
    let lower = error.to_lowercase();
    if lower.contains("credential is not configured") || lower.contains("missing credential") {
        return McpDiagnostic {
            code: "missing_credential".to_string(),
            message: "MCP server is missing a configured credential.".to_string(),
            suggested_action: "Edit the server and save the required token or environment value; Bloomery stores it in Windows Credential Manager.".to_string(),
        };
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return McpDiagnostic {
            code: "timeout".to_string(),
            message: "MCP server did not answer before the timeout.".to_string(),
            suggested_action: "Check the server process or URL, then increase the timeout only if the server is normally slow.".to_string(),
        };
    }
    if lower.contains("invalid mcp") || lower.contains("invalid transport") {
        return McpDiagnostic {
            code: "invalid_transport".to_string(),
            message: "MCP transport configuration is invalid.".to_string(),
            suggested_action:
                "Check whether the transport type matches the executable or URL configuration."
                    .to_string(),
        };
    }
    if lower.contains("failed to start") {
        return McpDiagnostic {
            code: "process_start_failed".to_string(),
            message: "Bloomery could not start the MCP stdio process.".to_string(),
            suggested_action: "Check the executable path, arguments, working directory, and inherited environment allowlist.".to_string(),
        };
    }
    McpDiagnostic {
        code: "connection_failed".to_string(),
        message: "MCP connection failed.".to_string(),
        suggested_action: "Check the server configuration and run the server manually once if the error is unclear.".to_string(),
    }
}

fn health_from_tools(
    configured_server_id: String,
    server_name: String,
    server_version: String,
    tools: Vec<McpTool>,
    resource_count: usize,
    prompt_count: usize,
) -> McpHealth {
    let summaries = tools
        .into_iter()
        .map(|tool| tool_summary(&configured_server_id, tool))
        .collect::<Vec<_>>();
    McpHealth {
        status: "healthy".to_string(),
        server_name: Some(server_name),
        server_version: Some(server_version),
        tool_count: summaries.len(),
        resource_count,
        prompt_count,
        tools: summaries,
        error: None,
        diagnostic: None,
        checked_at: Utc::now().to_rfc3339(),
    }
}

pub(super) fn tool_summary(server_id: &str, tool: McpTool) -> McpToolSummary {
    McpToolSummary {
        id: format!(
            "mcp.{}.{}",
            stable_segment(server_id),
            stable_segment(&tool.name)
        ),
        name: tool.name,
        description: tool.description.unwrap_or_default(),
        read_only: tool.read_only_hint,
        risk: if tool.read_only_hint {
            PermissionRisk::Automatic
        } else {
            PermissionRisk::ConfirmationRequired
        },
    }
}

fn stable_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .replace("..", "_")
}

fn env_credential_name(name: &str) -> String {
    format!("mcp_env_{name}")
}

fn set_secret(store: &dyn SecretStore, id: Uuid, name: &str, value: &str) -> Result<(), String> {
    let value = SecretValue::new(value).map_err(|error| error.to_string())?;
    let reference = SecretRef::new(id, name).map_err(|error| error.to_string())?;
    store
        .set(&reference, &value)
        .map_err(|error| error.to_string())
}

fn read_secret_value(
    store: &dyn SecretStore,
    id: Uuid,
    name: &str,
) -> Result<Option<SecretValue>, String> {
    let reference = SecretRef::new(id, name).map_err(|error| error.to_string())?;
    match store.get(&reference) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn read_secret(store: &dyn SecretStore, id: Uuid, name: &str) -> Result<Option<String>, String> {
    match read_secret_value(store, id, name)? {
        Some(value) => Ok(Some(value.expose().to_string())),
        None => Ok(None),
    }
}

fn delete_secret(store: &dyn SecretStore, id: Uuid, name: &str) -> Result<(), String> {
    let reference = SecretRef::new(id, name).map_err(|error| error.to_string())?;
    match store.delete(&reference) {
        Ok(()) => Ok(()),
        Err(error) if error.is_not_found() => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) async fn inspect_ephemeral(
    secrets: &SecretState,
    config: &McpServerConfig,
) -> McpHealth {
    match connect(secrets, config).await {
        Ok(mut supervisor) => {
            let result = inspect(&supervisor, &config.server_id).await;
            let shutdown = supervisor.shutdown().await;
            match (result, shutdown) {
                (Ok(health), Ok(())) => health,
                (Ok(_), Err(error)) => health_from_error(error.to_string()),
                (Err(error), Ok(())) => health_from_error(error),
                (Err(error), Err(shutdown_error)) => {
                    health_from_error(format!("{error}; shutdown failed: {shutdown_error}"))
                }
            }
        }
        Err(error) => health_from_error(error.to_string()),
    }
}

pub(super) async fn inspect_active(
    runtime: &McpRuntimeState,
    id: Uuid,
    configured_server_id: &str,
) -> Result<Option<McpHealth>, String> {
    let Some(supervisor) = runtime.get(id)? else {
        return Ok(None);
    };
    let guard = supervisor.lock().await;
    Ok(Some(inspect(&guard, configured_server_id).await?))
}

pub(super) async fn shutdown_active(runtime: &McpRuntimeState, id: Uuid) -> Result<(), String> {
    let Some(supervisor) = runtime.remove(id)? else {
        return Ok(());
    };
    let result = {
        let mut guard = supervisor.lock().await;
        guard.shutdown().await.map_err(|error| error.to_string())
    };
    result
}

pub(super) fn save_config_and_secrets(
    connection: &mut rusqlite::Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    config: &McpServerConfig,
    input: &McpServerInput,
    existing: Option<&McpServerConfig>,
) -> Result<(), String> {
    let names = secret_names(config, existing);
    let snapshot = snapshot_secrets(store, config.id, &names)?;
    let desired = desired_secrets(store, config, input, &names)?;

    if let Err(error) = apply_secrets(store, config.id, &desired) {
        return Err(with_rollback(store, config.id, &snapshot, error));
    }
    if let Err(error) = mcp_repository::save(connection, workspace_id, config) {
        return Err(with_rollback(store, config.id, &snapshot, error));
    }
    Ok(())
}

pub(super) fn delete_config_and_secrets(
    connection: &mut rusqlite::Connection,
    workspace_id: &str,
    store: &dyn SecretStore,
    id: Uuid,
    config: &McpServerConfig,
) -> Result<(), String> {
    let names = secret_names(config, None);
    let snapshot = snapshot_secrets(store, id, &names)?;
    let desired = names
        .iter()
        .map(|name| (name.clone(), None))
        .collect::<BTreeMap<_, _>>();

    if let Err(error) = apply_secrets(store, id, &desired) {
        return Err(with_rollback(store, id, &snapshot, error));
    }
    if let Err(error) = mcp_repository::delete(connection, workspace_id, id) {
        return Err(with_rollback(store, id, &snapshot, error));
    }
    Ok(())
}

fn secret_names(config: &McpServerConfig, existing: Option<&McpServerConfig>) -> BTreeSet<String> {
    let mut names = BTreeSet::from(["bearer".to_string()]);
    names.extend(
        config
            .env_names
            .iter()
            .map(|name| env_credential_name(name)),
    );
    if let Some(existing) = existing {
        names.extend(
            existing
                .env_names
                .iter()
                .map(|name| env_credential_name(name)),
        );
    }
    names
}

fn snapshot_secrets(
    store: &dyn SecretStore,
    id: Uuid,
    names: &BTreeSet<String>,
) -> Result<BTreeMap<String, Option<SecretValue>>, String> {
    names
        .iter()
        .map(|name| Ok((name.clone(), read_secret_value(store, id, name)?)))
        .collect()
}

fn desired_secrets(
    store: &dyn SecretStore,
    config: &McpServerConfig,
    input: &McpServerInput,
    names: &BTreeSet<String>,
) -> Result<BTreeMap<String, Option<SecretValue>>, String> {
    let mut desired = snapshot_secrets(store, config.id, names)?;

    if input.clear_bearer_token || input.bearer_token.as_deref() == Some("") {
        desired.insert("bearer".to_string(), None);
    } else if let Some(value) = input.bearer_token.as_deref() {
        desired.insert(
            "bearer".to_string(),
            Some(SecretValue::new(value).map_err(|error| error.to_string())?),
        );
    }

    // An empty environment map means the UI did not reveal or edit secrets.
    // A non-empty map is an explicit replacement of the configured names.
    if input.clear_environment_credentials || !input.env_values.is_empty() {
        for name in names {
            if name != "bearer" {
                desired.insert(name.clone(), None);
            }
        }
        for (name, value) in &input.env_values {
            desired.insert(
                env_credential_name(name),
                Some(SecretValue::new(value).map_err(|error| error.to_string())?),
            );
        }
    }
    Ok(desired)
}

fn apply_secrets(
    store: &dyn SecretStore,
    id: Uuid,
    desired: &BTreeMap<String, Option<SecretValue>>,
) -> Result<(), String> {
    for (name, value) in desired {
        let current = read_secret_value(store, id, name)?;
        if current == *value {
            continue;
        }
        match value {
            Some(value) => set_secret(store, id, name, value.expose())?,
            None => delete_secret(store, id, name)?,
        }
    }
    Ok(())
}

fn with_rollback(
    store: &dyn SecretStore,
    id: Uuid,
    snapshot: &BTreeMap<String, Option<SecretValue>>,
    primary_error: String,
) -> String {
    match apply_secrets(store, id, snapshot) {
        Ok(()) => primary_error,
        Err(rollback_error) => {
            format!("{primary_error}; MCP credential rollback failed: {rollback_error}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mcp::McpTransportKind,
        storage::{migrations, secrets::SecretError},
    };
    use rusqlite::Connection;
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    #[derive(Clone, Default)]
    struct MemorySecretStore {
        values: Arc<Mutex<HashMap<String, SecretValue>>>,
        set_calls: Arc<Mutex<usize>>,
        fail_set_on_call: Arc<Mutex<Option<usize>>>,
        fail_delete: Arc<Mutex<bool>>,
    }

    impl SecretStore for MemorySecretStore {
        fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError> {
            let mut calls = self.set_calls.lock().expect("set call counter");
            *calls += 1;
            if self
                .fail_set_on_call
                .lock()
                .expect("set failure flag")
                .is_some_and(|call| call == *calls)
            {
                return Err(SecretError::backend("injected MCP secret write failure"));
            }
            self.values
                .lock()
                .expect("secret values")
                .insert(reference.account(), value.clone());
            Ok(())
        }

        fn get(&self, reference: &SecretRef) -> Result<SecretValue, SecretError> {
            self.values
                .lock()
                .expect("secret values")
                .get(&reference.account())
                .cloned()
                .ok_or_else(SecretError::not_found)
        }

        fn delete(&self, reference: &SecretRef) -> Result<(), SecretError> {
            if *self.fail_delete.lock().expect("delete failure flag") {
                return Err(SecretError::backend("injected MCP secret delete failure"));
            }
            self.values
                .lock()
                .expect("secret values")
                .remove(&reference.account())
                .map(|_| ())
                .ok_or_else(SecretError::not_found)
        }
    }

    fn config(id: Uuid, env_names: &[&str]) -> McpServerConfig {
        McpServerConfig {
            id,
            display_name: "Steel standards".to_string(),
            server_id: "steel-standards".to_string(),
            transport: McpTransportKind::Stdio,
            url: None,
            executable: Some(PathBuf::from("powershell.exe")),
            args: vec!["-NoProfile".to_string()],
            working_directory: None,
            inherited_env: vec!["SystemRoot".to_string()],
            env_names: env_names.iter().map(|name| (*name).to_string()).collect(),
            timeout: Duration::from_secs(30),
            enabled: true,
        }
    }

    fn input(id: Uuid, env_values: &[(&str, &str)], bearer_token: &str) -> McpServerInput {
        McpServerInput {
            id: Some(id.to_string()),
            display_name: "Steel standards v2".to_string(),
            server_id: "steel-standards-v2".to_string(),
            transport: McpTransportKind::Stdio,
            url: None,
            executable: Some("powershell.exe".to_string()),
            args: vec!["-NoProfile".to_string()],
            working_directory: None,
            inherited_env: vec!["SystemRoot".to_string()],
            replace_inherited_env: true,
            env_values: env_values
                .iter()
                .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
                .collect(),
            bearer_token: Some(bearer_token.to_string()),
            clear_bearer_token: false,
            clear_environment_credentials: false,
            timeout_ms: 30_000,
            enabled: true,
        }
    }

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open SQLite");
        migrations::migrate(&mut connection).expect("migrate SQLite");
        connection
    }

    #[test]
    fn failed_secret_update_restores_config_and_all_previous_credentials() {
        let mut connection = database();
        let id = Uuid::new_v4();
        let previous = config(id, &["OLD_KEY"]);
        mcp_repository::save(&mut connection, "local", &previous).expect("save previous config");
        let store = MemorySecretStore::default();
        set_secret(&store, id, "bearer", "old-bearer").expect("save previous bearer");
        set_secret(&store, id, &env_credential_name("OLD_KEY"), "old-env")
            .expect("save previous environment credential");
        *store.set_calls.lock().expect("reset set call counter") = 0;
        *store
            .fail_set_on_call
            .lock()
            .expect("configure set failure") = Some(2);

        let next_input = input(id, &[("NEW_KEY", "new-env")], "new-bearer");
        let next = input_config(next_input.clone(), Some(&previous)).expect("build next config");

        let error = save_config_and_secrets(
            &mut connection,
            "local",
            &store,
            &next,
            &next_input,
            Some(&previous),
        )
        .expect_err("partial secret write must fail");

        assert!(error.contains("injected MCP secret write failure"));
        assert_eq!(
            mcp_repository::get(&connection, "local", id)
                .expect("load config")
                .expect("previous config remains"),
            previous
        );
        assert_eq!(
            read_secret(&store, id, "bearer")
                .expect("read bearer")
                .as_deref(),
            Some("old-bearer")
        );
        assert_eq!(
            read_secret(&store, id, &env_credential_name("OLD_KEY"))
                .expect("read old environment credential")
                .as_deref(),
            Some("old-env")
        );
        assert!(read_secret(&store, id, &env_credential_name("NEW_KEY"))
            .expect("read new environment credential")
            .is_none());
    }

    #[test]
    fn failed_config_delete_restores_all_deleted_credentials() {
        let mut connection = database();
        let id = Uuid::new_v4();
        let previous = config(id, &["STEEL_KEY"]);
        mcp_repository::save(&mut connection, "local", &previous).expect("save config");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_mcp_delete
                 BEFORE DELETE ON mcp_servers
                 BEGIN SELECT RAISE(ABORT, 'injected MCP database delete failure'); END;",
            )
            .expect("create failure trigger");
        let store = MemorySecretStore::default();
        set_secret(&store, id, "bearer", "bearer").expect("save bearer");
        set_secret(&store, id, &env_credential_name("STEEL_KEY"), "steel-key")
            .expect("save environment credential");

        let error = delete_config_and_secrets(&mut connection, "local", &store, id, &previous)
            .expect_err("database delete failure must fail deletion");

        assert!(error.contains("injected MCP database delete failure"));
        assert!(mcp_repository::get(&connection, "local", id)
            .expect("load config")
            .is_some());
        assert_eq!(
            read_secret(&store, id, "bearer")
                .expect("read bearer")
                .as_deref(),
            Some("bearer")
        );
        assert_eq!(
            read_secret(&store, id, &env_credential_name("STEEL_KEY"))
                .expect("read environment credential")
                .as_deref(),
            Some("steel-key")
        );
    }

    #[test]
    fn editing_without_environment_changes_preserves_existing_configuration() {
        let id = Uuid::new_v4();
        let previous = config(id, &["STEEL_KEY"]);
        let input = McpServerInput {
            id: Some(id.to_string()),
            display_name: previous.display_name.clone(),
            server_id: previous.server_id.clone(),
            transport: previous.transport,
            url: previous.url.clone(),
            executable: previous
                .executable
                .as_ref()
                .map(|value| value.to_string_lossy().to_string()),
            args: previous.args.clone(),
            working_directory: None,
            inherited_env: Vec::new(),
            replace_inherited_env: false,
            env_values: BTreeMap::new(),
            bearer_token: None,
            clear_bearer_token: false,
            clear_environment_credentials: false,
            timeout_ms: previous.timeout.as_millis() as u64,
            enabled: previous.enabled,
        };

        let next = input_config(input, Some(&previous)).expect("build edited config");

        assert_eq!(next.inherited_env, previous.inherited_env);
        assert_eq!(next.env_names, previous.env_names);
    }

    #[test]
    fn explicitly_replaced_empty_inherited_environment_is_persisted() {
        let id = Uuid::new_v4();
        let previous = config(id, &["STEEL_KEY"]);
        let mut input = input(id, &[], "bearer");
        input.inherited_env = Vec::new();
        input.replace_inherited_env = true;

        let next = input_config(input, Some(&previous)).expect("build edited config");

        assert!(next.inherited_env.is_empty());
    }

    #[test]
    fn explicitly_cleared_environment_credentials_remove_configured_names() {
        let id = Uuid::new_v4();
        let previous = config(id, &["STEEL_KEY"]);
        let mut input = input(id, &[], "new-bearer");
        input.clear_environment_credentials = true;

        let next = input_config(input, Some(&previous)).expect("build edited config");

        assert!(next.env_names.is_empty());
    }

    #[test]
    fn explicitly_empty_environment_values_remove_old_credentials() {
        let id = Uuid::new_v4();
        let previous = config(id, &["STEEL_KEY"]);
        let mut input = input(id, &[], "new-bearer");
        input.clear_environment_credentials = true;
        let store = MemorySecretStore::default();
        set_secret(&store, id, &env_credential_name("STEEL_KEY"), "old-key")
            .expect("save previous environment credential");

        let names = secret_names(&previous, None);
        let desired = desired_secrets(&store, &previous, &input, &names)
            .expect("build explicitly empty desired credentials");

        assert_eq!(desired.get(&env_credential_name("STEEL_KEY")), Some(&None));
    }

    #[test]
    fn failed_health_includes_a_structured_missing_credential_diagnostic() {
        let health = health_from_error("MCP environment credential is not configured: STEEL_KEY");

        let diagnostic = health.diagnostic.expect("diagnostic");
        assert_eq!(diagnostic.code, "missing_credential");
        assert!(diagnostic.suggested_action.contains("Credential Manager"));
        assert!(!diagnostic.message.contains("STEEL_KEY"));
    }
}
