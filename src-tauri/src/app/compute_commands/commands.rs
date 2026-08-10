use super::logic::{self, TrainSteelDatasetRequest};
use crate::app::task_commands::tasks::BackgroundTaskResponse;
use crate::db::DbState;
use serde_json::Value;

#[tauri::command]
pub fn train_steel_dataset(
    db: tauri::State<DbState>,
    request: TrainSteelDatasetRequest,
) -> Result<BackgroundTaskResponse, String> {
    logic::train_steel_dataset(db, request)
}

#[tauri::command]
pub fn get_compute_training_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    logic::get_compute_training_result(db, id)
}
