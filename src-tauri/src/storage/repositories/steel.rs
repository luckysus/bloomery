use crate::steel::DatasetPreview;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::HashSet;
use uuid::Uuid;

mod model;

pub use model::{DatasetColumnMapping, SteelDatasetColumnRecord, SteelDatasetRecord};

pub fn save_preview(
    connection: &mut Connection,
    workspace_id: &str,
    source_path: &str,
    source_sha256: &str,
    preview: &DatasetPreview,
    mappings: &[DatasetColumnMapping],
) -> Result<SteelDatasetRecord, String> {
    let mappings = normalize_mappings(preview, mappings)?;
    let preview_json = serde_json::to_string(preview).map_err(|error| error.to_string())?;
    let now = Utc::now().to_rfc3339();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "INSERT INTO steel_datasets
               (workspace_id, id, source_name, source_path, source_sha256, format,
                selected_sheet, row_count, column_count, truncated, mapping_state,
                preview_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'draft', ?11, ?12, ?12)
             ON CONFLICT(workspace_id, source_sha256, selected_sheet) DO UPDATE SET
               source_name = excluded.source_name,
               source_path = excluded.source_path,
               format = excluded.format,
               row_count = excluded.row_count,
               column_count = excluded.column_count,
               truncated = excluded.truncated,
               mapping_state = 'draft',
               preview_json = excluded.preview_json,
               updated_at = excluded.updated_at",
            params![
                workspace_id,
                Uuid::new_v4().to_string(),
                preview.source_name,
                source_path,
                source_sha256,
                preview.format,
                preview.selected_sheet,
                i64::try_from(preview.row_count).map_err(|_| "dataset row count is too large")?,
                i64::try_from(preview.column_count)
                    .map_err(|_| "dataset column count is too large")?,
                if preview.truncated { 1 } else { 0 },
                preview_json,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;

    let dataset_id: String = transaction
        .query_row(
            "SELECT id FROM steel_datasets
             WHERE workspace_id = ?1 AND source_sha256 = ?2 AND selected_sheet = ?3",
            params![workspace_id, source_sha256, preview.selected_sheet],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM steel_dataset_columns
             WHERE workspace_id = ?1 AND dataset_id = ?2",
            params![workspace_id, dataset_id],
        )
        .map_err(|error| error.to_string())?;

    for (ordinal, column) in preview.columns.iter().enumerate() {
        let mapping = mappings.iter().find(|mapping| mapping.ordinal == ordinal);
        transaction
            .execute(
                "INSERT INTO steel_dataset_columns
                   (workspace_id, dataset_id, ordinal, original_name, duplicate,
                    inferred_type, canonical_field, unit, non_empty_count, missing_count,
                    invalid_count, min_value, max_value)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    workspace_id,
                    dataset_id,
                    i64::try_from(ordinal).map_err(|_| "dataset column index is too large")?,
                    column.name,
                    if column.duplicate { 1 } else { 0 },
                    column.inferred_type,
                    mapping.and_then(|mapping| mapping.canonical_field.as_deref()),
                    mapping.and_then(|mapping| mapping.unit.as_deref()),
                    i64::try_from(column.non_empty_count)
                        .map_err(|_| "dataset non-empty count is too large")?,
                    i64::try_from(column.missing_count)
                        .map_err(|_| "dataset missing count is too large")?,
                    i64::try_from(column.invalid_count)
                        .map_err(|_| "dataset invalid count is too large")?,
                    column.min,
                    column.max,
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    get(connection, workspace_id, &dataset_id)?
        .ok_or_else(|| "saved dataset disappeared".to_string())
}

pub fn activate(
    connection: &mut Connection,
    workspace_id: &str,
    dataset_id: &str,
) -> Result<Option<SteelDatasetRecord>, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let exists = transaction
        .query_row(
            "SELECT 1 FROM steel_datasets WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, dataset_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if !exists {
        return Ok(None);
    }

    let has_canonical_field = {
        let mut statement = transaction
            .prepare(
                "SELECT duplicate, canonical_field, unit
                 FROM steel_dataset_columns
                 WHERE workspace_id = ?1 AND dataset_id = ?2
                 ORDER BY ordinal",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![workspace_id, dataset_id], |row| {
                Ok((
                    row.get::<_, i64>(0)? != 0,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        let mut has_canonical_field = false;
        for row in rows {
            let (duplicate, canonical_field, unit) = row.map_err(|error| error.to_string())?;
            if duplicate {
                return Err(
                    "dataset activation requires duplicate columns to be resolved".to_string(),
                );
            }
            if canonical_field
                .as_deref()
                .is_some_and(|field| !field.trim().is_empty())
            {
                has_canonical_field = true;
            } else if unit
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                return Err(
                    "dataset activation requires every unit to have a canonical field".to_string(),
                );
            }
        }
        has_canonical_field
    };
    if !has_canonical_field {
        return Err("dataset activation requires at least one canonical field".to_string());
    }

    transaction
        .execute(
            "UPDATE steel_datasets SET mapping_state = 'ready', updated_at = ?3
             WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, dataset_id, Utc::now().to_rfc3339()],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    get(connection, workspace_id, dataset_id)
}

pub fn list(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<SteelDatasetRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, source_name, source_path, source_sha256, format, selected_sheet,
                    row_count, column_count, truncated, mapping_state, preview_json,
                    created_at, updated_at
             FROM steel_datasets
             WHERE workspace_id = ?1
             ORDER BY updated_at DESC, id",
        )
        .map_err(|error| error.to_string())?;
    let parents = statement
        .query_map(params![workspace_id], map_parent)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    parents
        .into_iter()
        .map(|mut record| {
            record.columns = list_columns(connection, workspace_id, &record.id)?;
            Ok(record)
        })
        .collect()
}

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    dataset_id: &str,
) -> Result<Option<SteelDatasetRecord>, String> {
    let mut record = connection
        .query_row(
            "SELECT id, source_name, source_path, source_sha256, format, selected_sheet,
                    row_count, column_count, truncated, mapping_state, preview_json,
                    created_at, updated_at
             FROM steel_datasets
             WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, dataset_id],
            map_parent,
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(record) = &mut record {
        record.columns = list_columns(connection, workspace_id, &record.id)?;
    }
    Ok(record)
}

fn list_columns(
    connection: &Connection,
    workspace_id: &str,
    dataset_id: &str,
) -> Result<Vec<SteelDatasetColumnRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT ordinal, original_name, duplicate, inferred_type, canonical_field, unit,
                    non_empty_count, missing_count, invalid_count, min_value, max_value
             FROM steel_dataset_columns
             WHERE workspace_id = ?1 AND dataset_id = ?2
             ORDER BY ordinal",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id, dataset_id], map_column)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string());
    rows
}

fn map_parent(row: &rusqlite::Row<'_>) -> rusqlite::Result<SteelDatasetRecord> {
    let preview_json: String = row.get(10)?;
    Ok(SteelDatasetRecord {
        id: row.get(0)?,
        source_name: row.get(1)?,
        source_path: row.get(2)?,
        source_sha256: row.get(3)?,
        format: row.get(4)?,
        selected_sheet: row.get(5)?,
        row_count: count_value(row.get(6)?, 6)?,
        column_count: count_value(row.get(7)?, 7)?,
        truncated: row.get::<_, i64>(8)? != 0,
        mapping_state: row.get(9)?,
        preview: serde_json::from_str(&preview_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        columns: Vec::new(),
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn map_column(row: &rusqlite::Row<'_>) -> rusqlite::Result<SteelDatasetColumnRecord> {
    Ok(SteelDatasetColumnRecord {
        ordinal: count_value(row.get(0)?, 0)?,
        original_name: row.get(1)?,
        duplicate: row.get::<_, i64>(2)? != 0,
        inferred_type: row.get(3)?,
        canonical_field: row.get(4)?,
        unit: row.get(5)?,
        non_empty_count: count_value(row.get(6)?, 6)?,
        missing_count: count_value(row.get(7)?, 7)?,
        invalid_count: count_value(row.get(8)?, 8)?,
        min: row.get(9)?,
        max: row.get(10)?,
    })
}

fn count_value(value: i64, column: usize) -> rusqlite::Result<usize> {
    usize::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn normalize_mappings(
    preview: &DatasetPreview,
    mappings: &[DatasetColumnMapping],
) -> Result<Vec<DatasetColumnMapping>, String> {
    let mut ordinals = HashSet::new();
    let mut canonical_fields = HashSet::new();
    let mut normalized = Vec::with_capacity(mappings.len());
    for mapping in mappings {
        if mapping.ordinal >= preview.columns.len() {
            return Err(format!(
                "dataset mapping ordinal {} is out of range",
                mapping.ordinal
            ));
        }
        if !ordinals.insert(mapping.ordinal) {
            return Err(format!(
                "dataset mapping ordinal {} is duplicated",
                mapping.ordinal
            ));
        }
        let canonical_field = mapping
            .canonical_field
            .as_deref()
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(str::to_string);
        let unit = mapping
            .unit
            .as_deref()
            .map(str::trim)
            .filter(|unit| !unit.is_empty())
            .map(str::to_string);
        if let Some(field) = canonical_field.as_deref() {
            if !is_canonical_field(field) {
                return Err(format!("canonical field {field} must use lower snake_case"));
            }
            if !canonical_fields.insert(field.to_ascii_lowercase()) {
                return Err(format!(
                    "dataset canonical field {field} is mapped more than once"
                ));
            }
        }
        if unit.is_some() && canonical_field.is_none() {
            return Err("dataset unit requires a canonical field".to_string());
        }
        if let Some(unit) = unit.as_deref() {
            if unit.chars().count() > 24
                || unit
                    .chars()
                    .any(|character| character.is_control() || character.is_whitespace())
            {
                return Err(format!("dataset unit {unit} contains invalid characters"));
            }
        }
        if canonical_field.is_some() || unit.is_some() {
            normalized.push(DatasetColumnMapping {
                ordinal: mapping.ordinal,
                canonical_field,
                unit,
            });
        }
    }
    Ok(normalized)
}

fn is_canonical_field(field: &str) -> bool {
    let mut characters = field.chars();
    matches!(characters.next(), Some(character) if character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !field.ends_with('_')
        && !field.contains("__")
}
