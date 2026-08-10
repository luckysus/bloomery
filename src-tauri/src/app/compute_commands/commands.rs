use super::logic::{
    self, ExportLinearOnnxRequest, OptimizeSteelProcessRequest, PredictOnnxModelRequest,
    PredictSteelModelRequest, RegisterSteelModelRequest, TrainSteelDatasetRequest,
};
use crate::app::task_commands::tasks::BackgroundTaskResponse;
use crate::db::DbState;
use crate::storage::repositories::steel_models::SteelModelRecord;
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
pub fn export_linear_model_onnx(
    db: tauri::State<DbState>,
    request: ExportLinearOnnxRequest,
) -> Result<BackgroundTaskResponse, String> {
    logic::export_linear_model_onnx(db, request)
}

#[tauri::command]
pub fn get_compute_export_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    logic::get_compute_export_result(db, id)
}

#[tauri::command]
pub fn register_steel_model(
    db: tauri::State<DbState>,
    request: RegisterSteelModelRequest,
) -> Result<SteelModelRecord, String> {
    logic::register_steel_model(db, request)
}

#[tauri::command]
pub fn list_steel_models(
    db: tauri::State<DbState>,
    lineage_id: String,
) -> Result<Vec<SteelModelRecord>, String> {
    logic::list_steel_models(db, lineage_id)
}

#[tauri::command]
pub fn set_active_steel_model(
    db: tauri::State<DbState>,
    id: String,
) -> Result<SteelModelRecord, String> {
    logic::set_active_steel_model(db, id)
}

#[tauri::command]
pub fn delete_steel_model(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    logic::delete_steel_model(db, id)
}

#[tauri::command]
pub fn optimize_steel_process(
    db: tauri::State<DbState>,
    request: OptimizeSteelProcessRequest,
) -> Result<BackgroundTaskResponse, String> {
    logic::optimize_steel_process(db, request)
}

#[tauri::command]
pub fn get_compute_optimization_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    logic::get_compute_optimization_result(db, id)
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
