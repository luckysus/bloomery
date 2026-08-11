use crate::app::task_commands::tasks::{background_task_response, BackgroundTaskResponse};
use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::steel::{hash_dataset_source, read_dataset_table, DatasetTable};
use crate::storage::repositories::steel::{self as steel_repository, SteelDatasetColumnRecord};
use crate::storage::repositories::steel_models::{
    self as model_repository, NewSteelModel, SteelModelRecord,
};
use crate::tasks::{repository as task_repository, NewTask};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainSteelDatasetRequest {
    pub dataset_id: String,
    pub target_column: usize,
    pub feature_columns: Vec<usize>,
    #[serde(default)]
    pub split_policy: Option<Value>,
    #[serde(default)]
    pub algorithm: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictSteelModelRequest {
    pub dataset_id: String,
    pub training_task_id: String,
    pub feature_values: Vec<f64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictOnnxModelRequest {
    pub model_path: String,
    pub model_sha256: String,
    pub manifest: Value,
    pub features: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeSteelProcessRequest {
    pub dataset_id: String,
    pub training_task_id: String,
    pub direction: String,
    pub objective_columns: Vec<usize>,
    pub bounds: Vec<Value>,
    #[serde(default)]
    pub fixed_values: Vec<Option<f64>>,
    #[serde(default)]
    pub constraints: Vec<Value>,
    pub trials: u32,
    #[serde(default)]
    pub seed: i64,
}

pub const MAX_OPTIMIZATION_TRIALS: u32 = 500;

fn requested_training_algorithm(request: &TrainSteelDatasetRequest) -> Result<&str, String> {
    let algorithm = request
        .algorithm
        .as_deref()
        .unwrap_or("linear_regression")
        .trim();
    match algorithm {
        "linear_regression" | "elasticnet" | "random_forest" | "hist_gradient_boosting" => {
            Ok(algorithm)
        }
        _ => Err(format!("unsupported training algorithm: {algorithm}")),
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportLinearOnnxRequest {
    pub dataset_id: String,
    pub training_task_id: String,
    #[serde(default)]
    pub model_version: Option<String>,
}

pub fn build_linear_regression_payload(
    table: &DatasetTable,
    columns: &[SteelDatasetColumnRecord],
    request: &TrainSteelDatasetRequest,
) -> Result<Value, String> {
    if request.dataset_id.trim().is_empty() {
        return Err("dataset ID is required".to_string());
    }
    if table.headers.len() != columns.len() {
        return Err("dataset column metadata does not match the source table".to_string());
    }
    if request.feature_columns.is_empty() {
        return Err("at least one feature column is required".to_string());
    }
    if request.target_column >= table.headers.len() {
        return Err("target column is out of range".to_string());
    }
    let mut selected = HashSet::new();
    for ordinal in &request.feature_columns {
        if *ordinal >= table.headers.len() {
            return Err(format!("feature column {ordinal} is out of range"));
        }
        if *ordinal == request.target_column {
            return Err("target column cannot also be a feature".to_string());
        }
        if !selected.insert(*ordinal) {
            return Err(format!("feature column {ordinal} is duplicated"));
        }
        if columns[*ordinal].duplicate {
            return Err(format!(
                "feature column {ordinal} is duplicated in the dataset"
            ));
        }
    }
    if columns[request.target_column].duplicate {
        return Err("target column is duplicated in the dataset".to_string());
    }

    let feature_names = request
        .feature_columns
        .iter()
        .map(|ordinal| {
            columns[*ordinal]
                .canonical_field
                .clone()
                .filter(|field| !field.trim().is_empty())
                .unwrap_or_else(|| table.headers[*ordinal].clone())
        })
        .collect::<Vec<_>>();
    let field_mapping = request
        .feature_columns
        .iter()
        .zip(feature_names.iter())
        .map(|(ordinal, name)| {
            (
                name.clone(),
                columns[*ordinal]
                    .canonical_field
                    .clone()
                    .map(Value::String)
                    .unwrap_or_else(|| Value::String(table.headers[*ordinal].clone())),
            )
        })
        .collect::<serde_json::Map<_, _>>();

    let mut features = Vec::new();
    let mut targets = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let target_raw = row
            .get(request.target_column)
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        if target_raw.is_empty() {
            continue;
        }
        let target = parse_finite(target_raw).ok_or_else(|| {
            format!(
                "target column {} has an invalid number at source row {}",
                request.target_column,
                row_index + 2
            )
        })?;
        let mut feature_row = Vec::with_capacity(request.feature_columns.len());
        for ordinal in &request.feature_columns {
            let raw = row.get(*ordinal).map(String::as_str).unwrap_or("").trim();
            if raw.is_empty() {
                feature_row.push(Value::Null);
            } else {
                let value = parse_finite(raw).ok_or_else(|| {
                    format!(
                        "feature column {ordinal} has an invalid number at source row {}",
                        row_index + 2
                    )
                })?;
                feature_row.push(json!(value));
            }
        }
        features.push(Value::Array(feature_row));
        targets.push(json!(target));
    }
    if features.len() < 2 {
        return Err("training requires at least two rows with a valid target".to_string());
    }

    Ok(json!({
        "dataset_id": request.dataset_id,
        "features": features,
        "targets": targets,
        "feature_names": feature_names,
        "field_mapping": field_mapping,
        "split_policy": request.split_policy.clone().unwrap_or_else(|| json!({
            "kind": "random",
            "validation_fraction": 0.2,
            "seed": 0,
        })),
    }))
}

fn parse_finite(value: &str) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

pub fn build_linear_regression_prediction_payload(
    dataset_id: &str,
    training_task_id: &str,
    artifact: &Value,
    feature_values: &[f64],
) -> Result<Value, String> {
    if dataset_id.trim().is_empty() {
        return Err("dataset ID is required".to_string());
    }
    if training_task_id.trim().is_empty() {
        return Err("training task ID is required".to_string());
    }
    if artifact["artifact_version"] != "linear-regression.v1"
        && artifact["artifact_version"] != "sklearn-pickle.v1"
    {
        return Err("unsupported training model artifact".to_string());
    }
    let expected_count = artifact["feature_schema"]["count"]
        .as_u64()
        .or_else(|| {
            artifact["feature_names"]
                .as_array()
                .map(Vec::len)
                .map(|value| value as u64)
        })
        .ok_or_else(|| "training model artifact has no feature schema".to_string())?;
    if feature_values.len() != expected_count as usize {
        return Err("prediction feature count does not match the model".to_string());
    }
    if feature_values.iter().any(|value| !value.is_finite()) {
        return Err("prediction features must be finite numbers".to_string());
    }
    Ok(json!({
        "operation": "predict_linear_regression",
        "payload": {
            "dataset_id": dataset_id,
            "training_task_id": training_task_id,
            "artifact": artifact,
            "features": [feature_values],
        }
    }))
}

pub fn build_onnx_prediction_payload(request: &PredictOnnxModelRequest) -> Result<Value, String> {
    let model_path = request.model_path.trim();
    if model_path.is_empty() {
        return Err("ONNX model path is required".to_string());
    }
    let path = std::path::Path::new(model_path);
    validate_onnx_model_path(path)?;
    if request.model_sha256.len() != 64
        || !request
            .model_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("ONNX model sha256 is invalid".to_string());
    }
    let actual_hash = hash_onnx_model(path)?;
    let expected_hash = request.model_sha256.to_ascii_lowercase();
    if actual_hash != expected_hash {
        return Err("ONNX model hash does not match the selected file".to_string());
    }
    if !request.manifest.is_object() {
        return Err("ONNX manifest must be an object".to_string());
    }
    for key in [
        "model_id",
        "model_version",
        "inputs",
        "outputs",
        "preprocessing",
    ] {
        if request.manifest.get(key).is_none() {
            return Err(format!("ONNX manifest is missing {key}"));
        }
    }
    validate_onnx_features(&request.features)?;
    Ok(json!({
        "operation": "predict_onnx",
        "payload": {
            "model_path": model_path,
            "model_sha256": actual_hash,
            "manifest": request.manifest,
            "features": request.features,
        }
    }))
}

pub fn hash_onnx_model_file(path_text: &str) -> Result<String, String> {
    let path_text = path_text.trim();
    if path_text.is_empty() {
        return Err("ONNX model path is required".to_string());
    }
    let path = std::path::Path::new(path_text);
    validate_onnx_model_path(path)?;
    hash_onnx_model(path)
}

fn validate_onnx_model_path(path: &std::path::Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("ONNX model file was not found".to_string());
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("onnx"))
    {
        return Err("ONNX model path must use the .onnx extension".to_string());
    }
    Ok(())
}

fn hash_onnx_model(path: &std::path::Path) -> Result<String, String> {
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("read ONNX model: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("read ONNX model: {error}"))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn validate_onnx_features(features: &[Vec<f64>]) -> Result<(), String> {
    if features.is_empty() {
        return Err("ONNX features must be a non-empty matrix".to_string());
    }
    if features.len() > 100_000 {
        return Err("ONNX features exceed the row limit".to_string());
    }
    let width = features[0].len();
    if width == 0 {
        return Err("ONNX features must contain non-empty rows".to_string());
    }
    if width > 128 {
        return Err("ONNX features exceed the column limit".to_string());
    }
    for row in features {
        if row.len() != width {
            return Err("ONNX features must have a consistent column count".to_string());
        }
        if row.iter().any(|value| !value.is_finite()) {
            return Err("ONNX features must be finite numbers".to_string());
        }
    }
    Ok(())
}

pub fn build_optimization_payload(
    dataset_id: &str,
    training_task_id: &str,
    artifact: &Value,
    request: &OptimizeSteelProcessRequest,
) -> Result<Value, String> {
    if dataset_id.trim().is_empty() {
        return Err("dataset ID is required".to_string());
    }
    if training_task_id.trim().is_empty() {
        return Err("training task ID is required".to_string());
    }
    if artifact["artifact_version"] != "linear-regression.v1"
        || artifact["model_type"] != "linear_regression"
    {
        return Err("unsupported training model artifact".to_string());
    }
    if request.direction != "minimize" && request.direction != "maximize" {
        return Err("direction must be minimize or maximize".to_string());
    }
    let feature_names = artifact["feature_names"]
        .as_array()
        .ok_or_else(|| "training model artifact has no feature names".to_string())?;
    let feature_count = feature_names.len();
    if feature_count == 0 {
        return Err("training model artifact has no features".to_string());
    }
    if request.objective_columns.is_empty() || request.objective_columns.len() > 4 {
        return Err("optimization requires between 1 and 4 objectives".to_string());
    }
    let mut objective_names = Vec::new();
    for ordinal in &request.objective_columns {
        if *ordinal >= feature_count {
            return Err(format!("objective column {ordinal} is out of range"));
        }
        let name = feature_names[*ordinal]
            .as_str()
            .ok_or_else(|| "feature name is invalid".to_string())?;
        if objective_names
            .iter()
            .any(|existing: &String| existing == name)
        {
            return Err(format!("objective {name} is duplicated"));
        }
        objective_names.push(name.to_string());
    }
    if request.bounds.len() != feature_count {
        return Err("bounds must cover every model feature".to_string());
    }
    let mut bounds = Vec::new();
    for (index, entry) in request.bounds.iter().enumerate() {
        let minimum = entry
            .get("min")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("bounds[{index}].min must be a finite number"))?;
        let maximum = entry
            .get("max")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("bounds[{index}].max must be a finite number"))?;
        if minimum > maximum {
            return Err(format!("bounds[{index}] is inverted"));
        }
        bounds.push(json!({"min": minimum, "max": maximum}));
    }
    if request.fixed_values.len() != feature_count {
        return Err("fixed_values must cover every model feature".to_string());
    }
    let mut fixed_values = serde_json::Map::new();
    for (index, value) in request.fixed_values.iter().enumerate() {
        let Some(value) = value else { continue };
        if !value.is_finite() {
            return Err(format!("fixed_values[{index}] must be finite"));
        }
        let name = feature_names[index]
            .as_str()
            .ok_or_else(|| "feature name is invalid".to_string())?;
        fixed_values.insert(name.to_string(), json!(value));
    }
    let mut constraints = Vec::new();
    for (index, entry) in request.constraints.iter().enumerate() {
        let kind = entry
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("constraints[{index}].kind is required"))?;
        if kind != "equality" && kind != "inequality" {
            return Err(format!(
                "constraints[{index}].kind must be equality or inequality"
            ));
        }
        let coefficients = entry
            .get("coefficients")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("constraints[{index}].coefficients must be an array"))?;
        if coefficients.len() != feature_count {
            return Err(format!(
                "constraints[{index}].coefficients must cover every model feature"
            ));
        }
        let mut named = serde_json::Map::new();
        let mut non_zero = false;
        for (column, coefficient) in coefficients.iter().enumerate() {
            let value = coefficient
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    format!("constraints[{index}].coefficients[{column}] must be finite")
                })?;
            if value != 0.0 {
                non_zero = true;
            }
            let name = feature_names[column]
                .as_str()
                .ok_or_else(|| "feature name is invalid".to_string())?;
            named.insert(name.to_string(), json!(value));
        }
        if !non_zero {
            return Err(format!("constraints[{index}] has no coefficients"));
        }
        let target = entry
            .get("value")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("constraints[{index}].value must be a finite number"))?;
        let mut constraint = json!({
            "kind": kind,
            "coefficients": named,
            "value": target,
        });
        if let Some(tolerance) = entry.get("tolerance") {
            let tolerance = tolerance
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| format!("constraints[{index}].tolerance must be non-negative"))?;
            constraint["tolerance"] = json!(tolerance);
        }
        constraints.push(constraint);
    }
    if request.trials < 1 || request.trials > MAX_OPTIMIZATION_TRIALS {
        return Err(format!(
            "trials must be between 1 and {MAX_OPTIMIZATION_TRIALS}"
        ));
    }
    Ok(json!({
        "operation": "optimize_constrained",
        "payload": {
            "dataset_id": dataset_id,
            "training_task_id": training_task_id,
            "artifact": artifact,
            "direction": request.direction,
            "objectives": objective_names,
            "bounds": bounds,
            "fixed_values": fixed_values,
            "constraints": constraints,
            "trials": request.trials,
            "seed": request.seed,
        }
    }))
}

pub(crate) fn train_steel_dataset(
    db: tauri::State<DbState>,
    request: TrainSteelDatasetRequest,
) -> Result<BackgroundTaskResponse, String> {
    let algorithm = requested_training_algorithm(&request)?;
    let prepared = with_conn(&db, |connection| {
        let workspace_id = current_workspace_id();
        let dataset = steel_repository::get(connection, workspace_id, &request.dataset_id)?
            .ok_or_else(|| "steel dataset was not found in the local workspace".to_string())?;
        if dataset.mapping_state != "ready" {
            return Err("steel dataset must be activated before training".to_string());
        }
        let current_hash = hash_dataset_source(&dataset.source_path)?;
        if current_hash != dataset.source_sha256 {
            return Err(
                "steel dataset source changed since it was saved; preview and save it again"
                    .to_string(),
            );
        }
        let table = read_dataset_table(&crate::steel::DatasetPreviewRequest {
            source_path: dataset.source_path.clone(),
            sheet: Some(dataset.selected_sheet.clone()),
        })?;
        let payload = build_linear_regression_payload(&table, &dataset.columns, &request)?;
        Ok((workspace_id.to_string(), dataset.source_sha256, payload))
    })?;

    let (workspace_id, source_sha256, payload) = prepared;
    with_conn_mut(&db, |connection| {
        let operation = if algorithm == "linear_regression" {
            "train_linear_regression"
        } else {
            "train_sklearn_model"
        };
        let task_payload = json!({
            "operation": operation,
            "payload": {
                "dataset_id": payload["dataset_id"],
                "data_version": source_sha256,
                "features": payload["features"],
                "targets": payload["targets"],
                "feature_names": payload["feature_names"],
                "field_mapping": payload["field_mapping"],
                "split_policy": payload["split_policy"],
                "algorithm": algorithm,
            }
        });
        let task_kind = if algorithm == "linear_regression" {
            crate::compute::handler::COMPUTE_TRAIN_LINEAR_REGRESSION_KIND
        } else {
            crate::compute::handler::COMPUTE_TRAIN_SKLEARN_KIND
        };
        task_repository::create(
            connection,
            NewTask {
                workspace_id,
                kind: task_kind.to_string(),
                payload_json: serde_json::to_string(&task_payload)
                    .map_err(|error| error.to_string())?,
                checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
                next_run_at: None,
                progress: 0,
            },
        )
        .map(background_task_response)
        .map_err(|error| error.to_string())
    })
}

pub(crate) fn get_compute_training_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn(&db, |connection| {
        let task = task_repository::get(connection, current_workspace_id(), id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task_not_found: task not found".to_string())?;
        if !crate::compute::handler::is_training_task_kind(&task.kind) {
            return Err("task is not a model training task".to_string());
        }
        if task.state != crate::tasks::TaskState::Completed {
            return Ok(None);
        }
        let checkpoint: Value = serde_json::from_str(
            task.checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed compute task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid compute checkpoint: {error}"))?;
        Ok(checkpoint.get("result").cloned())
    })
}

pub(crate) fn predict_steel_model(
    db: tauri::State<DbState>,
    request: PredictSteelModelRequest,
) -> Result<BackgroundTaskResponse, String> {
    let training_task_id = uuid::Uuid::parse_str(&request.training_task_id)
        .map_err(|error| format!("invalid training task ID: {error}"))?;
    with_conn_mut(&db, |connection| {
        let workspace_id = current_workspace_id();
        let training_task = task_repository::get(connection, workspace_id, training_task_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "training task was not found in the local workspace".to_string())?;
        if !crate::compute::handler::is_training_task_kind(&training_task.kind) {
            return Err("task is not a model training task".to_string());
        }
        if training_task.state != crate::tasks::TaskState::Completed {
            return Err("training task must be completed before prediction".to_string());
        }
        let training_payload: Value = serde_json::from_str(&training_task.payload_json)
            .map_err(|error| format!("invalid training task payload: {error}"))?;
        let source_dataset_id = training_payload["payload"]["dataset_id"]
            .as_str()
            .ok_or_else(|| "training task has no dataset identity".to_string())?;
        if source_dataset_id != request.dataset_id {
            return Err("prediction dataset does not match the training task".to_string());
        }
        let checkpoint: Value = serde_json::from_str(
            training_task
                .checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed training task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid training checkpoint: {error}"))?;
        let artifact = checkpoint["result"]["artifact"].clone();
        let mut payload = build_linear_regression_prediction_payload(
            &request.dataset_id,
            &request.training_task_id,
            &artifact,
            &request.feature_values,
        )?;
        let task_kind = match artifact["artifact_version"].as_str() {
            Some("linear-regression.v1") => {
                crate::compute::handler::COMPUTE_PREDICT_LINEAR_REGRESSION_KIND
            }
            Some("sklearn-pickle.v1") => {
                payload["operation"] = json!("predict_trained_model");
                crate::compute::handler::COMPUTE_PREDICT_TRAINED_KIND
            }
            _ => return Err("unsupported training model artifact".to_string()),
        };
        let task_payload = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
        task_repository::create(
            connection,
            NewTask {
                workspace_id: workspace_id.to_string(),
                kind: task_kind.to_string(),
                payload_json: task_payload,
                checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
                next_run_at: None,
                progress: 0,
            },
        )
        .map(background_task_response)
        .map_err(|error| error.to_string())
    })
}

pub(crate) fn get_compute_prediction_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn(&db, |connection| {
        let task = task_repository::get(connection, current_workspace_id(), id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task_not_found: task not found".to_string())?;
        if !crate::compute::handler::is_prediction_task_kind(&task.kind) {
            return Err("task is not a model prediction task".to_string());
        }
        if task.state != crate::tasks::TaskState::Completed {
            return Ok(None);
        }
        let checkpoint: Value = serde_json::from_str(
            task.checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed prediction task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid prediction checkpoint: {error}"))?;
        Ok(checkpoint.get("result").cloned())
    })
}

pub fn build_export_payload(
    dataset_id: &str,
    training_task_id: &str,
    artifact: &Value,
    model_version: Option<&str>,
) -> Result<Value, String> {
    if dataset_id.trim().is_empty() {
        return Err("dataset ID is required".to_string());
    }
    if training_task_id.trim().is_empty() {
        return Err("training task ID is required".to_string());
    }
    if artifact["artifact_version"] != "linear-regression.v1"
        || artifact["model_type"] != "linear_regression"
    {
        return Err("unsupported training model artifact".to_string());
    }
    Ok(json!({
        "operation": "export_linear_onnx",
        "payload": {
            "dataset_id": dataset_id,
            "training_task_id": training_task_id,
            "artifact": artifact,
            "model_version": model_version.unwrap_or("1.0.0"),
        }
    }))
}

pub(crate) fn export_linear_model_onnx(
    db: tauri::State<DbState>,
    request: ExportLinearOnnxRequest,
) -> Result<BackgroundTaskResponse, String> {
    let training_task_id = uuid::Uuid::parse_str(&request.training_task_id)
        .map_err(|error| format!("invalid training task ID: {error}"))?;
    with_conn_mut(&db, |connection| {
        let workspace_id = current_workspace_id();
        let training_task = task_repository::get(connection, workspace_id, training_task_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "training task was not found in the local workspace".to_string())?;
        if training_task.kind != crate::compute::handler::COMPUTE_TRAIN_LINEAR_REGRESSION_KIND {
            return Err("task is not a linear regression training task".to_string());
        }
        if training_task.state != crate::tasks::TaskState::Completed {
            return Err("training task must be completed before export".to_string());
        }
        let training_payload: Value = serde_json::from_str(&training_task.payload_json)
            .map_err(|error| format!("invalid training task payload: {error}"))?;
        let source_dataset_id = training_payload["payload"]["dataset_id"]
            .as_str()
            .ok_or_else(|| "training task has no dataset identity".to_string())?;
        if source_dataset_id != request.dataset_id {
            return Err("export dataset does not match the training task".to_string());
        }
        let checkpoint: Value = serde_json::from_str(
            training_task
                .checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed training task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid training checkpoint: {error}"))?;
        let artifact = checkpoint["result"]["artifact"].clone();
        let payload = build_export_payload(
            &request.dataset_id,
            &request.training_task_id,
            &artifact,
            request.model_version.as_deref(),
        )?;
        let task_payload = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
        task_repository::create(
            connection,
            NewTask {
                workspace_id: workspace_id.to_string(),
                kind: crate::compute::handler::COMPUTE_EXPORT_ONNX_KIND.to_string(),
                payload_json: task_payload,
                checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
                next_run_at: None,
                progress: 0,
            },
        )
        .map(background_task_response)
        .map_err(|error| error.to_string())
    })
}

pub(crate) fn get_compute_export_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn(&db, |connection| {
        let task = task_repository::get(connection, current_workspace_id(), id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task_not_found: task not found".to_string())?;
        if task.kind != crate::compute::handler::COMPUTE_EXPORT_ONNX_KIND {
            return Err("task is not an ONNX export task".to_string());
        }
        if task.state != crate::tasks::TaskState::Completed {
            return Ok(None);
        }
        let checkpoint: Value = serde_json::from_str(
            task.checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed export task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid export checkpoint: {error}"))?;
        Ok(checkpoint.get("result").cloned())
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSteelModelRequest {
    pub task_id: String,
    #[serde(default)]
    pub lineage_id: Option<String>,
}

pub(crate) fn register_steel_model(
    db: tauri::State<DbState>,
    request: RegisterSteelModelRequest,
) -> Result<SteelModelRecord, String> {
    let task_id = uuid::Uuid::parse_str(&request.task_id)
        .map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn_mut(&db, |connection| {
        let task = task_repository::get(connection, current_workspace_id(), task_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task_not_found: task not found".to_string())?;
        if task.state != crate::tasks::TaskState::Completed {
            return Err("source task must be completed before registration".to_string());
        }
        let checkpoint: Value = serde_json::from_str(
            task.checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid task checkpoint: {error}"))?;
        let result = &checkpoint["result"];
        let payload: Value = serde_json::from_str(&task.payload_json)
            .map_err(|error| format!("invalid task payload: {error}"))?;

        let (kind, lineage, sha256, manifest_json, artifact_json, model_base64) =
            if crate::compute::handler::is_training_task_kind(&task.kind) {
                let artifact = &result["artifact"];
                let (kind, lineage_prefix) = match artifact["artifact_version"].as_str() {
                    Some("linear-regression.v1") => ("linear_artifact", "linear"),
                    Some("sklearn-pickle.v1") => ("sklearn_artifact", "sklearn"),
                    _ => return Err("unsupported training model artifact".to_string()),
                };
                let artifact_json =
                    serde_json::to_string(artifact).map_err(|error| error.to_string())?;
                let mut digest = Sha256::new();
                digest.update(artifact_json.as_bytes());
                let dataset_id = payload["payload"]["dataset_id"]
                    .as_str()
                    .unwrap_or("unknown");
                (
                    kind,
                    format!("{lineage_prefix}:{dataset_id}"),
                    format!("{:x}", digest.finalize()),
                    artifact_json.clone(),
                    Some(artifact_json),
                    None,
                )
            } else if task.kind == crate::compute::handler::COMPUTE_EXPORT_ONNX_KIND {
                let base64 = result["model_base64"]
                    .as_str()
                    .ok_or_else(|| "export result has no model blob".to_string())?;
                let sha256 = result["model_sha256"]
                    .as_str()
                    .ok_or_else(|| "export result has no model hash".to_string())?;
                let manifest_json = serde_json::to_string(&result["manifest"])
                    .map_err(|error| error.to_string())?;
                let training_task_id = payload["payload"]["training_task_id"]
                    .as_str()
                    .unwrap_or("unknown");
                (
                    "onnx",
                    format!("onnx:{training_task_id}"),
                    sha256.to_string(),
                    manifest_json,
                    None,
                    Some(base64.to_string()),
                )
            } else {
                return Err("task is not a registrable model source".to_string());
            };
        let lineage = request.lineage_id.clone().unwrap_or(lineage);
        model_repository::create(
            connection,
            current_workspace_id(),
            NewSteelModel {
                lineage_id: &lineage,
                kind,
                source_task_id: Some(&task.id.to_string()),
                model_sha256: &sha256,
                manifest_json: &manifest_json,
                artifact_json: artifact_json.as_deref(),
                model_base64: model_base64.as_deref(),
            },
        )
    })
}

pub(crate) fn list_steel_models(
    db: tauri::State<DbState>,
    lineage_id: String,
) -> Result<Vec<SteelModelRecord>, String> {
    with_conn(&db, |connection| {
        model_repository::list(connection, current_workspace_id(), &lineage_id)
    })
}

pub(crate) fn set_active_steel_model(
    db: tauri::State<DbState>,
    id: String,
) -> Result<SteelModelRecord, String> {
    with_conn_mut(&db, |connection| {
        model_repository::set_active(connection, current_workspace_id(), &id)
    })
}

pub(crate) fn delete_steel_model(db: tauri::State<DbState>, id: String) -> Result<(), String> {
    with_conn_mut(&db, |connection| {
        model_repository::delete(connection, current_workspace_id(), &id)
    })
}

pub(crate) fn optimize_steel_process(
    db: tauri::State<DbState>,
    request: OptimizeSteelProcessRequest,
) -> Result<BackgroundTaskResponse, String> {
    let training_task_id = uuid::Uuid::parse_str(&request.training_task_id)
        .map_err(|error| format!("invalid training task ID: {error}"))?;
    with_conn_mut(&db, |connection| {
        submit_optimization_on_connection(connection, &request, training_task_id)
            .map(background_task_response)
    })
}

pub fn submit_optimization_on_connection(
    connection: &mut rusqlite::Connection,
    request: &OptimizeSteelProcessRequest,
    training_task_id: uuid::Uuid,
) -> Result<crate::tasks::model::TaskRecord, String> {
    let workspace_id = current_workspace_id();
    let training_task = task_repository::get(connection, workspace_id, training_task_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "training task was not found in the local workspace".to_string())?;
    if training_task.kind != crate::compute::handler::COMPUTE_TRAIN_LINEAR_REGRESSION_KIND {
        return Err("task is not a linear regression training task".to_string());
    }
    if training_task.state != crate::tasks::TaskState::Completed {
        return Err("training task must be completed before optimization".to_string());
    }
    let training_payload: Value = serde_json::from_str(&training_task.payload_json)
        .map_err(|error| format!("invalid training task payload: {error}"))?;
    let source_dataset_id = training_payload["payload"]["dataset_id"]
        .as_str()
        .ok_or_else(|| "training task has no dataset identity".to_string())?;
    if source_dataset_id != request.dataset_id {
        return Err("optimization dataset does not match the training task".to_string());
    }
    let checkpoint: Value = serde_json::from_str(
        training_task
            .checkpoint_json
            .as_deref()
            .ok_or_else(|| "completed training task has no result".to_string())?,
    )
    .map_err(|error| format!("invalid training checkpoint: {error}"))?;
    let artifact = checkpoint["result"]["artifact"].clone();
    let payload = build_optimization_payload(
        &request.dataset_id,
        &request.training_task_id,
        &artifact,
        request,
    )?;
    let task_payload = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
    task_repository::create(
        connection,
        NewTask {
            workspace_id: workspace_id.to_string(),
            kind: crate::compute::handler::COMPUTE_OPTIMIZE_CONSTRAINED_KIND.to_string(),
            payload_json: task_payload,
            checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
            next_run_at: None,
            progress: 0,
        },
    )
    .map_err(|error| error.to_string())
}

pub fn optimization_task_status_on_connection(
    connection: &rusqlite::Connection,
    id: uuid::Uuid,
) -> Result<Value, String> {
    let task = task_repository::get(connection, current_workspace_id(), id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "task_not_found: task not found".to_string())?;
    if task.kind != crate::compute::handler::COMPUTE_OPTIMIZE_CONSTRAINED_KIND {
        return Err("task is not a constrained optimization task".to_string());
    }
    let mut status = json!({
        "task_id": task.id.to_string(),
        "state": task.state,
        "progress": task.progress,
        "attempt": task.attempt,
        "error_code": task.error_code,
    });
    if task.state == crate::tasks::TaskState::Completed {
        let checkpoint: Value = serde_json::from_str(
            task.checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed optimization task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid optimization checkpoint: {error}"))?;
        status["result"] = checkpoint.get("result").cloned().unwrap_or(Value::Null);
    }
    Ok(status)
}

pub(crate) fn get_compute_optimization_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn(&db, |connection| {
        let task = task_repository::get(connection, current_workspace_id(), id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task_not_found: task not found".to_string())?;
        if task.kind != crate::compute::handler::COMPUTE_OPTIMIZE_CONSTRAINED_KIND {
            return Err("task is not a constrained optimization task".to_string());
        }
        if task.state != crate::tasks::TaskState::Completed {
            return Ok(None);
        }
        let checkpoint: Value = serde_json::from_str(
            task.checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed optimization task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid optimization checkpoint: {error}"))?;
        Ok(checkpoint.get("result").cloned())
    })
}

pub(crate) fn predict_onnx_model(
    db: tauri::State<DbState>,
    request: PredictOnnxModelRequest,
) -> Result<BackgroundTaskResponse, String> {
    let task_payload = build_onnx_prediction_payload(&request)?;
    with_conn_mut(&db, |connection| {
        let workspace_id = current_workspace_id().to_string();
        task_repository::create(
            connection,
            NewTask {
                workspace_id,
                kind: crate::compute::handler::COMPUTE_PREDICT_ONNX_KIND.to_string(),
                payload_json: serde_json::to_string(&task_payload)
                    .map_err(|error| error.to_string())?,
                checkpoint_json: Some(json!({"stage": "queued"}).to_string()),
                next_run_at: None,
                progress: 0,
            },
        )
        .map(background_task_response)
        .map_err(|error| error.to_string())
    })
}

pub(crate) fn get_compute_onnx_prediction_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn(&db, |connection| {
        let task = task_repository::get(connection, current_workspace_id(), id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task_not_found: task not found".to_string())?;
        if task.kind != crate::compute::handler::COMPUTE_PREDICT_ONNX_KIND {
            return Err("task is not an ONNX prediction task".to_string());
        }
        if task.state != crate::tasks::TaskState::Completed {
            return Ok(None);
        }
        let checkpoint: Value = serde_json::from_str(
            task.checkpoint_json
                .as_deref()
                .ok_or_else(|| "completed ONNX task has no result".to_string())?,
        )
        .map_err(|error| format!("invalid ONNX checkpoint: {error}"))?;
        Ok(checkpoint.get("result").cloned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn training_and_prediction_task_kind_helpers_cover_supported_models() {
        assert!(crate::compute::handler::is_training_task_kind(
            crate::compute::handler::COMPUTE_TRAIN_LINEAR_REGRESSION_KIND
        ));
        assert!(crate::compute::handler::is_training_task_kind(
            crate::compute::handler::COMPUTE_TRAIN_SKLEARN_KIND
        ));
        assert!(crate::compute::handler::is_prediction_task_kind(
            crate::compute::handler::COMPUTE_PREDICT_LINEAR_REGRESSION_KIND
        ));
        assert!(crate::compute::handler::is_prediction_task_kind(
            crate::compute::handler::COMPUTE_PREDICT_TRAINED_KIND
        ));
        assert!(!crate::compute::handler::is_training_task_kind(
            crate::compute::handler::COMPUTE_PREDICT_TRAINED_KIND
        ));
        assert!(!crate::compute::handler::is_prediction_task_kind(
            crate::compute::handler::COMPUTE_TRAIN_SKLEARN_KIND
        ));
    }

    #[test]
    fn builds_numeric_training_payload_and_preserves_missing_features() {
        let table = DatasetTable {
            source_name: "heats.csv".to_string(),
            format: "csv".to_string(),
            sheets: vec!["CSV".to_string()],
            selected_sheet: "CSV".to_string(),
            headers: vec![
                "heat_id".to_string(),
                "temperature".to_string(),
                "strength".to_string(),
            ],
            rows: vec![
                vec!["H-01".to_string(), "10".to_string(), "355".to_string()],
                vec!["H-02".to_string(), "".to_string(), "360".to_string()],
                vec!["H-03".to_string(), "30".to_string(), "365".to_string()],
            ],
            warnings: Vec::new(),
        };
        let columns = vec![
            SteelDatasetColumnRecord {
                ordinal: 0,
                original_name: "heat_id".to_string(),
                duplicate: false,
                inferred_type: "text".to_string(),
                canonical_field: Some("heat_id".to_string()),
                unit: None,
                non_empty_count: 3,
                missing_count: 0,
                invalid_count: 0,
                min: None,
                max: None,
            },
            SteelDatasetColumnRecord {
                ordinal: 1,
                original_name: "temperature".to_string(),
                duplicate: false,
                inferred_type: "number".to_string(),
                canonical_field: Some("temperature".to_string()),
                unit: Some("C".to_string()),
                non_empty_count: 2,
                missing_count: 1,
                invalid_count: 0,
                min: Some(10.0),
                max: Some(30.0),
            },
            SteelDatasetColumnRecord {
                ordinal: 2,
                original_name: "strength".to_string(),
                duplicate: false,
                inferred_type: "number".to_string(),
                canonical_field: Some("yield_strength".to_string()),
                unit: Some("MPa".to_string()),
                non_empty_count: 3,
                missing_count: 0,
                invalid_count: 0,
                min: Some(355.0),
                max: Some(365.0),
            },
        ];

        let payload = build_linear_regression_payload(
            &table,
            &columns,
            &TrainSteelDatasetRequest {
                dataset_id: "dataset-1".to_string(),
                target_column: 2,
                feature_columns: vec![1],
                split_policy: None,
                algorithm: None,
            },
        )
        .expect("build training payload");

        assert_eq!(
            payload["features"],
            serde_json::json!([[10.0], [null], [30.0]])
        );
        assert_eq!(payload["targets"], serde_json::json!([355.0, 360.0, 365.0]));
        assert_eq!(payload["feature_names"], serde_json::json!(["temperature"]));
        assert_eq!(payload["field_mapping"]["temperature"], "temperature");
    }

    #[test]
    fn builds_optimization_payload_with_named_constraints_and_fixed_values() {
        let artifact = json!({
            "artifact_version": "linear-regression.v1",
            "model_id": "model-1",
            "model_type": "linear_regression",
            "feature_names": ["temperature", "carbon"],
            "feature_schema": {"count": 2},
        });
        let request = OptimizeSteelProcessRequest {
            dataset_id: "dataset-1".to_string(),
            training_task_id: "task-1".to_string(),
            direction: "minimize".to_string(),
            objective_columns: vec![0],
            bounds: vec![
                json!({"min": 0.0, "max": 10.0}),
                json!({"min": 0.0, "max": 5.0}),
            ],
            fixed_values: vec![None, Some(2.0)],
            constraints: vec![json!({
                "kind": "inequality",
                "coefficients": [1.0, 0.0],
                "value": 4.0,
                "tolerance": 0.01,
            })],
            trials: 24,
            seed: 7,
        };

        let payload = build_optimization_payload("dataset-1", "task-1", &artifact, &request)
            .expect("build optimization payload");

        assert_eq!(payload["operation"], "optimize_constrained");
        assert_eq!(payload["payload"]["objectives"], json!(["temperature"]));
        assert_eq!(payload["payload"]["fixed_values"], json!({"carbon": 2.0}));
        assert_eq!(
            payload["payload"]["constraints"][0]["coefficients"],
            json!({"temperature": 1.0, "carbon": 0.0})
        );
        assert_eq!(
            payload["payload"]["constraints"][0]["tolerance"],
            json!(0.01)
        );
        assert_eq!(payload["payload"]["trials"], json!(24));
        assert_eq!(payload["payload"]["seed"], json!(7));
    }

    #[test]
    fn rejects_optimization_requests_with_invalid_bounds_or_trials() {
        let artifact = json!({
            "artifact_version": "linear-regression.v1",
            "model_id": "model-1",
            "model_type": "linear_regression",
            "feature_names": ["temperature"],
            "feature_schema": {"count": 1},
        });
        let base = OptimizeSteelProcessRequest {
            dataset_id: "dataset-1".to_string(),
            training_task_id: "task-1".to_string(),
            direction: "minimize".to_string(),
            objective_columns: vec![0],
            bounds: vec![json!({"min": 5.0, "max": 1.0})],
            fixed_values: vec![None],
            constraints: Vec::new(),
            trials: 24,
            seed: 0,
        };

        let error = build_optimization_payload("dataset-1", "task-1", &artifact, &base)
            .expect_err("inverted bounds must be rejected");
        assert_eq!(error, "bounds[0] is inverted");

        let oversized = OptimizeSteelProcessRequest {
            bounds: vec![json!({"min": 0.0, "max": 10.0})],
            trials: 10_000,
            ..base.clone()
        };
        let error = build_optimization_payload("dataset-1", "task-1", &artifact, &oversized)
            .expect_err("oversized trial counts must be rejected");
        assert_eq!(error, "trials must be between 1 and 500");

        let bad_objective = OptimizeSteelProcessRequest {
            objective_columns: vec![3],
            bounds: vec![json!({"min": 0.0, "max": 10.0})],
            trials: 24,
            ..base
        };
        let error = build_optimization_payload("dataset-1", "task-1", &artifact, &bad_objective)
            .expect_err("out-of-range objectives must be rejected");
        assert_eq!(error, "objective column 3 is out of range");
    }

    #[test]
    fn builds_prediction_payload_with_model_provenance_and_features() {
        let artifact = json!({
            "artifact_version": "linear-regression.v1",
            "model_id": "model-1",
            "model_type": "linear_regression",
            "feature_names": ["temperature", "carbon"],
            "feature_schema": {"count": 2},
        });

        let payload = build_linear_regression_prediction_payload(
            "dataset-1",
            "task-1",
            &artifact,
            &[125.0, 0.2],
        )
        .expect("build prediction payload");

        assert_eq!(payload["operation"], "predict_linear_regression");
        assert_eq!(payload["payload"]["dataset_id"], "dataset-1");
        assert_eq!(payload["payload"]["training_task_id"], "task-1");
        assert_eq!(payload["payload"]["features"], json!([[125.0, 0.2]]));
        assert_eq!(payload["payload"]["artifact"]["model_id"], "model-1");
    }

    #[test]
    fn rejects_prediction_features_that_do_not_match_model_schema() {
        let artifact = json!({
            "artifact_version": "linear-regression.v1",
            "model_id": "model-1",
            "model_type": "linear_regression",
            "feature_names": ["temperature", "carbon"],
            "feature_schema": {"count": 2},
        });

        let error =
            build_linear_regression_prediction_payload("dataset-1", "task-1", &artifact, &[125.0])
                .expect_err("feature count mismatch must be rejected");

        assert_eq!(error, "prediction feature count does not match the model");
    }

    #[test]
    fn builds_onnx_prediction_payload_with_hash_and_batch_features() {
        let path =
            std::env::temp_dir().join(format!("bloomery-model-{}.onnx", uuid::Uuid::new_v4()));
        let bytes = b"onnx-fixture";
        std::fs::write(&path, bytes).expect("write model fixture");
        let mut digest = Sha256::new();
        digest.update(bytes);
        let request = PredictOnnxModelRequest {
            model_path: path.to_string_lossy().into_owned(),
            model_sha256: format!("{:x}", digest.finalize()),
            manifest: json!({
                "model_id": "mul-model",
                "model_version": "1.0.0",
                "inputs": [{"name": "X", "dtype": "float32", "shape": [-1, 2]}],
                "outputs": [{"name": "Y", "dtype": "float32", "shape": [-1, 2]}],
                "preprocessing": {
                    "feature_names": ["temperature", "carbon"],
                    "means": [0.0, 0.0],
                    "scales": [1.0, 1.0]
                }
            }),
            features: vec![vec![1.0, 2.0], vec![3.0, 4.0]],
        };

        let payload = build_onnx_prediction_payload(&request).expect("build ONNX payload");

        assert_eq!(payload["operation"], "predict_onnx");
        assert_eq!(payload["payload"]["model_sha256"], request.model_sha256);
        assert_eq!(
            payload["payload"]["features"],
            json!([[1.0, 2.0], [3.0, 4.0]])
        );
        assert_eq!(payload["payload"]["manifest"]["model_id"], "mul-model");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_onnx_prediction_when_model_hash_is_wrong() {
        let path =
            std::env::temp_dir().join(format!("bloomery-model-{}.onnx", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"onnx-fixture").expect("write model fixture");
        let request = PredictOnnxModelRequest {
            model_path: path.to_string_lossy().into_owned(),
            model_sha256: "0".repeat(64),
            manifest: json!({"model_id": "mul-model", "model_version": "1.0.0"}),
            features: vec![vec![1.0]],
        };

        let error = build_onnx_prediction_payload(&request)
            .expect_err("wrong ONNX model hash must be rejected");

        assert_eq!(error, "ONNX model hash does not match the selected file");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn hashes_onnx_model_file_for_ui_pinning() {
        let path =
            std::env::temp_dir().join(format!("bloomery-hash-{}.onnx", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"onnx-fixture").expect("write model fixture");

        let mut digest = Sha256::new();
        digest.update(b"onnx-fixture");
        let expected = format!("{:x}", digest.finalize());

        assert_eq!(
            hash_onnx_model_file(&path.to_string_lossy()).expect("hash model"),
            expected
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_hashing_files_without_onnx_extension() {
        let path = std::env::temp_dir().join(format!("bloomery-hash-{}.bin", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"onnx-fixture").expect("write model fixture");

        let error = hash_onnx_model_file(&path.to_string_lossy())
            .expect_err("non-ONNX extensions must be rejected");

        assert_eq!(error, "ONNX model path must use the .onnx extension");
        let _ = std::fs::remove_file(path);
    }
}
