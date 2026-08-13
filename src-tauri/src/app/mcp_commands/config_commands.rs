use super::{
    logic,
    types::{McpServerInput, McpServerSummary},
};
use crate::{
    app::mcp_runtime::McpRuntimeState,
    db::{current_workspace_id, with_conn, DbState},
    storage::{repositories::mcp as mcp_repository, secrets::SecretState},
};

#[tauri::command]
pub(crate) fn list_mcp_servers(
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
) -> Result<Vec<McpServerSummary>, String> {
    with_conn(&db, |connection| {
        mcp_repository::list(connection, current_workspace_id())?
            .into_iter()
            .map(|config| logic::summary(config, &secrets))
            .collect()
    })
}

#[tauri::command]
pub(crate) async fn save_mcp_server(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    runtime: tauri::State<'_, McpRuntimeState>,
    input: McpServerInput,
) -> Result<McpServerSummary, String> {
    let existing = input
        .id
        .as_deref()
        .map(logic::parse_id)
        .transpose()?
        .map(|id| logic::load_config(&db, id))
        .transpose()?;
    let config = logic::input_config(input.clone(), existing.as_ref())?;
    crate::db::with_conn_mut(&db, |connection| {
        logic::save_config_and_secrets(
            connection,
            crate::db::current_workspace_id(),
            secrets.store(),
            &config,
            &input,
            existing.as_ref(),
        )
    })?;
    logic::shutdown_active(&runtime, config.id).await?;
    logic::summary(config, &secrets)
}

#[tauri::command]
pub(crate) async fn delete_mcp_server(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    runtime: tauri::State<'_, McpRuntimeState>,
    id: String,
) -> Result<(), String> {
    let id = logic::parse_id(&id)?;
    let config = logic::load_config(&db, id)?;
    logic::shutdown_active(&runtime, id).await?;
    crate::db::with_conn_mut(&db, |connection| {
        logic::delete_config_and_secrets(
            connection,
            crate::db::current_workspace_id(),
            secrets.store(),
            id,
            &config,
        )
    })
}
