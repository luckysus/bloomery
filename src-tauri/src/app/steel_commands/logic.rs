use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::steel::{
    analyze_dataset, calculate_carbon_equivalent, hash_dataset_source, preview_dataset,
    read_dataset_table, CarbonEquivalentFormula, CarbonEquivalentResult, CompositionInput,
    CompositionUnit, DatasetAnalysis, DatasetAnalysisRequest, DatasetPreview,
    DatasetPreviewRequest,
};
use crate::storage::repositories::steel::{
    self as repository, DatasetColumnMapping, SteelDatasetRecord,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarbonEquivalentRequest {
    pub formula: CarbonEquivalentFormula,
    pub unit: CompositionUnit,
    pub composition: BTreeMap<String, f64>,
}

pub fn calculate_steel_carbon_equivalent(
    request: CarbonEquivalentRequest,
) -> Result<CarbonEquivalentResult, String> {
    calculate_carbon_equivalent(
        &CompositionInput {
            values: request.composition,
            unit: request.unit,
        },
        request.formula,
    )
    .map_err(|error| error.to_string())
}

pub fn preview_steel_dataset(request: DatasetPreviewRequest) -> Result<DatasetPreview, String> {
    preview_dataset(&request)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSteelDatasetRequest {
    pub source_path: String,
    #[serde(default)]
    pub sheet: Option<String>,
    #[serde(default)]
    pub mappings: Vec<DatasetColumnMapping>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeSteelDatasetRequest {
    pub dataset_id: String,
    #[serde(default)]
    pub selected_columns: Vec<usize>,
    #[serde(default)]
    pub outlier_iqr_multiplier: Option<f64>,
    #[serde(default)]
    pub group_by_column: Option<usize>,
    #[serde(default)]
    pub correlation_columns: Vec<usize>,
}

pub fn list_steel_datasets(db: tauri::State<DbState>) -> Result<Vec<SteelDatasetRecord>, String> {
    with_conn(&db, |connection| {
        repository::list(connection, current_workspace_id())
    })
}

pub fn save_steel_dataset(
    db: tauri::State<DbState>,
    request: SaveSteelDatasetRequest,
) -> Result<SteelDatasetRecord, String> {
    let preview = preview_dataset(&DatasetPreviewRequest {
        source_path: request.source_path.clone(),
        sheet: request.sheet.clone(),
    })?;
    let source_sha256 = hash_dataset_source(&request.source_path)?;
    with_conn_mut(&db, |connection| {
        repository::save_preview(
            connection,
            current_workspace_id(),
            &request.source_path,
            &source_sha256,
            &preview,
            &request.mappings,
        )
    })
}

pub fn activate_steel_dataset(
    db: tauri::State<DbState>,
    dataset_id: String,
) -> Result<SteelDatasetRecord, String> {
    with_conn_mut(&db, |connection| {
        let workspace_id = current_workspace_id();
        let dataset = repository::get(connection, workspace_id, &dataset_id)?
            .ok_or_else(|| "steel dataset was not found in the local workspace".to_string())?;
        let current_hash = hash_dataset_source(&dataset.source_path)?;
        if current_hash != dataset.source_sha256 {
            return Err(
                "steel dataset source changed since it was saved; preview and save it again"
                    .to_string(),
            );
        }
        repository::activate(connection, workspace_id, &dataset_id)?
            .ok_or_else(|| "steel dataset was removed before activation".to_string())
    })
}

pub fn analyze_steel_dataset(
    db: tauri::State<DbState>,
    request: AnalyzeSteelDatasetRequest,
) -> Result<DatasetAnalysis, String> {
    with_conn(&db, |connection| {
        let workspace_id = current_workspace_id();
        let dataset = repository::get(connection, workspace_id, &request.dataset_id)?
            .ok_or_else(|| "steel dataset was not found in the local workspace".to_string())?;
        let source_sha256 = hash_dataset_source(&dataset.source_path)?;
        if source_sha256 != dataset.source_sha256 {
            return Err(
                "steel dataset source changed since it was saved; preview and save it again"
                    .to_string(),
            );
        }
        let table = read_dataset_table(&DatasetPreviewRequest {
            source_path: dataset.source_path.clone(),
            sheet: Some(dataset.selected_sheet.clone()),
        })?;
        let mut analysis = analyze_dataset(
            table.headers,
            table.rows,
            DatasetAnalysisRequest {
                selected_columns: request.selected_columns,
                outlier_iqr_multiplier: request.outlier_iqr_multiplier,
                group_by_column: request.group_by_column,
                correlation_columns: request.correlation_columns,
            },
        )?;
        reconcile_bounded_analysis(&mut analysis, table.row_count);
        analysis.dataset_id = Some(dataset.id);
        analysis.source_sha256 = Some(dataset.source_sha256);
        analysis.selected_sheet = Some(dataset.selected_sheet);
        for column in &mut analysis.columns {
            if let Some(saved) = dataset
                .columns
                .iter()
                .find(|saved| saved.ordinal == column.ordinal)
            {
                column.canonical_field = saved.canonical_field.clone();
                column.unit = saved.unit.clone();
            }
        }
        Ok(analysis)
    })
}

fn reconcile_bounded_analysis(analysis: &mut DatasetAnalysis, source_row_count: usize) {
    if source_row_count <= analysis.row_count {
        return;
    }
    analysis.row_count = source_row_count;
    analysis.excluded_row_count = source_row_count.saturating_sub(analysis.analyzed_row_count);
    analysis.warnings.push(format!(
        "analysis is bounded to the first {} retained data rows; {} source rows were excluded",
        analysis.analyzed_row_count, analysis.excluded_row_count
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_returns_a_versioned_carbon_equivalent() {
        let result = calculate_steel_carbon_equivalent(CarbonEquivalentRequest {
            formula: CarbonEquivalentFormula::Iiw,
            unit: CompositionUnit::PercentMass,
            composition: BTreeMap::from([
                ("C".to_string(), 0.2),
                ("Mn".to_string(), 1.0),
                ("Cr".to_string(), 0.25),
                ("Mo".to_string(), 0.05),
                ("V".to_string(), 0.02),
                ("Ni".to_string(), 0.2),
                ("Cu".to_string(), 0.3),
            ]),
        })
        .expect("carbon equivalent command");

        assert_eq!(result.formula_id, "carbon-equivalent.iiw.v1");
        assert!((result.value - 0.464).abs() < 1e-9);
    }

    #[test]
    fn bounded_analysis_reports_source_rows_and_exclusions() {
        let mut analysis = analyze_dataset(
            vec!["yield_strength".to_string()],
            vec![vec!["355".to_string()], vec!["360".to_string()]],
            DatasetAnalysisRequest::default(),
        )
        .expect("analyze retained rows");

        reconcile_bounded_analysis(&mut analysis, 100_001);

        assert_eq!(analysis.row_count, 100_001);
        assert_eq!(analysis.analyzed_row_count, 2);
        assert_eq!(analysis.excluded_row_count, 99_999);
        assert!(analysis
            .warnings
            .iter()
            .any(|warning| warning.contains("bounded")));
    }
}
