use super::{offsets, DocumentBlock, ParseError, ParsedDocument};

pub(super) fn parse(source: &str) -> Result<ParsedDocument, ParseError> {
    let mut document = ParsedDocument::empty();
    let mut paragraph_start = None;
    let mut paragraph_end = 0usize;
    let mut lines = Vec::new();
    let mut offset = 0usize;

    for segment in source.split_inclusive('\n') {
        let text = segment.trim_end_matches(['\r', '\n']);
        let end = offset + text.len();
        if text.trim().is_empty() {
            finish_paragraph(
                &mut document,
                &mut lines,
                &mut paragraph_start,
                paragraph_end,
            );
        } else {
            paragraph_start.get_or_insert(offset);
            paragraph_end = end;
            lines.push(text.trim());
        }
        offset += segment.len();
    }
    if !source.ends_with('\n') || paragraph_start.is_some() {
        finish_paragraph(
            &mut document,
            &mut lines,
            &mut paragraph_start,
            paragraph_end,
        );
    }
    Ok(document)
}

fn finish_paragraph(
    document: &mut ParsedDocument,
    lines: &mut Vec<&str>,
    start: &mut Option<usize>,
    end: usize,
) {
    if let Some(start_offset) = start.take() {
        document.blocks.push(DocumentBlock::Paragraph {
            text: lines.join("\n"),
            location: offsets(start_offset, end),
        });
        lines.clear();
    }
}
