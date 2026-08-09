use super::{
    logic,
    types::{McpHealth, McpToolSummary},
};
use crate::{app::mcp_runtime::McpRuntimeState, db::DbState, storage::secrets::SecretState};

#[tauri::command]
pub(crate) async fn check_mcp_server(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    runtime: tauri::State<'_, McpRuntimeState>,
    id: String,
) -> Result<McpHealth, String> {
    let id = logic::parse_id(&id)?;
    let config = logic::load_config(&db, id)?;
    if let Some(health) = logic::inspect_active(&runtime, id, &config.server_id).await? {
        return Ok(health);
    }
    Ok(logic::inspect_ephemeral(&secrets, &config).await)
}

#[tauri::command]
pub(crate) async fn restart_mcp_server(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    runtime: tauri::State<'_, McpRuntimeState>,
    id: String,
) -> Result<McpHealth, String> {
    let id = logic::parse_id(&id)?;
    let config = logic::load_config(&db, id)?;
    logic::shutdown_active(&runtime, id).await?;
    match logic::connect(&secrets, &config).await {
        Ok(mut supervisor) => match logic::inspect(&supervisor, &config.server_id).await {
            Ok(health) => {
                runtime.insert(id, supervisor)?;
                Ok(health)
            }
            Err(error) => {
                let _ = supervisor.shutdown().await;
                Ok(logic::health_from_error(error))
            }
        },
        Err(error) => Ok(logic::health_from_error(error.to_string())),
    }
}

#[tauri::command]
pub(crate) async fn list_mcp_tools(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    runtime: tauri::State<'_, McpRuntimeState>,
    id: String,
) -> Result<Vec<McpToolSummary>, String> {
    let id = logic::parse_id(&id)?;
    let config = logic::load_config(&db, id)?;
    if let Some(supervisor) = runtime.get(id)? {
        let guard = supervisor.lock().await;
        let tools = guard
            .client()
            .map_err(|error| error.to_string())?
            .list_tools()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(tools
            .into_iter()
            .map(|tool| logic::tool_summary(&config.server_id, tool))
            .collect());
    }
    let mut supervisor = logic::connect(&secrets, &config)
        .await
        .map_err(|error| error.to_string())?;
    let result = supervisor
        .client()
        .map_err(|error| error.to_string())?
        .list_tools()
        .await
        .map(|tools| {
            tools
                .into_iter()
                .map(|tool| logic::tool_summary(&config.server_id, tool))
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string());
    let shutdown = supervisor
        .shutdown()
        .await
        .map_err(|error| error.to_string());
    match (result, shutdown) {
        (Ok(tools), Ok(())) => Ok(tools),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(shutdown_error)) => {
            Err(format!("{error}; shutdown failed: {shutdown_error}"))
        }
    }
}
