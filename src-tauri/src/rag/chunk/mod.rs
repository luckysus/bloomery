mod policy;
mod table;

pub use policy::ChunkPolicy;

use crate::rag::model::{ChunkId, SourceLocation};
use crate::rag::parse::{DocumentBlock, ParsedDocument};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: ChunkId,
    pub ordinal: u32,
    pub text: String,
    pub source_location: SourceLocation,
    pub content_sha256: String,
    pub token_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkError {
    code: &'static str,
    message: String,
}

impl ChunkError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for ChunkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ChunkError {}

pub fn chunk_document(
    document: &ParsedDocument,
    policy: &ChunkPolicy,
) -> Result<Vec<DocumentChunk>, ChunkError> {
    policy.validate()?;
    let mut heading_path = Vec::new();
    let mut pending = Vec::new();
    for (block_index, block) in document.blocks.iter().enumerate() {
        if let DocumentBlock::Heading { level, text, .. } = block {
            update_heading_path(&mut heading_path, *level, text);
            continue;
        }
        let location = block.location().clone();
        let context = bounded_context(&heading_path, policy.max_tokens);
        let context_tokens = count_tokens(&context);
        let body_target = policy.target_tokens.saturating_sub(context_tokens).max(1);
        let body_max = policy.max_tokens.saturating_sub(context_tokens).max(1);
        let texts = match block {
            DocumentBlock::Paragraph { text, .. } => {
                split_block(text, body_target, body_max, policy)
            }
            DocumentBlock::List { ordered, items, .. } => {
                let text = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        if *ordered {
                            format!("{}. {item}", index + 1)
                        } else {
                            format!("- {item}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                split_block(&text, body_target, body_max, policy)
            }
            DocumentBlock::Table { rows, .. } => {
                table::windows(rows, policy.table_header_rows, body_target, body_max)
            }
            DocumentBlock::Formula { text, .. } => split_block(text, body_target, body_max, policy)
                .into_iter()
                .map(|part| format!("$${part}$$"))
                .collect(),
            DocumentBlock::Image { alt, .. } => {
                let text = if alt.trim().is_empty() {
                    "[Image]".to_string()
                } else {
                    format!("[Image: {}]", alt.trim())
                };
                split_block(&text, body_target, body_max, policy)
            }
            DocumentBlock::Heading { .. } => unreachable!(),
        };
        for (slice_index, body) in texts.into_iter().enumerate() {
            if body.trim().is_empty() {
                continue;
            }
            let text = if context.is_empty() {
                body
            } else {
                format!("{context}\n\n{body}")
            };
            pending.push((block_index, slice_index, text, location.clone()));
        }
    }

    pending
        .into_iter()
        .enumerate()
        .map(
            |(ordinal, (block_index, slice_index, text, source_location))| {
                build_chunk(
                    ordinal,
                    block_index,
                    slice_index,
                    text,
                    source_location,
                    policy,
                )
            },
        )
        .collect()
}

fn build_chunk(
    ordinal: usize,
    block_index: usize,
    slice_index: usize,
    text: String,
    source_location: SourceLocation,
    policy: &ChunkPolicy,
) -> Result<DocumentChunk, ChunkError> {
    let content_sha256 = digest(text.as_bytes());
    let location = serde_json::to_vec(&source_location).map_err(|_| {
        ChunkError::new("chunk_encode_failed", "source location is not serializable")
    })?;
    let mut identity = Sha256::new();
    identity.update(policy.version.as_bytes());
    identity.update([0]);
    identity.update(block_index.to_le_bytes());
    identity.update(slice_index.to_le_bytes());
    identity.update(location);
    identity.update(text.as_bytes());
    let id = ChunkId::new(format!("chunk-{:x}", identity.finalize()))
        .map_err(|error| ChunkError::new("chunk_id_invalid", error))?;
    Ok(DocumentChunk {
        id,
        ordinal: u32::try_from(ordinal)
            .map_err(|_| ChunkError::new("too_many_chunks", "document has too many chunks"))?,
        token_count: count_tokens(&text),
        text,
        source_location,
        content_sha256,
    })
}

fn split_block(
    text: &str,
    target_tokens: usize,
    max_tokens: usize,
    policy: &ChunkPolicy,
) -> Vec<String> {
    let limit = target_tokens.min(max_tokens).max(1);
    let overlap = policy.overlap_tokens.min(limit.saturating_sub(1));
    split_text(text, limit, overlap)
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn bounded_context(path: &[(u8, String)], max_tokens: usize) -> String {
    for start in 0..path.len() {
        let context = render_context(&path[start..]);
        if count_tokens(&context) < max_tokens {
            return context;
        }
    }
    String::new()
}

fn render_context(path: &[(u8, String)]) -> String {
    path.iter()
        .map(|(level, text)| format!("{} {text}", "#".repeat(usize::from(*level))))
        .collect::<Vec<_>>()
        .join("\n")
}

fn update_heading_path(path: &mut Vec<(u8, String)>, level: u8, text: &str) {
    while path.last().is_some_and(|(existing, _)| *existing >= level) {
        path.pop();
    }
    if !text.trim().is_empty() {
        path.push((level, text.trim().to_string()));
    }
}

pub(super) fn count_tokens(text: &str) -> usize {
    token_starts(text).len()
}

pub(super) fn split_text(text: &str, limit: usize, overlap: usize) -> Vec<&str> {
    let starts = token_starts(text);
    if starts.len() <= limit || starts.is_empty() {
        return vec![text];
    }
    let mut output = Vec::new();
    let mut start_token = 0usize;
    while start_token < starts.len() {
        let end_token = (start_token + limit).min(starts.len());
        let start_byte = if start_token == 0 {
            0
        } else {
            starts[start_token]
        };
        let end_byte = starts.get(end_token).copied().unwrap_or(text.len());
        output.push(&text[start_byte..end_byte]);
        if end_token == starts.len() {
            break;
        }
        start_token = end_token.saturating_sub(overlap.min(limit.saturating_sub(1)));
    }
    output
}

fn token_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut in_word = false;
    for (offset, character) in text.char_indices() {
        if is_cjk(character) {
            starts.push(offset);
            in_word = false;
        } else if character.is_alphanumeric() || character == '_' {
            if !in_word {
                starts.push(offset);
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    starts
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0x3040..=0x30FF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
    )
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
