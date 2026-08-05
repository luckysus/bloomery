use super::{
    collapse_whitespace, DocumentBlock, ParseError, ParseLimits, ParseWarning, ParsedDocument,
};
use crate::rag::model::SourceLocation;
use flate2::read::ZlibDecoder;
use std::io::Read;

pub(super) fn parse(bytes: &[u8], limits: ParseLimits) -> Result<ParsedDocument, ParseError> {
    if !bytes.starts_with(b"%PDF-") {
        return Err(ParseError::new("invalid_pdf", "PDF signature is missing"));
    }
    let mut document = ParsedDocument::empty();
    for (index, stream) in streams(bytes, limits.max_expanded_bytes)?
        .into_iter()
        .enumerate()
    {
        let text = collapse_whitespace(&extract_literal_text(&stream));
        if !text.is_empty() {
            document.blocks.push(DocumentBlock::Paragraph {
                text,
                location: SourceLocation::PdfPage {
                    page: index as u32 + 1,
                    bbox: None,
                },
            });
        }
    }
    document.warnings.push(ParseWarning {
        code: "pdf_text_layer_limited".to_string(),
        message: "Local PDF parsing reads basic text-show operators only; custom fonts, layout, tables, formulas, and images may be incomplete. Configure MinerU for structured parsing."
            .to_string(),
        location: None,
    });
    if document.blocks.is_empty() {
        document.warnings.push(ParseWarning {
            code: "pdf_text_layer_missing".to_string(),
            message: "No usable local PDF text layer was found".to_string(),
            location: None,
        });
    }
    Ok(document)
}

fn streams(bytes: &[u8], max_expanded_bytes: u64) -> Result<Vec<Vec<u8>>, ParseError> {
    let mut streams = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative_start) = find(&bytes[cursor..], b"stream") {
        let keyword = cursor + relative_start;
        let mut start = keyword + b"stream".len();
        if bytes.get(start..start + 2) == Some(b"\r\n") {
            start += 2;
        } else if matches!(bytes.get(start), Some(b'\n' | b'\r')) {
            start += 1;
        } else {
            cursor = start;
            continue;
        }
        let Some(relative_end) = find(&bytes[start..], b"endstream") else {
            return Err(ParseError::new(
                "invalid_pdf",
                "PDF stream is not terminated",
            ));
        };
        let end = start + relative_end;
        let mut content = bytes[start..end]
            .strip_suffix(b"\r\n")
            .or_else(|| bytes[start..end].strip_suffix(b"\n"))
            .unwrap_or(&bytes[start..end])
            .to_vec();
        let dictionary_start = keyword.saturating_sub(512);
        if find(&bytes[dictionary_start..keyword], b"/FlateDecode").is_some() {
            let mut decoder = ZlibDecoder::new(content.as_slice());
            let mut expanded = Vec::new();
            decoder
                .by_ref()
                .take(max_expanded_bytes.saturating_add(1))
                .read_to_end(&mut expanded)
                .map_err(|error| ParseError::new("invalid_pdf_stream", error.to_string()))?;
            if expanded.len() as u64 > max_expanded_bytes {
                return Err(ParseError::new(
                    "pdf_stream_too_large",
                    "expanded PDF stream exceeds the limit",
                ));
            }
            content = expanded;
        }
        streams.push(content);
        cursor = end + b"endstream".len();
    }
    Ok(streams)
}

fn extract_literal_text(stream: &[u8]) -> String {
    let mut values = Vec::new();
    let mut index = 0usize;
    while index < stream.len() {
        if stream[index] != b'(' {
            index += 1;
            continue;
        }
        let (value, end) = literal_string(stream, index + 1);
        index = end;
        let mut operator = index;
        while matches!(
            stream.get(operator),
            Some(b' ' | b'\r' | b'\n' | b'\t' | b']')
        ) {
            operator += 1;
        }
        if stream.get(operator..operator + 2) == Some(b"Tj")
            || stream.get(operator..operator + 2) == Some(b"TJ")
            || stream[index..].iter().take(64).any(|byte| *byte == b']')
        {
            values.push(String::from_utf8_lossy(&value).into_owned());
        }
    }
    values.join(" ")
}

fn literal_string(bytes: &[u8], mut index: usize) -> (Vec<u8>, usize) {
    let mut value = Vec::new();
    let mut depth = 1usize;
    while index < bytes.len() && depth > 0 {
        match bytes[index] {
            b'\\' => {
                index += 1;
                if index >= bytes.len() {
                    break;
                }
                match bytes[index] {
                    b'n' => value.push(b'\n'),
                    b'r' => value.push(b'\r'),
                    b't' => value.push(b'\t'),
                    b'b' => value.push(8),
                    b'f' => value.push(12),
                    b'\r' | b'\n' => {}
                    byte => value.push(byte),
                }
            }
            b'(' => {
                depth += 1;
                value.push(b'(');
            }
            b')' => {
                depth -= 1;
                if depth > 0 {
                    value.push(b')');
                }
            }
            byte => value.push(byte),
        }
        index += 1;
    }
    (value, index)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
