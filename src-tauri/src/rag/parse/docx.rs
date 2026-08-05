use super::archive::OfficeArchive;
use super::xml;
use super::{
    collapse_whitespace, DocumentBlock, ParseError, ParseLimits, ParseWarning, ParsedAsset,
    ParsedDocument,
};
use crate::rag::model::SourceLocation;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

pub(super) fn parse(bytes: &[u8], limits: ParseLimits) -> Result<ParsedDocument, ParseError> {
    let mut archive = OfficeArchive::open(bytes, limits)?;
    let document_xml = archive.read_required("word/document.xml")?;
    let media = archive.read_prefix("word/media/")?;
    let mut document = ParsedDocument::empty();
    for (name, bytes) in media {
        document.assets.push(ParsedAsset {
            kind: "image".to_string(),
            media_type: media_type(&name).to_string(),
            original_name: name.rsplit('/').next().map(str::to_string),
            bytes,
            location: None,
        });
    }
    parse_xml(&document_xml, &mut document)?;
    Ok(document)
}

struct State {
    heading_path: Vec<String>,
    paragraph: String,
    paragraph_level: Option<u8>,
    paragraph_numbered: bool,
    paragraph_drawing: bool,
    in_paragraph: bool,
    in_table: bool,
    in_cell: bool,
    cell: String,
    row: Vec<String>,
    rows: Vec<Vec<String>>,
    in_math: bool,
    formula: String,
    pending_list: Vec<String>,
    image_index: usize,
}

fn parse_xml(xml_bytes: &[u8], document: &mut ParsedDocument) -> Result<(), ParseError> {
    let mut reader = Reader::from_reader(xml_bytes);
    reader.config_mut().trim_text(false);
    let mut state = State {
        heading_path: Vec::new(),
        paragraph: String::new(),
        paragraph_level: None,
        paragraph_numbered: false,
        paragraph_drawing: false,
        in_paragraph: false,
        in_table: false,
        in_cell: false,
        cell: String::new(),
        row: Vec::new(),
        rows: Vec::new(),
        in_math: false,
        formula: String::new(),
        pending_list: Vec::new(),
        image_index: 0,
    };
    loop {
        match reader
            .read_event()
            .map_err(|error| ParseError::new("invalid_docx_xml", error.to_string()))?
        {
            Event::Start(element) => start(&reader, &element, &mut state, document)?,
            Event::Empty(element) => start(&reader, &element, &mut state, document)?,
            Event::Text(event) => {
                let text = xml::text(&event)?;
                if state.in_math {
                    state.formula.push_str(&text);
                } else if state.in_cell {
                    state.cell.push_str(&text);
                } else if state.in_paragraph {
                    state.paragraph.push_str(&text);
                }
            }
            Event::End(element) => end(element.local_name().as_ref(), &mut state, document)?,
            Event::Eof => break,
            _ => {}
        }
    }
    flush_list(&mut state, document);
    Ok(())
}

fn start(
    reader: &Reader<&[u8]>,
    element: &BytesStart<'_>,
    state: &mut State,
    document: &mut ParsedDocument,
) -> Result<(), ParseError> {
    match element.local_name().as_ref() {
        b"p" if !state.in_table => {
            state.in_paragraph = true;
            state.paragraph.clear();
            state.paragraph_level = None;
            state.paragraph_numbered = false;
            state.paragraph_drawing = false;
        }
        b"pStyle" if state.in_paragraph => {
            if let Some(style) = xml::attribute(reader, element, b"val")? {
                state.paragraph_level = style
                    .strip_prefix("Heading")
                    .or_else(|| style.strip_prefix("heading"))
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|level| (1..=9).contains(level));
            }
        }
        b"numPr" if state.in_paragraph => state.paragraph_numbered = true,
        b"drawing" if state.in_paragraph => state.paragraph_drawing = true,
        b"tab" | b"br" if state.in_paragraph => state.paragraph.push(' '),
        b"tbl" => {
            flush_list(state, document);
            state.in_table = true;
            state.rows.clear();
        }
        b"tr" if state.in_table => state.row.clear(),
        b"tc" if state.in_table => {
            state.in_cell = true;
            state.cell.clear();
        }
        b"oMath" => {
            flush_list(state, document);
            state.in_math = true;
            state.formula.clear();
        }
        _ => {}
    }
    Ok(())
}

fn end(name: &[u8], state: &mut State, document: &mut ParsedDocument) -> Result<(), ParseError> {
    match name {
        b"p" if state.in_paragraph && !state.in_table => finish_paragraph(state, document),
        b"tc" if state.in_table => {
            state.in_cell = false;
            state.row.push(collapse_whitespace(&state.cell));
        }
        b"tr" if state.in_table => {
            if !state.row.is_empty() {
                state.rows.push(std::mem::take(&mut state.row));
            }
        }
        b"tbl" if state.in_table => {
            state.in_table = false;
            if !state.rows.is_empty() {
                document.blocks.push(DocumentBlock::Table {
                    rows: std::mem::take(&mut state.rows),
                    location: heading_location(&state.heading_path),
                });
            }
        }
        b"oMath" if state.in_math => {
            state.in_math = false;
            let text = collapse_whitespace(&state.formula);
            if !text.is_empty() {
                document.blocks.push(DocumentBlock::Formula {
                    text,
                    location: heading_location(&state.heading_path),
                });
            }
        }
        _ => {}
    }
    Ok(())
}

fn finish_paragraph(state: &mut State, document: &mut ParsedDocument) {
    state.in_paragraph = false;
    let text = collapse_whitespace(&state.paragraph);
    if state.paragraph_numbered && !text.is_empty() {
        state.pending_list.push(text);
        return;
    }
    flush_list(state, document);
    if let Some(level) = state.paragraph_level {
        if !text.is_empty() {
            let depth = level.saturating_sub(1) as usize;
            state
                .heading_path
                .truncate(depth.min(state.heading_path.len()));
            state.heading_path.push(text.clone());
            document.blocks.push(DocumentBlock::Heading {
                level,
                text,
                location: heading_location(&state.heading_path),
            });
        }
    } else if !text.is_empty() {
        document.blocks.push(DocumentBlock::Paragraph {
            text,
            location: heading_location(&state.heading_path),
        });
    }
    if state.paragraph_drawing {
        if state.image_index < document.assets.len() {
            document.blocks.push(DocumentBlock::Image {
                alt: document.assets[state.image_index]
                    .original_name
                    .clone()
                    .unwrap_or_default(),
                asset_index: Some(state.image_index),
                location: heading_location(&state.heading_path),
            });
            state.image_index += 1;
        } else {
            document.warnings.push(ParseWarning {
                code: "docx_image_missing".to_string(),
                message: "DOCX drawing has no matching embedded media".to_string(),
                location: Some(heading_location(&state.heading_path)),
            });
        }
    }
}

fn flush_list(state: &mut State, document: &mut ParsedDocument) {
    if !state.pending_list.is_empty() {
        document.blocks.push(DocumentBlock::List {
            ordered: true,
            items: std::mem::take(&mut state.pending_list),
            location: heading_location(&state.heading_path),
        });
    }
}

fn heading_location(path: &[String]) -> SourceLocation {
    SourceLocation::Heading {
        path: if path.is_empty() {
            vec!["Document".to_string()]
        } else {
            path.to_vec()
        },
    }
}

fn media_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}
