use super::logic::{
    self, AnalyzeSteelDatasetRequest, CarbonEquivalentRequest, SaveSteelDatasetRequest,
};
use crate::db::DbState;
use crate::steel::{
    CarbonEquivalentResult, DatasetAnalysis, DatasetPreview, DatasetPreviewRequest,
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
