use crate::{
    app::mcp_runtime::McpRuntimeState,
    db::{current_workspace_id, with_conn, with_conn_mut, DbState},
    mcp::{McpError, McpServerConfig, McpSupervisor, McpTool, McpTransportKind},
    storage::{repositories::mcp as mcp_repository, secrets::SecretState},
};
use chrono::Utc;
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;

use super::types::{McpHealth, McpServerInput, McpServerSummary, McpToolSummary};

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
    let bearer_configured = read_secret(secrets, config.id, "bearer")?.is_some();
    let mut all_environment_secrets_configured = !config.env_names.is_empty();
    for name in &config.env_names {
        if read_secret(secrets, config.id, &env_credential_name(name))?.is_none() {
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
    let env_names = if input.env_values.is_empty() {
        existing
            .map(|value| value.env_names.clone())
            .unwrap_or_default()
    } else {
        input.env_values.keys().cloned().collect()
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
        inherited_env: input.inherited_env,
        env_names,
        timeout,
        enabled: input.enabled,
    };
    config.normalize().map_err(|error| error.to_string())?;
    Ok(config)
}

pub(super) fn save_secret_updates(
    secrets: &SecretState,
    id: Uuid,
    input: &McpServerInput,
) -> Result<(), String> {
    if input.clear_bearer_token || input.bearer_token.as_deref() == Some("") {
        delete_secret(secrets, id, "bearer")?;
    } else if let Some(value) = input.bearer_token.as_deref() {
        set_secret(secrets, id, "bearer", value)?;
    }
    for (name, value) in &input.env_values {
        set_secret(secrets, id, &env_credential_name(name), value)?;
    }
    Ok(())
}

pub(super) fn delete_secrets(
    secrets: &SecretState,
    id: Uuid,
    env_names: &[String],
) -> Result<(), String> {
    delete_secret(secrets, id, "bearer")?;
    for name in env_names {
        delete_secret(secrets, id, &env_credential_name(name))?;
    }
    Ok(())
}

pub(super) fn credentials(
    secrets: &SecretState,
    config: &McpServerConfig,
) -> Result<(Option<String>, BTreeMap<String, String>), String> {
    let bearer = read_secret(secrets, config.id, "bearer")?;
    let mut env = BTreeMap::new();
    for name in &config.env_names {
        match read_secret(secrets, config.id, &env_credential_name(name))? {
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
    McpHealth {
        status: "failed".to_string(),
        server_name: None,
        server_version: None,
        tool_count: 0,
        resource_count: 0,
        prompt_count: 0,
        tools: Vec::new(),
        error: Some(error.into()),
        checked_at: Utc::now().to_rfc3339(),
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

fn set_secret(secrets: &SecretState, id: Uuid, name: &str, value: &str) -> Result<(), String> {
    let value =
        crate::storage::secrets::SecretValue::new(value).map_err(|error| error.to_string())?;
    let reference =
        crate::storage::secrets::SecretRef::new(id, name).map_err(|error| error.to_string())?;
    secrets
        .store()
        .set(&reference, &value)
        .map_err(|error| error.to_string())
}

fn read_secret(secrets: &SecretState, id: Uuid, name: &str) -> Result<Option<String>, String> {
    let reference =
        crate::storage::secrets::SecretRef::new(id, name).map_err(|error| error.to_string())?;
    match secrets.store().get(&reference) {
        Ok(value) => Ok(Some(value.expose().to_string())),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn delete_secret(secrets: &SecretState, id: Uuid, name: &str) -> Result<(), String> {
    let reference =
        crate::storage::secrets::SecretRef::new(id, name).map_err(|error| error.to_string())?;
    match secrets.store().delete(&reference) {
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

pub(super) fn save_config(
    db: &tauri::State<'_, DbState>,
    config: &McpServerConfig,
) -> Result<(), String> {
    with_conn_mut(db, |connection| {
        mcp_repository::save(connection, current_workspace_id(), config)
    })
}

pub(super) fn delete_config(db: &tauri::State<'_, DbState>, id: Uuid) -> Result<(), String> {
    with_conn_mut(db, |connection| {
        mcp_repository::delete(connection, current_workspace_id(), id)
    })
}
