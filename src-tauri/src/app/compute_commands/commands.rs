use super::logic::{
    self, PredictOnnxModelRequest, PredictSteelModelRequest, TrainSteelDatasetRequest,
};
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

#[tauri::command]
pub fn predict_steel_model(
    db: tauri::State<DbState>,
    request: PredictSteelModelRequest,
) -> Result<BackgroundTaskResponse, String> {
    logic::predict_steel_model(db, request)
}

#[tauri::command]
pub fn get_compute_prediction_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    logic::get_compute_prediction_result(db, id)
}

#[tauri::command]
pub fn predict_onnx_model(
    db: tauri::State<DbState>,
    request: PredictOnnxModelRequest,
) -> Result<BackgroundTaskResponse, String> {
    logic::predict_onnx_model(db, request)
}

#[tauri::command]
pub fn hash_onnx_model_file(path: String) -> Result<String, String> {
    logic::hash_onnx_model_file(&path)
}

#[tauri::command]
pub fn get_compute_onnx_prediction_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    logic::get_compute_onnx_prediction_result(db, id)
}
