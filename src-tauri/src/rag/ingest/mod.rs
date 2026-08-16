mod detect;
mod hash;
mod import;

pub use import::{
    queue_document_import, DocumentImportRequest, DocumentImportResponse, KnowledgeBaseTarget,
};

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Pdf,
    Markdown,
    Text,
    Html,
    Docx,
    Csv,
    Xlsx,
}

impl SourceFormat {
    fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "pdf" => Some(Self::Pdf),
            "md" | "markdown" => Some(Self::Markdown),
            "txt" => Some(Self::Text),
            "html" | "htm" => Some(Self::Html),
            "docx" => Some(Self::Docx),
            "csv" => Some(Self::Csv),
            "xlsx" => Some(Self::Xlsx),
            _ => None,
        }
    }

    pub(crate) fn from_mime_type(mime_type: &str) -> Option<Self> {
        match mime_type {
            "application/pdf" => Some(Self::Pdf),
            "text/markdown" => Some(Self::Markdown),
            "text/plain" => Some(Self::Text),
            "text/html" => Some(Self::Html),
            "text/csv" => Some(Self::Csv),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Some(Self::Docx)
            }
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some(Self::Xlsx),
            _ => None,
        }
    }

    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Markdown => "text/markdown",
            Self::Text => "text/plain",
            Self::Html => "text/html",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Csv => "text/csv",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Markdown => "markdown",
            Self::Text => "text",
            Self::Html => "html",
            Self::Docx => "docx",
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IngestLimits {
    pub max_bytes: u64,
}

impl Default for IngestLimits {
    fn default() -> Self {
        Self {
            max_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestedSource {
    pub content_sha256: String,
    pub byte_len: u64,
    pub format: SourceFormat,
    pub mime_type: String,
    pub storage_key: String,
    pub stored_path: PathBuf,
    pub duplicate: bool,
}

#[derive(Debug)]
pub struct IngestError {
    code: &'static str,
    message: String,
}

impl IngestError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn io(code: &'static str, action: &str, error: io::Error) -> Self {
        Self::new(code, format!("{action}: {error}"))
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for IngestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for IngestError {}

struct StagedFile(PathBuf);

impl Drop for StagedFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn ingest_file(
    source: &Path,
    content_root: &Path,
    limits: IngestLimits,
) -> Result<IngestedSource, IngestError> {
    if limits.max_bytes == 0 {
        return Err(IngestError::new(
            "invalid_ingest_limit",
            "maximum source size must be positive",
        ));
    }
    let (authorized_source, source_file) =
        crate::permissions::path::authorize_existing_file_with_handle(source)
            .map_err(|error| IngestError::new("path_not_authorized", error.to_string()))?;
    let source = authorized_source.canonical_path();
    let staging_directory = content_root.join(".staging");
    fs::create_dir_all(&staging_directory)
        .map_err(|error| IngestError::io("storage_io", "create staging directory", error))?;
    let staged = StagedFile(staging_directory.join(Uuid::new_v4().to_string()));
    let digest = hash::stage_and_hash(source_file, &staged.0, limits.max_bytes)?;
    let format = detect::detect(
        source,
        &digest.prefix,
        digest.byte_len == digest.prefix.len() as u64,
    )?;
    let storage_key = format!(
        "objects/sha256/{}/{}",
        &digest.content_sha256[..2],
        digest.content_sha256
    );
    let stored_path = content_root.join(Path::new(&storage_key));
    fs::create_dir_all(
        stored_path
            .parent()
            .expect("content-addressed path always has a parent"),
    )
    .map_err(|error| IngestError::io("storage_io", "create object directory", error))?;

    let duplicate = if stored_path.exists() {
        hash::verify_object(&stored_path, digest.byte_len, &digest.content_sha256)?;
        true
    } else {
        match fs::rename(&staged.0, &stored_path) {
            Ok(()) => false,
            Err(_) if stored_path.exists() => {
                hash::verify_object(&stored_path, digest.byte_len, &digest.content_sha256)?;
                true
            }
            Err(error) => {
                return Err(IngestError::io(
                    "storage_io",
                    "persist content-addressed object",
                    error,
                ));
            }
        }
    };

    Ok(IngestedSource {
        content_sha256: digest.content_sha256,
        byte_len: digest.byte_len,
        format,
        mime_type: format.mime_type().to_string(),
        storage_key,
        stored_path,
        duplicate,
    })
}
