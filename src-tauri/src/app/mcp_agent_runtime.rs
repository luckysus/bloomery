use crate::{
    app::{mcp_commands, mcp_runtime::McpRuntimeState},
    mcp::{McpServerConfig, McpToolExecutor},
    storage::secrets::SecretState,
};
use tauri::Manager;

pub(crate) async fn load_enabled_tools_for_query(
    app: &tauri::AppHandle,
    configs: Vec<McpServerConfig>,
    query: &str,
) -> Result<McpToolExecutor, String> {
    let runtime = app.state::<McpRuntimeState>();
    let secrets = app.state::<SecretState>();
    let mut supervisors = Vec::new();
    for config in configs.into_iter().filter(|config| config.enabled) {
        let supervisor = match runtime.get(config.id)? {
            Some(supervisor) => supervisor,
            None => {
                let Ok(supervisor) = mcp_commands::connect_server(&secrets, &config).await else {
                    continue;
                };
                runtime.insert(config.id, supervisor)?;
                let Some(supervisor) = runtime.get(config.id)? else {
                    continue;
                };
                supervisor
            }
        };
        supervisors.push(supervisor);
    }
    McpToolExecutor::from_supervisors_for_query(supervisors, query)
        .await
        .map_err(|error| format!("load MCP tools failed: {error}"))
}
