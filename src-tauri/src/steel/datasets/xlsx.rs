use super::{DatasetTable, MAX_PREVIEW_COLUMNS, MAX_PREVIEW_ROWS};
use crate::rag::parse::archive::OfficeArchive;
use crate::rag::parse::{xml, ParseError, ParseLimits};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_XLSX_ENTRY_BYTES: usize = 64 * 1024 * 1024;

pub(super) fn read_dataset_table(
    path: &Path,
    requested_sheet: Option<&str>,
) -> Result<DatasetTable, String> {
    let limits = ParseLimits::default();
    let mut file =
        File::open(path).map_err(|error| format!("could not read dataset source: {error}"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limits.max_source_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read dataset source: {error}"))?;
    if bytes.len() as u64 > limits.max_source_bytes {
        return Err(format!(
            "parse_source_too_large: parse source exceeds the {}-byte limit",
            limits.max_source_bytes
        ));
    }

    let mut archive = OfficeArchive::open(&bytes, limits).map_err(|error| error.to_string())?;
    let workbook = archive
        .read_required_limited("xl/workbook.xml", MAX_XLSX_ENTRY_BYTES)
        .map_err(|error| error.to_string())?;
    let relationships = archive
        .read_required_limited("xl/_rels/workbook.xml.rels", MAX_XLSX_ENTRY_BYTES)
        .map_err(|error| error.to_string())?;
    let shared = archive
        .read_optional_limited("xl/sharedStrings.xml", MAX_XLSX_ENTRY_BYTES)
        .map_err(|error| error.to_string())?
        .map(|bytes| parse_shared_strings(bytes.as_slice()))
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let sheets = parse_workbook(&workbook).map_err(|error| error.to_string())?;
    if sheets.is_empty() {
        return Err("dataset does not contain a table".to_string());
    }
    let sheet_names = sheets
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let targets = parse_relationships(&relationships).map_err(|error| error.to_string())?;
    let (selected_sheet, parsed) =
        select_sheet(&mut archive, &sheets, &targets, &shared, requested_sheet)
            .map_err(|error| error.to_string())?;
    let sheet_names = if requested_sheet.is_none() {
        vec![selected_sheet.clone()]
    } else {
        sheet_names
    };
    validate_width(parsed.width)?;

    let mut headers = if parsed.header.is_empty() {
        (0..parsed.width)
            .map(|index| format!("column_{}", index + 1))
            .collect::<Vec<_>>()
    } else {
        parsed
            .header
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let value = value.trim().to_string();
                if value.is_empty() {
                    format!("column_{}", index + 1)
                } else {
                    value
                }
            })
            .collect::<Vec<_>>()
    };
    if headers.len() < parsed.width {
        headers.extend((headers.len()..parsed.width).map(|index| format!("column_{}", index + 1)));
    }

    let truncated = parsed.row_count > parsed.rows.len();
    let mut warnings = Vec::new();
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
        format: "xlsx".to_string(),
        sheets: sheet_names,
        selected_sheet,
        headers,
        rows: parsed.rows,
        row_count: parsed.row_count,
        truncated,
        warnings,
    })
}

fn select_sheet(
    archive: &mut OfficeArchive<'_>,
    sheets: &[(String, String)],
    targets: &HashMap<String, String>,
    shared: &[String],
    requested_sheet: Option<&str>,
) -> Result<(String, ParsedWorksheet), ParseError> {
    let mut found_empty = false;
    for (name, relationship_id) in sheets {
        if requested_sheet.is_some_and(|requested| requested != name) {
            continue;
        }
        let target = targets.get(relationship_id).ok_or_else(|| {
            ParseError::new(
                "missing_archive_entry",
                format!("worksheet relationship {relationship_id} is missing"),
            )
        })?;
        let worksheet =
            archive.read_required_limited(&worksheet_path(target)?, MAX_XLSX_ENTRY_BYTES)?;
        let parsed = parse_worksheet(&worksheet, shared)?;
        if parsed.width > 0 {
            return Ok((name.clone(), parsed));
        }
        found_empty = true;
        if requested_sheet.is_some() {
            break;
        }
    }
    if requested_sheet.is_some() && !found_empty {
        return Err(ParseError::new(
            "invalid_xlsx",
            "requested dataset sheet was not found",
        ));
    }
    Err(ParseError::new(
        "invalid_xlsx",
        "dataset does not contain a table",
    ))
}

struct ParsedWorksheet {
    header: Vec<String>,
    rows: Vec<Vec<String>>,
    width: usize,
    row_count: usize,
}

struct Cell {
    reference: String,
    kind: Option<String>,
    value: String,
    formula: String,
}

fn parse_workbook(bytes: &[u8]) -> Result<Vec<(String, String)>, ParseError> {
    let mut reader = Reader::from_reader(bytes);
    let mut sheets = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| ParseError::new("invalid_xlsx_xml", error.to_string()))?
        {
            Event::Start(element) | Event::Empty(element)
                if element.local_name().as_ref() == b"sheet" =>
            {
                let name = xml::attribute(&reader, &element, b"name")?
                    .ok_or_else(|| ParseError::new("invalid_xlsx", "sheet name is missing"))?;
                let id = xml::attribute(&reader, &element, b"id")?.ok_or_else(|| {
                    ParseError::new("invalid_xlsx", "sheet relationship ID is missing")
                })?;
                sheets.push((name, id));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(sheets)
}

fn parse_relationships(bytes: &[u8]) -> Result<HashMap<String, String>, ParseError> {
    let mut reader = Reader::from_reader(bytes);
    let mut relationships = HashMap::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| ParseError::new("invalid_xlsx_xml", error.to_string()))?
        {
            Event::Start(element) | Event::Empty(element)
                if element.local_name().as_ref() == b"Relationship" =>
            {
                let id = xml::attribute(&reader, &element, b"Id")?
                    .ok_or_else(|| ParseError::new("invalid_xlsx", "relationship ID is missing"))?;
                let target = xml::attribute(&reader, &element, b"Target")?.ok_or_else(|| {
                    ParseError::new("invalid_xlsx", "relationship target is missing")
                })?;
                relationships.insert(id, target);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(relationships)
}

fn parse_shared_strings(bytes: &[u8]) -> Result<Vec<String>, ParseError> {
    let mut reader = Reader::from_reader(bytes);
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_item = false;
    let mut in_text = false;
    loop {
        match reader
            .read_event()
            .map_err(|error| ParseError::new("invalid_xlsx_xml", error.to_string()))?
        {
            Event::Start(element) if element.local_name().as_ref() == b"si" => {
                in_item = true;
                current.clear();
            }
            Event::Start(element) if in_item && element.local_name().as_ref() == b"t" => {
                in_text = true
            }
            Event::Text(event) if in_text => current.push_str(&xml::text(&event)?),
            Event::End(element) if element.local_name().as_ref() == b"t" => in_text = false,
            Event::End(element) if element.local_name().as_ref() == b"si" => {
                in_item = false;
                strings.push(std::mem::take(&mut current));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(strings)
}

fn parse_worksheet(bytes: &[u8], shared: &[String]) -> Result<ParsedWorksheet, ParseError> {
    let mut reader = Reader::from_reader(bytes);
    let mut current_cell: Option<Cell> = None;
    let mut in_value = false;
    let mut in_formula = false;
    let mut header = Vec::new();
    let mut rows = Vec::new();
    let mut width = 0usize;
    let mut max_row = 0usize;

    loop {
        match reader
            .read_event()
            .map_err(|error| ParseError::new("invalid_xlsx_xml", error.to_string()))?
        {
            Event::Start(element) if element.local_name().as_ref() == b"c" => {
                current_cell = Some(start_cell(&reader, &element)?);
            }
            Event::Start(element) if element.local_name().as_ref() == b"v" => in_value = true,
            Event::Start(element) if element.local_name().as_ref() == b"f" => in_formula = true,
            Event::Start(element) if element.local_name().as_ref() == b"t" => in_value = true,
            Event::Text(event) if in_formula => {
                if let Some(cell) = &mut current_cell {
                    cell.formula.push_str(&xml::text(&event)?);
                }
            }
            Event::Text(event) if in_value => {
                if let Some(cell) = &mut current_cell {
                    cell.value.push_str(&xml::text(&event)?);
                }
            }
            Event::End(element) if element.local_name().as_ref() == b"v" => in_value = false,
            Event::End(element) if element.local_name().as_ref() == b"t" => in_value = false,
            Event::End(element) if element.local_name().as_ref() == b"f" => in_formula = false,
            Event::End(element) if element.local_name().as_ref() == b"c" => {
                if let Some(cell) = current_cell.take() {
                    let (column, row) = cell_coordinates(&cell.reference)?;
                    validate_width(column)
                        .map_err(|message| ParseError::new("invalid_xlsx", message))?;
                    width = width.max(column);
                    max_row = max_row.max(row);
                    if row == 1 {
                        let value = cell_value(cell, shared)?;
                        set_cell(&mut header, column, value);
                    } else if row <= MAX_PREVIEW_ROWS + 1 {
                        let value = cell_value(cell, shared)?;
                        rows.resize_with(row - 1, Vec::new);
                        set_cell(&mut rows[row - 2], column, value);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    header.resize(width, String::new());
    let row_count = max_row.saturating_sub(1);
    rows.resize_with(row_count.min(MAX_PREVIEW_ROWS), Vec::new);
    for row in &mut rows {
        row.resize(width, String::new());
    }
    Ok(ParsedWorksheet {
        header,
        rows,
        width,
        row_count,
    })
}

fn start_cell(reader: &Reader<&[u8]>, element: &BytesStart<'_>) -> Result<Cell, ParseError> {
    Ok(Cell {
        reference: xml::attribute(reader, element, b"r")?
            .ok_or_else(|| ParseError::new("invalid_xlsx", "cell reference is missing"))?,
        kind: xml::attribute(reader, element, b"t")?,
        value: String::new(),
        formula: String::new(),
    })
}

fn cell_value(cell: Cell, shared: &[String]) -> Result<String, ParseError> {
    if !cell.formula.is_empty() {
        return Ok(format!("={}", cell.formula));
    }
    if cell.kind.as_deref() == Some("s") {
        let index = cell.value.parse::<usize>().map_err(|error| {
            ParseError::new(
                "invalid_xlsx",
                format!("invalid shared string index: {error}"),
            )
        })?;
        return shared
            .get(index)
            .cloned()
            .ok_or_else(|| ParseError::new("invalid_xlsx", "shared string index is out of range"));
    }
    Ok(cell.value)
}

fn set_cell(row: &mut Vec<String>, column: usize, value: String) {
    row.resize(column, String::new());
    row[column - 1] = value;
}

fn worksheet_path(target: &str) -> Result<String, ParseError> {
    let target = target.trim_start_matches('/');
    if target.contains("..") || target.contains('\\') || target.contains(':') {
        return Err(ParseError::new(
            "archive_path_traversal",
            "worksheet relationship contains an unsafe path",
        ));
    }
    Ok(if target.starts_with("xl/") {
        target.to_string()
    } else {
        format!("xl/{target}")
    })
}

fn cell_coordinates(reference: &str) -> Result<(usize, usize), ParseError> {
    let mut column = 0usize;
    let mut split = 0usize;
    for (index, byte) in reference.bytes().enumerate() {
        if byte.is_ascii_alphabetic() {
            column = column
                .checked_mul(26)
                .and_then(|value| {
                    value.checked_add((byte.to_ascii_uppercase() - b'A' + 1) as usize)
                })
                .ok_or_else(|| ParseError::new("invalid_xlsx", "cell column overflows"))?;
            split = index + 1;
        } else {
            break;
        }
    }
    let row = reference[split..]
        .parse::<usize>()
        .map_err(|error| ParseError::new("invalid_xlsx", format!("invalid cell row: {error}")))?;
    if column == 0 || row == 0 {
        return Err(ParseError::new(
            "invalid_xlsx",
            "cell reference must be positive",
        ));
    }
    Ok((column, row))
}

fn validate_width(width: usize) -> Result<(), String> {
    if width > MAX_PREVIEW_COLUMNS {
        return Err(format!(
            "dataset has more than {MAX_PREVIEW_COLUMNS} columns"
        ));
    }
    Ok(())
}
