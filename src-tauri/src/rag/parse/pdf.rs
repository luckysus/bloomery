use super::{DocumentBlock, ParseError, ParseLimits, ParseWarning, ParsedDocument};
use crate::rag::model::SourceLocation;

pub(super) fn parse(bytes: &[u8], limits: ParseLimits) -> Result<ParsedDocument, ParseError> {
    if !bytes.starts_with(b"%PDF-") {
        return Err(ParseError::new("invalid_pdf", "PDF signature is missing"));
    }
    if bytes.len() as u64 > limits.max_source_bytes {
        return Err(ParseError::new(
            "parse_source_too_large",
            "PDF exceeds the source size limit",
        ));
    }
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
        .map_err(|error| ParseError::new("pdf_parse_failed", error.to_string()))?;
    let mut document = ParsedDocument::empty();
    for (index, text) in pages.into_iter().enumerate() {
        let text = text.trim();
        if !text.is_empty() {
            document.blocks.push(DocumentBlock::Paragraph {
                text: text.to_string(),
                location: SourceLocation::PdfPage {
                    page: index as u32 + 1,
                    bbox: None,
                },
            });
        }
    }
    document.warnings.push(ParseWarning {
        code: "pdf_layout_limited".to_string(),
        message: "Local PDF preview preserves page numbers and text but not full visual layout. Configure MinerU for structured parsing."
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
