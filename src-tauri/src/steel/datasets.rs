use crate::rag::ingest::SourceFormat;
use crate::rag::model::SourceLocation;
use crate::rag::parse::{parse_document, DocumentBlock, ParseLimits};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn preview_dataset(request: &DatasetPreviewRequest) -> Result<DatasetPreview, String> {
    let source_path = request.source_path.trim();
    if source_path.is_empty() {
        return Err("dataset source path is required".to_string());
    }
    let path = Path::new(source_path);
    if !path.is_file() {
        return Err("dataset source file does not exist".to_string());
    }
    let format = format_for_path(path)?;
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
    let rows = tables[selected_index].1;
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 || rows.is_empty() {
        return Ok(empty_preview(
            path,
            format,
            sheets,
            selected_sheet,
            parsed.warnings,
        ));
    }
    if width > MAX_PREVIEW_COLUMNS {
        return Err(format!(
            "dataset has more than {MAX_PREVIEW_COLUMNS} columns"
        ));
    }

    let headers = rows[0]
        .iter()
        .take(width)
        .enumerate()
        .map(|(index, value)| {
            let value = value.trim();
            if value.is_empty() {
                format!("column_{}", index + 1)
            } else {
                value.to_string()
            }
        })
        .collect::<Vec<_>>();
    let mut names = HashMap::<String, usize>::new();
    let duplicate_flags = headers
        .iter()
        .map(|name| {
            let count = names.entry(name.clone()).or_default();
            *count += 1;
            *count > 1
        })
        .collect::<Vec<_>>();
    let data_rows = &rows[1..];
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
    let mut warnings = parsed
        .warnings
        .into_iter()
        .map(|warning| warning.message)
        .collect::<Vec<_>>();
    if duplicate_flags.iter().any(|duplicate| *duplicate) {
        warnings.push("duplicate column names require explicit mapping".to_string());
    }

    Ok(DatasetPreview {
        source_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dataset")
            .to_string(),
        format: format.as_str().to_string(),
        sheets,
        selected_sheet,
        row_count: data_rows.len(),
        column_count: width,
        truncated: data_rows.len() > MAX_PREVIEW_ROWS,
        columns,
        sample_rows,
        warnings,
    })
}

fn empty_preview(
    path: &Path,
    format: SourceFormat,
    sheets: Vec<String>,
    selected_sheet: String,
    warnings: Vec<crate::rag::parse::ParseWarning>,
) -> DatasetPreview {
    DatasetPreview {
        source_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dataset")
            .to_string(),
        format: format.as_str().to_string(),
        sheets,
        selected_sheet,
        row_count: 0,
        column_count: 0,
        truncated: false,
        columns: Vec::new(),
        sample_rows: Vec::new(),
        warnings: warnings
            .into_iter()
            .map(|warning| warning.message)
            .collect(),
    }
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
