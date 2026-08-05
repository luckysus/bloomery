use super::archive::OfficeArchive;
use super::xml;
use super::{DocumentBlock, ParseError, ParseLimits, ParsedDocument};
use crate::rag::model::SourceLocation;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashMap;

pub(super) fn parse(bytes: &[u8], limits: ParseLimits) -> Result<ParsedDocument, ParseError> {
    let mut archive = OfficeArchive::open(bytes, limits)?;
    let workbook = archive.read_required("xl/workbook.xml")?;
    let relationships = archive.read_required("xl/_rels/workbook.xml.rels")?;
    let shared = archive
        .read_optional("xl/sharedStrings.xml")?
        .map(|bytes| parse_shared_strings(&bytes))
        .transpose()?
        .unwrap_or_default();
    let sheets = parse_workbook(&workbook)?;
    let targets = parse_relationships(&relationships)?;
    let mut document = ParsedDocument::empty();
    for (name, relationship_id) in sheets {
        let target = targets.get(&relationship_id).ok_or_else(|| {
            ParseError::new(
                "missing_archive_entry",
                format!("worksheet relationship {relationship_id} is missing"),
            )
        })?;
        let path = worksheet_path(target)?;
        let worksheet = archive.read_required(&path)?;
        parse_worksheet(&worksheet, &name, &shared, &mut document)?;
    }
    Ok(document)
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

struct Cell {
    reference: String,
    kind: Option<String>,
    value: String,
    formula: String,
}

fn parse_worksheet(
    bytes: &[u8],
    sheet: &str,
    shared: &[String],
    document: &mut ParsedDocument,
) -> Result<(), ParseError> {
    let mut reader = Reader::from_reader(bytes);
    let mut current: Option<Cell> = None;
    let mut in_value = false;
    let mut in_formula = false;
    let mut cells = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| ParseError::new("invalid_xlsx_xml", error.to_string()))?
        {
            Event::Start(element) if element.local_name().as_ref() == b"c" => {
                current = Some(start_cell(&reader, &element)?);
            }
            Event::Start(element) if element.local_name().as_ref() == b"v" => in_value = true,
            Event::Start(element) if element.local_name().as_ref() == b"f" => in_formula = true,
            Event::Start(element) if element.local_name().as_ref() == b"t" => in_value = true,
            Event::Text(event) if in_formula => {
                if let Some(cell) = &mut current {
                    cell.formula.push_str(&xml::text(&event)?);
                }
            }
            Event::Text(event) if in_value => {
                if let Some(cell) = &mut current {
                    cell.value.push_str(&xml::text(&event)?);
                }
            }
            Event::End(element) if element.local_name().as_ref() == b"v" => in_value = false,
            Event::End(element) if element.local_name().as_ref() == b"t" => in_value = false,
            Event::End(element) if element.local_name().as_ref() == b"f" => in_formula = false,
            Event::End(element) if element.local_name().as_ref() == b"c" => {
                if let Some(cell) = current.take() {
                    cells.push(cell);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    emit_sheet(sheet, shared, cells, document)
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

fn emit_sheet(
    sheet: &str,
    shared: &[String],
    cells: Vec<Cell>,
    document: &mut ParsedDocument,
) -> Result<(), ParseError> {
    if cells.is_empty() {
        return Ok(());
    }
    let coordinates = cells
        .iter()
        .map(|cell| cell_coordinates(&cell.reference))
        .collect::<Result<Vec<_>, _>>()?;
    let max_column = coordinates
        .iter()
        .map(|(column, _)| *column)
        .max()
        .unwrap_or(1);
    let max_row = coordinates.iter().map(|(_, row)| *row).max().unwrap_or(1);
    let mut rows = vec![vec![String::new(); max_column]; max_row];
    let mut formulas = Vec::new();
    for (cell, (column, row)) in cells.into_iter().zip(coordinates) {
        let value = if !cell.formula.is_empty() {
            formulas.push((cell.formula.clone(), cell.reference.clone()));
            format!("={}", cell.formula)
        } else if cell.kind.as_deref() == Some("s") {
            let index = cell.value.parse::<usize>().map_err(|error| {
                ParseError::new(
                    "invalid_xlsx",
                    format!("invalid shared string index: {error}"),
                )
            })?;
            shared.get(index).cloned().ok_or_else(|| {
                ParseError::new("invalid_xlsx", "shared string index is out of range")
            })?
        } else {
            cell.value
        };
        rows[row - 1][column - 1] = value;
    }
    document.blocks.push(DocumentBlock::Table {
        rows,
        location: SourceLocation::SheetRange {
            sheet: sheet.to_string(),
            range: format!("A1:{}{max_row}", column_name(max_column)),
        },
    });
    for (formula, reference) in formulas {
        document.blocks.push(DocumentBlock::Formula {
            text: formula,
            location: SourceLocation::SheetRange {
                sheet: sheet.to_string(),
                range: reference,
            },
        });
    }
    Ok(())
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

fn column_name(mut column: usize) -> String {
    let mut name = String::new();
    while column > 0 {
        column -= 1;
        name.insert(0, (b'A' + (column % 26) as u8) as char);
        column /= 26;
    }
    name
}
