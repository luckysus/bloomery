use crate::app::task_commands::tasks::{background_task_response, BackgroundTaskResponse};
use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::steel::{hash_dataset_source, read_dataset_table, DatasetTable};
use crate::storage::repositories::steel::{self as steel_repository, SteelDatasetColumnRecord};
use crate::tasks::{repository as task_repository, NewTask};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainSteelDatasetRequest {
    pub dataset_id: String,
    pub target_column: usize,
    pub feature_columns: Vec<usize>,
    #[serde(default)]
    pub split_policy: Option<Value>,
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

pub fn train_steel_dataset(
    db: tauri::State<DbState>,
    request: TrainSteelDatasetRequest,
) -> Result<BackgroundTaskResponse, String> {
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
        let task_payload = json!({
            "operation": "train_linear_regression",
            "payload": {
                "data_version": source_sha256,
                "features": payload["features"],
                "targets": payload["targets"],
                "feature_names": payload["feature_names"],
                "field_mapping": payload["field_mapping"],
                "split_policy": payload["split_policy"],
            }
        });
        task_repository::create(
            connection,
            NewTask {
                workspace_id,
                kind: crate::compute::handler::COMPUTE_TRAIN_LINEAR_REGRESSION_KIND.to_string(),
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

pub fn get_compute_training_result(
    db: tauri::State<DbState>,
    id: String,
) -> Result<Option<Value>, String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|error| format!("invalid task ID: {error}"))?;
    with_conn(&db, |connection| {
        let task = task_repository::get(connection, current_workspace_id(), id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task_not_found: task not found".to_string())?;
        if task.kind != crate::compute::handler::COMPUTE_TRAIN_LINEAR_REGRESSION_KIND {
            return Err("task is not a linear regression training task".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
