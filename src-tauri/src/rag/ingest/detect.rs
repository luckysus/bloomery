use super::{IngestError, SourceFormat};
use std::path::Path;

pub(super) fn detect(
    path: &Path,
    prefix: &[u8],
    prefix_is_complete: bool,
) -> Result<SourceFormat, IngestError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| IngestError::new("unsupported_format", "source extension is required"))?;
    let format = SourceFormat::from_extension(&extension).ok_or_else(|| {
        IngestError::new(
            "unsupported_format",
            format!("unsupported source extension: {extension}"),
        )
    })?;

    let is_pdf = prefix.starts_with(b"%PDF-");
    let is_zip = [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"]
        .iter()
        .any(|signature| prefix.starts_with(*signature));
    let signature_matches = match format {
        SourceFormat::Pdf => is_pdf,
        SourceFormat::Docx | SourceFormat::Xlsx => is_zip,
        SourceFormat::Markdown | SourceFormat::Text | SourceFormat::Html | SourceFormat::Csv => {
            !is_pdf
                && !is_zip
                && !prefix.contains(&0)
                && is_valid_utf8_prefix(prefix, prefix_is_complete)
        }
    };

    if signature_matches {
        Ok(format)
    } else {
        Err(IngestError::new(
            "mime_extension_mismatch",
            "source bytes do not match the selected file extension",
        ))
    }
}

fn is_valid_utf8_prefix(bytes: &[u8], complete: bool) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(error) => !complete && error.error_len().is_none(),
    }
}
