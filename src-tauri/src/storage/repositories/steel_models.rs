use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SteelModelRecord {
    pub id: String,
    pub lineage_id: String,
    pub kind: String,
    pub version: i64,
    pub source_task_id: Option<String>,
    pub model_sha256: String,
    pub manifest_json: String,
    pub artifact_json: Option<String>,
    pub model_base64: Option<String>,
    pub is_active: bool,
    pub created_at: String,
}

pub struct NewSteelModel<'a> {
    pub lineage_id: &'a str,
    pub kind: &'a str,
    pub source_task_id: Option<&'a str>,
    pub model_sha256: &'a str,
    pub manifest_json: &'a str,
    pub artifact_json: Option<&'a str>,
    pub model_base64: Option<&'a str>,
}

const SELECT_MODEL: &str = "SELECT id, lineage_id, kind, version, source_task_id,
         model_sha256, manifest_json, artifact_json, model_base64, is_active, created_at
         FROM steel_models WHERE workspace_id = ?1 AND id = ?2";

fn row_to_model(row: &rusqlite::Row<'_>) -> rusqlite::Result<SteelModelRecord> {
    Ok(SteelModelRecord {
        id: row.get(0)?,
        lineage_id: row.get(1)?,
        kind: row.get(2)?,
        version: row.get(3)?,
        source_task_id: row.get(4)?,
        model_sha256: row.get(5)?,
        manifest_json: row.get(6)?,
        artifact_json: row.get(7)?,
        model_base64: row.get(8)?,
        is_active: row.get::<_, i64>(9)? != 0,
        created_at: row.get(10)?,
    })
}

pub fn create(
    connection: &mut Connection,
    workspace_id: &str,
    model: NewSteelModel<'_>,
) -> Result<SteelModelRecord, String> {
    if model.lineage_id.trim().is_empty() {
        return Err("model lineage is required".to_string());
    }
    if model.kind != "linear_artifact" && model.kind != "sklearn_artifact" && model.kind != "onnx" {
        return Err("model kind must be linear_artifact, sklearn_artifact, or onnx".to_string());
    }
    if model.model_sha256.len() != 64
        || !model
            .model_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("model sha256 is invalid".to_string());
    }
    let manifest: serde_json::Value = serde_json::from_str(model.manifest_json)
        .map_err(|error| format!("model manifest is invalid: {error}"))?;
    if !manifest.is_object() {
        return Err("model manifest must be an object".to_string());
    }
    match model.kind {
        "linear_artifact" if model.artifact_json.is_none() || model.model_base64.is_some() => {
            return Err("linear model versions must store an artifact and no blob".to_string());
        }
        "sklearn_artifact" if model.artifact_json.is_none() || model.model_base64.is_some() => {
            return Err("sklearn model versions must store an artifact and no blob".to_string());
        }
        "onnx" if model.model_base64.is_none() || model.artifact_json.is_some() => {
            return Err("onnx model versions must store a blob and no artifact".to_string());
        }
        _ => {}
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let version: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM steel_models
             WHERE workspace_id = ?1 AND lineage_id = ?2",
            params![workspace_id, model.lineage_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let is_active = if version == 1 { 1 } else { 0 };
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    transaction
        .execute(
            "INSERT INTO steel_models
               (workspace_id, id, lineage_id, kind, version, source_task_id,
                model_sha256, manifest_json, artifact_json, model_base64, is_active, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                workspace_id,
                id,
                model.lineage_id,
                model.kind,
                version,
                model.source_task_id,
                model.model_sha256,
                model.manifest_json,
                model.artifact_json,
                model.model_base64,
                is_active,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    get(connection, workspace_id, &id)?
        .ok_or_else(|| "created model version could not be read back".to_string())
}

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    id: &str,
) -> Result<Option<SteelModelRecord>, String> {
    connection
        .query_row(SELECT_MODEL, params![workspace_id, id], row_to_model)
        .optional()
        .map_err(|error| error.to_string())
}

pub fn list(
    connection: &Connection,
    workspace_id: &str,
    lineage_id: &str,
) -> Result<Vec<SteelModelRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, lineage_id, kind, version, source_task_id,
                    model_sha256, manifest_json, artifact_json, model_base64, is_active, created_at
             FROM steel_models
             WHERE workspace_id = ?1 AND lineage_id = ?2
             ORDER BY version DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id, lineage_id], row_to_model)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn set_active(
    connection: &mut Connection,
    workspace_id: &str,
    id: &str,
) -> Result<SteelModelRecord, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let target = transaction
        .query_row(SELECT_MODEL, params![workspace_id, id], row_to_model)
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "model version was not found".to_string())?;
    transaction
        .execute(
            "UPDATE steel_models SET is_active = 0
             WHERE workspace_id = ?1 AND lineage_id = ?2",
            params![workspace_id, target.lineage_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE steel_models SET is_active = 1 WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    get(connection, workspace_id, id)?
        .ok_or_else(|| "activated model version could not be read back".to_string())
}

/// Release rule: the active model version of a lineage cannot be deleted.
pub fn delete(connection: &mut Connection, workspace_id: &str, id: &str) -> Result<(), String> {
    let target = get(connection, workspace_id, id)?
        .ok_or_else(|| "model version was not found".to_string())?;
    if target.is_active {
        return Err("active model versions cannot be deleted".to_string());
    }
    connection
        .execute(
            "DELETE FROM steel_models WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
