use super::{DatasetTable, MAX_PREVIEW_COLUMNS, MAX_PREVIEW_ROWS};
use crate::rag::parse::ParseLimits;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub(super) fn read_dataset_table(
    path: &Path,
    requested_sheet: Option<&str>,
) -> Result<DatasetTable, String> {
    if requested_sheet.is_some_and(|sheet| sheet != "CSV") {
        return Err("requested dataset sheet was not found".to_string());
    }
    let file =
        File::open(path).map_err(|error| format!("could not read dataset source: {error}"))?;
    let max_source_bytes = ParseLimits::default().max_source_bytes;
    let source_bytes = file
        .metadata()
        .map_err(|error| format!("could not inspect dataset source: {error}"))?
        .len();
    if source_bytes > max_source_bytes {
        return Err(format!(
            "parse_source_too_large: parse source exceeds the {max_source_bytes}-byte limit"
        ));
    }
    let mut rows = CsvRowReader::new(BufReader::new(file));
    let Some(header_row) = rows.next_row()? else {
        return Err("dataset does not contain a table".to_string());
    };
    let header_row_len = header_row.len();
    let mut width = header_row_len;
    validate_width(width)?;
    let mut headers = header_row
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            if index == 0 {
                value.strip_prefix('\u{feff}').unwrap_or(&value).to_string()
            } else {
                value
            }
        })
        .enumerate()
        .map(|(index, value)| {
            let value = value.trim().to_string();
            if value.is_empty() {
                format!("column_{}", index + 1)
            } else {
                value
            }
        })
        .collect::<Vec<_>>();
    let mut data_rows = Vec::new();
    let mut row_count = 0;
    let mut ragged = false;
    while let Some(row) = rows.next_row()? {
        ragged |= row.len() != header_row_len;
        width = width.max(row.len());
        validate_width(width)?;
        row_count += 1;
        if data_rows.len() < MAX_PREVIEW_ROWS {
            data_rows.push(row);
        }
    }
    if headers.len() < width {
        headers.extend((headers.len()..width).map(|index| format!("column_{}", index + 1)));
    }
    for row in &mut data_rows {
        row.resize(width, String::new());
    }
    let truncated = row_count > data_rows.len();
    let mut warnings = Vec::new();
    if ragged {
        warnings.push("CSV rows have inconsistent column counts".to_string());
    }
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
        format: "csv".to_string(),
        sheets: vec!["CSV".to_string()],
        selected_sheet: "CSV".to_string(),
        headers,
        rows: data_rows,
        row_count,
        truncated,
        warnings,
    })
}

fn validate_width(width: usize) -> Result<(), String> {
    if width > MAX_PREVIEW_COLUMNS {
        return Err(format!(
            "dataset has more than {MAX_PREVIEW_COLUMNS} columns"
        ));
    }
    Ok(())
}

struct CsvRowReader<R> {
    reader: R,
    row: Vec<String>,
    field: String,
    quoted: bool,
    quote_closed: bool,
}

impl<R: BufRead> CsvRowReader<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            row: Vec::new(),
            field: String::new(),
            quoted: false,
            quote_closed: false,
        }
    }

    fn next_row(&mut self) -> Result<Option<Vec<String>>, String> {
        loop {
            let mut bytes = Vec::new();
            match self.reader.read_until(b'\n', &mut bytes) {
                Ok(0) => {
                    if self.quoted {
                        return Err("invalid_csv: CSV quote is not closed".to_string());
                    }
                    if !self.field.is_empty() || !self.row.is_empty() {
                        self.row.push(std::mem::take(&mut self.field));
                        return Ok(Some(std::mem::take(&mut self.row)));
                    }
                    return Ok(None);
                }
                Ok(_) => {
                    let text = std::str::from_utf8(&bytes)
                        .map_err(|error| format!("invalid_utf8: source is not UTF-8: {error}"))?;
                    let mut chars = text.chars().peekable();
                    while let Some(character) = chars.next() {
                        if let Some(row) = self.push_back(character, &mut chars)? {
                            return Ok(Some(row));
                        }
                    }
                }
                Err(error) => return Err(format!("could not read dataset source: {error}")),
            }
        }
    }

    fn push_back<I>(
        &mut self,
        character: char,
        chars: &mut std::iter::Peekable<I>,
    ) -> Result<Option<Vec<String>>, String>
    where
        I: Iterator<Item = char>,
    {
        if self.quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    self.field.push('"');
                    chars.next();
                } else {
                    self.quoted = false;
                    self.quote_closed = true;
                }
            } else {
                self.field.push(character);
            }
            return Ok(None);
        }
        match character {
            '"' if self.field.is_empty() && !self.quote_closed => self.quoted = true,
            ',' => {
                self.row.push(std::mem::take(&mut self.field));
                self.quote_closed = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => return Ok(Some(self.end_row())),
            value if self.quote_closed && !value.is_whitespace() => {
                return Err("invalid_csv: characters follow a closing CSV quote".to_string());
            }
            value => self.field.push(value),
        }
        Ok(None)
    }

    fn end_row(&mut self) -> Vec<String> {
        self.row.push(std::mem::take(&mut self.field));
        self.quote_closed = false;
        std::mem::take(&mut self.row)
    }
}
