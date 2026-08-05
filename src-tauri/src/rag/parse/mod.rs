mod archive;
mod csv;
mod docx;
mod html;
mod markdown;
mod mineru;
mod pdf;
mod text;
mod xlsx;
mod xml;

use crate::rag::ingest::SourceFormat;
use crate::rag::model::SourceLocation;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub blocks: Vec<DocumentBlock>,
    pub assets: Vec<ParsedAsset>,
    pub warnings: Vec<ParseWarning>,
}

impl ParsedDocument {
    fn empty() -> Self {
        Self {
            blocks: Vec::new(),
            assets: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DocumentBlock {
    Heading {
        level: u8,
        text: String,
        location: SourceLocation,
    },
    Paragraph {
        text: String,
        location: SourceLocation,
    },
    List {
        ordered: bool,
        items: Vec<String>,
        location: SourceLocation,
    },
    Table {
        rows: Vec<Vec<String>>,
        location: SourceLocation,
    },
    Formula {
        text: String,
        location: SourceLocation,
    },
    Image {
        alt: String,
        asset_index: Option<usize>,
        location: SourceLocation,
    },
}

impl DocumentBlock {
    pub const fn location(&self) -> &SourceLocation {
        match self {
            Self::Heading { location, .. }
            | Self::Paragraph { location, .. }
            | Self::List { location, .. }
            | Self::Table { location, .. }
            | Self::Formula { location, .. }
            | Self::Image { location, .. } => location,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedAsset {
    pub kind: String,
    pub media_type: String,
    pub original_name: Option<String>,
    pub bytes: Vec<u8>,
    pub location: Option<SourceLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseWarning {
    pub code: String,
    pub message: String,
    pub location: Option<SourceLocation>,
}

#[derive(Debug, Clone, Copy)]
pub struct ParseLimits {
    pub max_source_bytes: u64,
    pub max_archive_entries: usize,
    pub max_expanded_bytes: u64,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 512 * 1024 * 1024,
            max_archive_entries: 4096,
            max_expanded_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct ParseError {
    code: &'static str,
    message: String,
}

impl ParseError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn io(action: &str, error: io::Error) -> Self {
        Self::new("parse_source_unavailable", format!("{action}: {error}"))
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ParseError {}

pub fn parse_document(
    path: &Path,
    format: SourceFormat,
    limits: ParseLimits,
) -> Result<ParsedDocument, ParseError> {
    let bytes = read_bounded(path, limits.max_source_bytes)?;
    match format {
        SourceFormat::Markdown => markdown::parse(decode_utf8(&bytes)?),
        SourceFormat::Text => text::parse(decode_utf8(&bytes)?),
        SourceFormat::Html => html::parse(decode_utf8(&bytes)?),
        SourceFormat::Csv => csv::parse(decode_utf8(&bytes)?),
        SourceFormat::Pdf => pdf::parse(&bytes, limits),
        SourceFormat::Docx => docx::parse(&bytes, limits),
        SourceFormat::Xlsx => xlsx::parse(&bytes, limits),
    }
}

pub use mineru::parse as parse_mineru_artifact;

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>, ParseError> {
    if max_bytes == 0 {
        return Err(ParseError::new(
            "invalid_parse_limit",
            "maximum source size must be positive",
        ));
    }
    let mut file = File::open(path).map_err(|error| ParseError::io("open parse source", error))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| ParseError::io("read parse source", error))?;
    if bytes.len() as u64 > max_bytes {
        return Err(ParseError::new(
            "parse_source_too_large",
            format!("parse source exceeds the {max_bytes}-byte limit"),
        ));
    }
    Ok(bytes)
}

fn decode_utf8(bytes: &[u8]) -> Result<&str, ParseError> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    std::str::from_utf8(bytes)
        .map_err(|error| ParseError::new("invalid_utf8", format!("source is not UTF-8: {error}")))
}

pub(crate) fn offsets(start: usize, end: usize) -> SourceLocation {
    SourceLocation::TextOffsets {
        start: start as u64,
        end: end as u64,
    }
}

pub(crate) fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
