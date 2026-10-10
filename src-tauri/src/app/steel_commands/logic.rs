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

/// 读取数据集原始表，并校验源文件自保存以来未被修改。
fn load_dataset_table(
    db: &tauri::State<'_, DbState>,
    dataset_id: &str,
) -> Result<(SteelDatasetRecord, crate::steel::DatasetTable), String> {
    with_conn(db, |connection| {
        let workspace_id = current_workspace_id();
        let dataset = repository::get(connection, workspace_id, dataset_id)?
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
        Ok((dataset, table))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteelDatasetSeriesRequest {
    pub dataset_id: String,
    pub columns: Vec<usize>,
    #[serde(default)]
    pub max_rows: Option<usize>,
}

/// 第 32 章：折线图与散点图需要的按行数值序列。
pub fn read_steel_dataset_series(
    db: tauri::State<DbState>,
    request: SteelDatasetSeriesRequest,
) -> Result<crate::steel::DatasetSeries, String> {
    let (_, table) = load_dataset_table(&db, &request.dataset_id)?;
    crate::steel::dataset_series(
        &table.headers,
        &table.rows,
        &crate::steel::DatasetSeriesRequest {
            columns: request.columns,
            max_rows: request.max_rows,
        },
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteelMultivariateRequest {
    pub dataset_id: String,
    pub columns: Vec<usize>,
    #[serde(default)]
    pub components: Option<usize>,
    #[serde(default)]
    pub clusters: Option<usize>,
}

/// 第 31/32 章：PCA 主成分分析与 k-means 聚类。
pub fn analyze_steel_dataset_multivariate(
    db: tauri::State<DbState>,
    request: SteelMultivariateRequest,
) -> Result<crate::steel::MultivariateResult, String> {
    let (_, table) = load_dataset_table(&db, &request.dataset_id)?;
    let rows = crate::steel::numeric_matrix(&table.rows);
    crate::steel::multivariate_analysis(
        &rows,
        &crate::steel::MultivariateRequest {
            columns: request.columns,
            components: request.components,
            clusters: request.clusters,
        },
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteelModelExplanationRequest {
    pub model_id: String,
    pub features: Vec<f64>,
}

/// 第 37 章：线性模型的精确 SHAP。非线性的模型族会被明确拒绝。
pub fn explain_steel_model(
    db: tauri::State<DbState>,
    request: SteelModelExplanationRequest,
) -> Result<crate::steel::ShapExplanation, String> {
    with_conn(&db, |connection| {
        let workspace_id = current_workspace_id();
        let record = crate::storage::repositories::steel_models::get(
            connection,
            workspace_id,
            &request.model_id,
        )?
        .ok_or_else(|| "steel model was not found in the local workspace".to_string())?;
        let artifact_json = record
            .artifact_json
            .ok_or_else(|| "steel model has no linear artifact to explain".to_string())?;
        let artifact: serde_json::Value = serde_json::from_str(&artifact_json)
            .map_err(|error| format!("model artifact is not valid JSON: {error}"))?;
        let model = crate::steel::parse_linear_artifact(&artifact)?;
        crate::steel::explain_linear(&model, &request.features)
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanSteelDatasetRequest {
    pub dataset_id: String,
    #[serde(default)]
    pub plan: crate::steel::DatasetCleaningPlan,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanedSteelDataset {
    pub dataset: SteelDatasetRecord,
    pub summary: crate::steel::DatasetCleaningSummary,
}

/// 第 31 章：按清洗计划处理重复行、缺失值与异常值，并把结果落盘为新数据集。
pub fn clean_steel_dataset(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    request: CleanSteelDatasetRequest,
) -> Result<CleanedSteelDataset, String> {
    let (dataset, table) = load_dataset_table(&db, &request.dataset_id)?;
    let (rows, summary) =
        crate::steel::clean_dataset(&table.headers, &table.rows, &request.plan)?;
    let directory = crate::db::app_data_directory(&app)?.join("datasets");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let stem = std::path::Path::new(&dataset.source_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("dataset");
    let path = directory.join(format!("{stem}-cleaned-{}.csv", uuid::Uuid::new_v4()));
    std::fs::write(&path, crate::steel::to_csv(&table.headers, &rows))
        .map_err(|error| error.to_string())?;
    let source_path = path.to_string_lossy().to_string();
    let preview = preview_dataset(&DatasetPreviewRequest {
        source_path: source_path.clone(),
        sheet: None,
    })?;
    let source_sha256 = hash_dataset_source(&source_path)?;
    let mappings = dataset
        .columns
        .iter()
        .map(|column| DatasetColumnMapping {
            ordinal: column.ordinal,
            canonical_field: column.canonical_field.clone(),
            unit: column.unit.clone(),
        })
        .collect::<Vec<_>>();
    let saved = with_conn_mut(&db, |connection| {
        repository::save_preview(
            connection,
            current_workspace_id(),
            &source_path,
            &source_sha256,
            &preview,
            &mappings,
        )
    })?;
    Ok(CleanedSteelDataset {
        dataset: saved,
        summary,
    })
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
