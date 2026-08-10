use crate::steel::DatasetPreview;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetColumnMapping {
    pub ordinal: usize,
    pub canonical_field: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteelDatasetColumnRecord {
    pub ordinal: usize,
    pub original_name: String,
    pub duplicate: bool,
    pub inferred_type: String,
    pub canonical_field: Option<String>,
    pub unit: Option<String>,
    pub non_empty_count: usize,
    pub missing_count: usize,
    pub invalid_count: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteelDatasetRecord {
    pub id: String,
    pub source_name: String,
    pub source_path: String,
    pub source_sha256: String,
    pub format: String,
    pub selected_sheet: String,
    pub row_count: usize,
    pub column_count: usize,
    pub truncated: bool,
    pub mapping_state: String,
    pub preview: DatasetPreview,
    pub columns: Vec<SteelDatasetColumnRecord>,
    pub created_at: String,
    pub updated_at: String,
}
