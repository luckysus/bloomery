use super::logic::{
    self, AnalyzeSteelDatasetRequest, CarbonEquivalentRequest, CleanSteelDatasetRequest,
    SaveSteelDatasetRequest, SteelDatasetSeriesRequest, SteelModelExplanationRequest,
    SteelMultivariateRequest,
};
use crate::db::DbState;
use crate::steel::{
    CarbonEquivalentResult, DatasetAnalysis, DatasetPreview, DatasetPreviewRequest, DatasetSeries,
    MultivariateResult, ShapExplanation,
};
use crate::storage::repositories::steel::SteelDatasetRecord;

#[tauri::command]
pub fn calculate_steel_carbon_equivalent(
    request: CarbonEquivalentRequest,
) -> Result<CarbonEquivalentResult, String> {
    logic::calculate_steel_carbon_equivalent(request)
}

#[tauri::command]
pub fn preview_steel_dataset(request: DatasetPreviewRequest) -> Result<DatasetPreview, String> {
    logic::preview_steel_dataset(request)
}

#[tauri::command]
pub fn list_steel_datasets(db: tauri::State<DbState>) -> Result<Vec<SteelDatasetRecord>, String> {
    logic::list_steel_datasets(db)
}

#[tauri::command]
pub fn save_steel_dataset(
    db: tauri::State<DbState>,
    request: SaveSteelDatasetRequest,
) -> Result<SteelDatasetRecord, String> {
    logic::save_steel_dataset(db, request)
}

#[tauri::command]
pub fn activate_steel_dataset(
    db: tauri::State<DbState>,
    dataset_id: String,
) -> Result<SteelDatasetRecord, String> {
    logic::activate_steel_dataset(db, dataset_id)
}

#[tauri::command]
pub fn analyze_steel_dataset(
    db: tauri::State<DbState>,
    request: AnalyzeSteelDatasetRequest,
) -> Result<DatasetAnalysis, String> {
    logic::analyze_steel_dataset(db, request)
}

#[tauri::command]
pub fn read_steel_dataset_series(
    db: tauri::State<DbState>,
    request: SteelDatasetSeriesRequest,
) -> Result<DatasetSeries, String> {
    logic::read_steel_dataset_series(db, request)
}

#[tauri::command]
pub fn analyze_steel_dataset_multivariate(
    db: tauri::State<DbState>,
    request: SteelMultivariateRequest,
) -> Result<MultivariateResult, String> {
    logic::analyze_steel_dataset_multivariate(db, request)
}

#[tauri::command]
pub fn explain_steel_model(
    db: tauri::State<DbState>,
    request: SteelModelExplanationRequest,
) -> Result<ShapExplanation, String> {
    logic::explain_steel_model(db, request)
}

#[tauri::command]
pub fn clean_steel_dataset(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    request: CleanSteelDatasetRequest,
) -> Result<logic::CleanedSteelDataset, String> {
    logic::clean_steel_dataset(app, db, request)
}
