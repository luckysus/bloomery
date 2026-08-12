mod csv;

use crate::permissions::path::authorize_existing_file;
use crate::rag::ingest::SourceFormat;
use crate::rag::model::SourceLocation;
use crate::rag::parse::{parse_document, DocumentBlock, ParseLimits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const MAX_PREVIEW_ROWS: usize = 100_000;
const MAX_PREVIEW_COLUMNS: usize = 256;
const SAMPLE_ROW_LIMIT: usize = 20;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetPreviewRequest {
    pub source_path: String,
    #[serde(default)]
    pub sheet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetPreview {
    pub source_name: String,
    pub format: String,
    pub sheets: Vec<String>,
    pub selected_sheet: String,
    pub row_count: usize,
    pub column_count: usize,
    pub truncated: bool,
    pub columns: Vec<DatasetColumnPreview>,
    pub sample_rows: Vec<Vec<String>>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetColumnPreview {
    pub name: String,
    pub duplicate: bool,
    pub inferred_type: String,
    pub non_empty_count: usize,
    pub missing_count: usize,
    pub invalid_count: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetTable {
    pub source_name: String,
    pub format: String,
    pub sheets: Vec<String>,
    pub selected_sheet: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

pub fn read_dataset_table(request: &DatasetPreviewRequest) -> Result<DatasetTable, String> {
    let source_path = request.source_path.trim();
    if source_path.is_empty() {
        return Err("dataset source path is required".to_string());
    }
    let path = authorize_existing_file(Path::new(source_path))
        .map_err(|error| format!("dataset source path is not authorized: {error}"))?;
    let path = path.canonical_path();
    let format = format_for_path(path)?;
    if format == SourceFormat::Csv {
        return csv::read_dataset_table(path, request.sheet.as_deref());
    }
    let parsed =
        parse_document(path, format, ParseLimits::default()).map_err(|error| error.to_string())?;
    let tables = parsed
        .blocks
        .iter()
        .filter_map(|block| match block {
            DocumentBlock::Table { location, rows } => Some((sheet_name(location), rows)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if tables.is_empty() {
        return Err("dataset does not contain a table".to_string());
    }

    let sheets = tables
        .iter()
        .map(|(sheet, _)| sheet.clone())
        .collect::<Vec<_>>();
    let selected_index = match request.sheet.as_deref() {
        Some(requested) => sheets
            .iter()
            .position(|sheet| sheet == requested)
            .ok_or_else(|| "requested dataset sheet was not found".to_string())?,
        None => 0,
    };
    let selected_sheet = sheets[selected_index].clone();
    let source_rows = tables[selected_index].1;
    let width = source_rows.iter().map(Vec::len).max().unwrap_or(0);
    if width > MAX_PREVIEW_COLUMNS {
        return Err(format!(
            "dataset has more than {MAX_PREVIEW_COLUMNS} columns"
        ));
    }
    let headers = if source_rows.is_empty() || width == 0 {
        Vec::new()
    } else {
        (0..width)
            .map(|index| {
                source_rows[0]
                    .get(index)
                    .map(String::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .enumerate()
            .map(|(index, value)| {
                if value.is_empty() {
                    format!("column_{}", index + 1)
                } else {
                    value
                }
            })
            .collect()
    };
    let row_count = source_rows.len().saturating_sub(1);
    let rows = if source_rows.len() <= 1 {
        Vec::new()
    } else {
        source_rows[1..]
            .iter()
            .take(MAX_PREVIEW_ROWS)
            .map(|row| {
                (0..width)
                    .map(|index| row.get(index).cloned().unwrap_or_default())
                    .collect()
            })
            .collect()
    };
    let truncated = row_count > rows.len();

    let mut warnings = parsed
        .warnings
        .into_iter()
        .map(|warning| warning.message)
        .collect::<Vec<_>>();
    if truncated {
        warnings.push(format!(
            "dataset preview is limited to the first {MAX_PREVIEW_ROWS} rows"
        ));
    }

    Ok(DatasetTable {
        source_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dataset")
            .to_string(),
        format: format.as_str().to_string(),
        sheets,
        selected_sheet,
        headers,
        rows,
        row_count,
        truncated,
        warnings,
    })
}

pub fn preview_dataset(request: &DatasetPreviewRequest) -> Result<DatasetPreview, String> {
    let table = read_dataset_table(request)?;
    let width = table.headers.len();
    if width == 0 {
        return Ok(DatasetPreview {
            source_name: table.source_name,
            format: table.format,
            sheets: table.sheets,
            selected_sheet: table.selected_sheet,
            row_count: table.row_count,
            column_count: 0,
            truncated: table.truncated,
            columns: Vec::new(),
            sample_rows: Vec::new(),
            warnings: table.warnings,
        });
    }
    let headers = table.headers;
    let mut names = HashMap::<String, usize>::new();
    let duplicate_flags = headers
        .iter()
        .map(|name| {
            let count = names.entry(name.clone()).or_default();
            *count += 1;
            *count > 1
        })
        .collect::<Vec<_>>();
    let data_rows = &table.rows;
    let mut columns = Vec::with_capacity(width);
    for column_index in 0..width {
        let values = data_rows
            .iter()
            .map(|row| row.get(column_index).map(String::as_str).unwrap_or(""));
        let mut non_empty_count = 0;
        let mut missing_count = 0;
        let mut numeric_values = Vec::new();
        let mut non_numeric_count = 0;
        let mut date_like_count = 0;
        for raw in values {
            let value = raw.trim();
            if value.is_empty() {
                missing_count += 1;
                continue;
            }
            non_empty_count += 1;
            if let Ok(number) = value.parse::<f64>() {
                if number.is_finite() {
                    numeric_values.push(number);
                    continue;
                }
            }
            non_numeric_count += 1;
            if is_date_like(value) {
                date_like_count += 1;
            }
        }
        let inferred_type = if !numeric_values.is_empty() {
            "number"
        } else if non_empty_count > 0 && date_like_count == non_empty_count {
            "date"
        } else {
            "text"
        };
        columns.push(DatasetColumnPreview {
            name: headers[column_index].clone(),
            duplicate: duplicate_flags[column_index],
            inferred_type: inferred_type.to_string(),
            non_empty_count,
            missing_count,
            invalid_count: if inferred_type == "number" {
                non_numeric_count
            } else {
                0
            },
            min: numeric_values.iter().copied().reduce(f64::min),
            max: numeric_values.iter().copied().reduce(f64::max),
        });
    }

    let sample_rows = data_rows
        .iter()
        .take(SAMPLE_ROW_LIMIT)
        .map(|row| {
            (0..width)
                .map(|index| row.get(index).cloned().unwrap_or_default())
                .collect()
        })
        .collect::<Vec<Vec<String>>>();
    let mut warnings = table.warnings;
    if duplicate_flags.iter().any(|duplicate| *duplicate) {
        warnings.push("duplicate column names require explicit mapping".to_string());
    }

    Ok(DatasetPreview {
        source_name: table.source_name,
        format: table.format,
        sheets: table.sheets,
        selected_sheet: table.selected_sheet,
        row_count: table.row_count,
        column_count: width,
        truncated: table.truncated,
        columns,
        sample_rows,
        warnings,
    })
}

pub fn hash_dataset_source(source_path: &str) -> Result<String, String> {
    let authorized = authorize_existing_file(Path::new(source_path.trim()))
        .map_err(|error| format!("dataset source path is not authorized: {error}"))?;
    let file = File::open(authorized.canonical_path())
        .map_err(|error| format!("could not read dataset source: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("could not hash dataset source: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn format_for_path(path: &Path) -> Result<SourceFormat, String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("csv") => Ok(SourceFormat::Csv),
        Some("xlsx") => Ok(SourceFormat::Xlsx),
        Some(extension) => Err(format!("unsupported dataset format: {extension}")),
        None => Err("dataset file extension is required".to_string()),
    }
}

fn sheet_name(location: &SourceLocation) -> String {
    match location {
        SourceLocation::SheetRange { sheet, .. } => sheet.clone(),
        _ => "DATA".to_string(),
    }
}

fn is_date_like(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
        || chrono::DateTime::parse_from_rfc3339(value).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn previews_csv_columns_quality_and_sample_rows() {
        let path =
            std::env::temp_dir().join(format!("bloomery-dataset-{}.csv", uuid::Uuid::new_v4()));
        fs::write(
            &path,
            "heat_id,yield_strength,grade\nH-01,355,Q355B\nH-02,,Q355B\nH-03,invalid,Q235B\n",
        )
        .expect("write fixture");

        let preview = preview_dataset(&DatasetPreviewRequest {
            source_path: path.to_string_lossy().into_owned(),
            sheet: None,
        })
        .expect("preview dataset");

        assert_eq!(preview.format, "csv");
        assert_eq!(preview.row_count, 3);
        assert_eq!(preview.column_count, 3);
        assert_eq!(preview.columns[0].name, "heat_id");
        assert_eq!(preview.columns[0].inferred_type, "text");
        assert_eq!(preview.columns[1].missing_count, 1);
        assert_eq!(preview.columns[1].invalid_count, 1);
        assert_eq!(preview.columns[1].min, Some(355.0));
        assert_eq!(preview.columns[1].max, Some(355.0));
        assert_eq!(preview.sample_rows.len(), 3);

        let _ = fs::remove_file(path);
    }
}
