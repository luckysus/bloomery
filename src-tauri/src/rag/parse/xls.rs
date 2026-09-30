use super::{DocumentBlock, ParseError, ParsedDocument};
use crate::rag::model::SourceLocation;
use calamine::{Reader, Xls};
use std::io::Cursor;

pub(super) fn parse(bytes: &[u8]) -> Result<ParsedDocument, ParseError> {
    let mut workbook = Xls::new(Cursor::new(bytes))
        .map_err(|error| ParseError::new("invalid_xls", error.to_string()))?;
    let mut document = ParsedDocument::empty();
    for sheet in workbook.sheet_names() {
        let range = workbook
            .worksheet_range(&sheet)
            .map_err(|error| ParseError::new("invalid_xls", error.to_string()))?;
        let Some((start_row, start_column)) = range.start() else {
            continue;
        };
        let Some((end_row, end_column)) = range.end() else {
            continue;
        };
        let rows = range
            .rows()
            .map(|row| row.iter().map(ToString::to_string).collect::<Vec<_>>())
            .collect();
        document.blocks.push(DocumentBlock::Table {
            rows,
            location: SourceLocation::SheetRange {
                sheet,
                range: format!(
                    "R{}C{}:R{}C{}",
                    start_row + 1,
                    start_column + 1,
                    end_row + 1,
                    end_column + 1
                ),
            },
        });
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_xls_bytes() {
        let error = parse(b"not an xls workbook").unwrap_err();
        assert_eq!(error.code(), "invalid_xls");
    }
}
