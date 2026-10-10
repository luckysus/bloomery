use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::storage::repositories::experiments::{self, ExperimentRecord, NewExperiment};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateExperimentPlanRequest {
    /// "optimization"（来自工艺优化候选，state=proposed）或 "manual"（state=draft）。
    pub source: String,
    pub title: String,
    #[serde(default)]
    pub objective_note: Option<String>,
    /// `[{name, low?, high?, value?}]`；来自优化方案时 value 是推荐取值。
    pub variables: serde_json::Value,
    /// 可选：来源候选的完整数据（含训练任务 ID 等）。
    #[serde(default)]
    pub recommendation: Option<serde_json::Value>,
}

#[tauri::command]
pub fn create_experiment_plan(
    db: tauri::State<DbState>,
    request: CreateExperimentPlanRequest,
) -> Result<ExperimentRecord, String> {
    if request.source != "optimization" && request.source != "manual" {
        return Err("experiment plan source must be optimization or manual".to_string());
    }
    let state = if request.source == "optimization" { "proposed" } else { "draft" };
    let variables_json = serde_json::to_string(&request.variables)
        .map_err(|error| format!("variables must be serializable: {error}"))?;
    let recommendation_json = match request.recommendation {
        Some(value) => Some(
            serde_json::to_string(&value)
                .map_err(|error| format!("recommendation must be serializable: {error}"))?,
        ),
        None => None,
    };
    with_conn_mut(&db, |connection| {
        experiments::create(
            connection,
            current_workspace_id(),
            NewExperiment {
                title: &request.title,
                objective: request.objective_note.as_deref(),
                variables_json: &variables_json,
                recommendation_json: recommendation_json.as_deref(),
                state,
            },
        )
    })
}

#[tauri::command]
pub fn list_experiment_plans(
    db: tauri::State<DbState>,
) -> Result<Vec<ExperimentRecord>, String> {
    with_conn(&db, |connection| {
        experiments::list(connection, current_workspace_id())
    })
}

#[tauri::command]
pub fn set_experiment_plan_state(
    db: tauri::State<DbState>,
    id: String,
    state: String,
) -> Result<ExperimentRecord, String> {
    with_conn_mut(&db, |connection| {
        experiments::set_state(connection, current_workspace_id(), &id, &state)
    })
}

#[tauri::command]
pub fn delete_experiment_plan(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    with_conn_mut(&db, |connection| {
        experiments::delete(connection, current_workspace_id(), &id)
    })
}
