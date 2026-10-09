use crate::agent::profiles::{self, AgentProfile};
use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ToolCapabilitySummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub source: String,
}

#[tauri::command]
pub fn list_agent_profiles(db: tauri::State<DbState>) -> Result<Vec<AgentProfile>, String> {
    with_conn(&db, |connection| {
        profiles::list(connection, current_workspace_id())
    })
}

#[tauri::command]
pub fn list_agent_profile_presets() -> Vec<AgentProfile> {
    profiles::presets()
}

#[tauri::command]
pub fn save_agent_profile(
    db: tauri::State<DbState>,
    profile: AgentProfile,
) -> Result<AgentProfile, String> {
    with_conn_mut(&db, |connection| {
        profiles::save(connection, current_workspace_id(), profile)
    })
}

#[tauri::command]
pub fn reset_agent_profile(db: tauri::State<DbState>, id: String) -> Result<AgentProfile, String> {
    with_conn_mut(&db, |connection| {
        profiles::reset(connection, current_workspace_id(), &id)
    })
}

#[tauri::command]
pub fn delete_agent_profile(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    with_conn_mut(&db, |connection| {
        profiles::delete(connection, current_workspace_id(), &id)
    })
}

#[tauri::command]
pub fn list_tool_capabilities() -> Vec<ToolCapabilitySummary> {
    profiles::builtin_tool_capabilities()
        .into_iter()
        .map(|(id, name, description)| ToolCapabilitySummary {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            enabled: true,
            source: "builtin".to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{list_agent_profile_presets, list_tool_capabilities};

    #[test]
    fn exposes_the_documented_agent_roster() {
        let agents = list_agent_profile_presets();
        assert_eq!(agents.len(), 9);
        assert_eq!(agents[0].id, "master");
        assert!(agents.iter().all(|agent| agent.enabled));
    }

    #[test]
    fn exposes_actual_runtime_tool_ids() {
        let tools = list_tool_capabilities();
        assert!(tools
            .iter()
            .any(|tool| tool.id == "steel.predict_performance" && tool.enabled));
        assert!(tools
            .iter()
            .all(|tool| tool.source == "builtin" && tool.id.contains('.')));
    }
}
