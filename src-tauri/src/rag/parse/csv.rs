use super::{DocumentBlock, ParseError, ParseWarning, ParsedDocument};
use crate::rag::model::SourceLocation;

pub(super) fn parse(source: &str) -> Result<ParsedDocument, ParseError> {
    let rows = rows(source)?;
    let mut document = ParsedDocument::empty();
    if rows.is_empty() {
        return Ok(document);
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if rows.iter().any(|row| row.len() != width) {
        document.warnings.push(ParseWarning {
            code: "ragged_table".to_string(),
            message: "CSV rows have inconsistent column counts".to_string(),
            location: None,
        });
    }
    document.blocks.push(DocumentBlock::Table {
        location: SourceLocation::SheetRange {
            sheet: "CSV".to_string(),
            range: format!("A1:{}{}", column_name(width), rows.len()),
        },
        rows,
    });
    Ok(document)
}

fn rows(source: &str) -> Result<Vec<Vec<String>>, ParseError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = source.chars().peekable();
    let mut quoted = false;
    let mut quote_closed = false;
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                    quote_closed = true;
                }
            } else {
                field.push(character);
            }
            continue;
        }
        match character {
            '"' if field.is_empty() && !quote_closed => quoted = true,
            ',' => {
                row.push(std::mem::take(&mut field));
                quote_closed = false;
            }
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                quote_closed = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            value if quote_closed && !value.is_whitespace() => {
                return Err(ParseError::new(
                    "invalid_csv",
                    "characters follow a closing CSV quote",
                ));
            }
            value => field.push(value),
        }
    }
    if quoted {
        return Err(ParseError::new("invalid_csv", "CSV quote is not closed"));
    }
    if !field.is_empty() || !row.is_empty() || source.ends_with(',') {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

fn column_name(mut width: usize) -> String {
    let mut name = String::new();
    while width > 0 {
        width -= 1;
        name.insert(0, (b'A' + (width % 26) as u8) as char);
        width /= 26;
    }
    name
}
