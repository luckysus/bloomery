use crate::rag::model::{ChunkId, DocumentVersionId, KnowledgeBaseId, SourceDocumentId};
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

const MAX_RESULTS: usize = 200;

#[derive(Debug, Clone)]
pub struct FtsSearchRequest {
    pub workspace_id: String,
    pub query: String,
    pub knowledge_base_ids: Vec<KnowledgeBaseId>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FtsHit {
    pub knowledge_base_id: KnowledgeBaseId,
    pub document_id: SourceDocumentId,
    pub version_id: DocumentVersionId,
    pub chunk_id: ChunkId,
    pub title_path: String,
    pub source_name: String,
    pub text: String,
    pub snippet: String,
    pub bm25_score: Option<f64>,
    pub cjk_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsError {
    code: &'static str,
    message: String,
}

impl FtsError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for FtsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for FtsError {}

pub fn search(
    connection: &Connection,
    request: &FtsSearchRequest,
) -> Result<Vec<FtsHit>, FtsError> {
    let workspace_id = request.workspace_id.trim();
    if workspace_id.is_empty() || workspace_id != request.workspace_id {
        return Err(FtsError::new(
            "fts_scope_invalid",
            "workspace ID is invalid",
        ));
    }
    let query = request.query.trim();
    let limit = request.limit.min(MAX_RESULTS);
    if query.is_empty() || request.knowledge_base_ids.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let mut base_ids = request
        .knowledge_base_ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    base_ids.sort_unstable();
    base_ids.dedup();
    let Some(match_query) = structured_query(query) else {
        return Ok(Vec::new());
    };

    let mut hits = search_bm25(connection, workspace_id, &base_ids, &match_query, limit)?;
    if contains_cjk(query) && hits.len() < limit {
        let fallback_query = query.trim_matches('"').trim();
        let existing = hits
            .iter()
            .map(|hit| (hit.version_id, hit.chunk_id.clone()))
            .collect::<HashSet<_>>();
        let fallback = search_cjk_fallback(
            connection,
            workspace_id,
            &base_ids,
            fallback_query,
            limit - hits.len(),
        )?;
        hits.extend(
            fallback
                .into_iter()
                .filter(|hit| !existing.contains(&(hit.version_id, hit.chunk_id.clone()))),
        );
        hits.truncate(limit);
    }
    Ok(hits)
}

fn search_bm25(
    connection: &Connection,
    workspace_id: &str,
    base_ids: &[String],
    query: &str,
    limit: usize,
) -> Result<Vec<FtsHit>, FtsError> {
    let placeholders = vec!["?"; base_ids.len()].join(", ");
    let sql = format!(
        "SELECT knowledge_chunks_fts.knowledge_base_id,
                knowledge_chunks_fts.document_id, knowledge_chunks_fts.version_id,
                knowledge_chunks_fts.chunk_id, knowledge_chunks_fts.title_path,
                knowledge_chunks_fts.source_name, knowledge_chunks_fts.text,
                snippet(knowledge_chunks_fts, 8, char(1), char(2), '...', 24),
                bm25(knowledge_chunks_fts)
         FROM knowledge_chunks_fts
         JOIN knowledge_document_versions AS versions
           ON versions.workspace_id = knowledge_chunks_fts.workspace_id
          AND versions.id = knowledge_chunks_fts.version_id
         JOIN knowledge_source_documents AS documents
           ON documents.workspace_id = versions.workspace_id
          AND documents.id = versions.document_id
          AND documents.active_version_id = versions.id
         WHERE knowledge_chunks_fts MATCH ?
           AND knowledge_chunks_fts.workspace_id = ?
           AND documents.knowledge_base_id IN ({placeholders})
         ORDER BY bm25(knowledge_chunks_fts), knowledge_chunks_fts.chunk_id,
                  knowledge_chunks_fts.version_id
         LIMIT ?"
    );
    let mut values = vec![
        Value::Text(query.to_string()),
        Value::Text(workspace_id.to_string()),
    ];
    values.extend(base_ids.iter().cloned().map(Value::Text));
    values.push(Value::Integer(limit as i64));
    let mut statement = connection.prepare(&sql).map_err(storage)?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok(RawHit {
                knowledge_base_id: row.get(0)?,
                document_id: row.get(1)?,
                version_id: row.get(2)?,
                chunk_id: row.get(3)?,
                title_path: row.get(4)?,
                source_name: row.get(5)?,
                text: row.get(6)?,
                snippet: row.get(7)?,
                score: Some(row.get(8)?),
                cjk_fallback: false,
            })
        })
        .map_err(storage)?;
    rows.map(|row| row.map_err(storage).and_then(FtsHit::try_from))
        .collect()
}

fn search_cjk_fallback(
    connection: &Connection,
    workspace_id: &str,
    base_ids: &[String],
    query: &str,
    limit: usize,
) -> Result<Vec<FtsHit>, FtsError> {
    if query.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; base_ids.len()].join(", ");
    let sql = format!(
        "SELECT documents.knowledge_base_id, documents.id, versions.id, chunks.id,
                chunks.source_location_json, documents.display_name, chunks.text
         FROM knowledge_chunks AS chunks
         JOIN knowledge_document_versions AS versions
           ON versions.workspace_id = chunks.workspace_id AND versions.id = chunks.version_id
         JOIN knowledge_source_documents AS documents
           ON documents.workspace_id = versions.workspace_id
          AND documents.id = versions.document_id
          AND documents.active_version_id = versions.id
         WHERE chunks.workspace_id = ? AND instr(chunks.text, ?) > 0
           AND documents.knowledge_base_id IN ({placeholders})
         ORDER BY chunks.id, versions.id
         LIMIT ?"
    );
    let mut values = vec![
        Value::Text(workspace_id.to_string()),
        Value::Text(query.to_string()),
    ];
    values.extend(base_ids.iter().cloned().map(Value::Text));
    values.push(Value::Integer(limit as i64));
    let mut statement = connection.prepare(&sql).map_err(storage)?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(storage)?;
    rows.map(|row| {
        let (base, document, version, chunk, location, source_name, text) = row.map_err(storage)?;
        Ok(FtsHit {
            knowledge_base_id: parse(&base, "knowledge base ID")?,
            document_id: parse(&document, "document ID")?,
            version_id: parse(&version, "version ID")?,
            chunk_id: ChunkId::new(chunk)
                .map_err(|error| FtsError::new("fts_row_invalid", error))?,
            title_path: title_path(&location)?,
            source_name,
            snippet: highlight_plain(&text, query),
            text,
            bm25_score: None,
            cjk_fallback: true,
        })
    })
    .collect()
}

struct RawHit {
    knowledge_base_id: String,
    document_id: String,
    version_id: String,
    chunk_id: String,
    title_path: String,
    source_name: String,
    text: String,
    snippet: String,
    score: Option<f64>,
    cjk_fallback: bool,
}

impl FtsHit {
    fn try_from(raw: RawHit) -> Result<Self, FtsError> {
        Ok(Self {
            knowledge_base_id: parse(&raw.knowledge_base_id, "knowledge base ID")?,
            document_id: parse(&raw.document_id, "document ID")?,
            version_id: parse(&raw.version_id, "version ID")?,
            chunk_id: ChunkId::new(raw.chunk_id)
                .map_err(|error| FtsError::new("fts_row_invalid", error))?,
            title_path: raw.title_path,
            source_name: raw.source_name,
            text: raw.text,
            snippet: render_snippet(&raw.snippet),
            bm25_score: raw.score,
            cjk_fallback: raw.cjk_fallback,
        })
    }
}

fn structured_query(query: &str) -> Option<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in query.chars() {
        match character {
            '"' => {
                if quoted {
                    push_part(&mut parts, &mut current);
                } else {
                    push_part(&mut parts, &mut current);
                }
                quoted = !quoted;
            }
            value if quoted && (value.is_alphanumeric() || value.is_whitespace()) => {
                current.push(value)
            }
            value if !quoted && (value.is_alphanumeric() || matches!(value, '-' | '_')) => {
                current.push(value)
            }
            _ => push_part(&mut parts, &mut current),
        }
    }
    push_part(&mut parts, &mut current);
    if parts.is_empty() {
        None
    } else {
        Some(
            parts
                .into_iter()
                .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" AND "),
        )
    }
}

fn push_part(parts: &mut Vec<String>, current: &mut String) {
    let normalized = current.split_whitespace().collect::<Vec<_>>().join(" ");
    if !normalized.is_empty() {
        parts.push(normalized);
    }
    current.clear();
}

fn contains_cjk(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character as u32, 0x3400..=0x9fff | 0xf900..=0xfaff))
}

fn title_path(value: &str) -> Result<String, FtsError> {
    let location: crate::rag::model::SourceLocation = serde_json::from_str(value)
        .map_err(|error| FtsError::new("fts_row_invalid", error.to_string()))?;
    Ok(match location {
        crate::rag::model::SourceLocation::Heading { path } => path.join(" > "),
        _ => String::new(),
    })
}

fn highlight_plain(text: &str, query: &str) -> String {
    match text.find(query) {
        Some(start) => {
            let end = start + query.len();
            format!(
                "{}<mark>{}</mark>{}",
                escape_html(&text[..start]),
                escape_html(&text[start..end]),
                escape_html(&text[end..])
            )
        }
        None => escape_html(text),
    }
}

fn render_snippet(value: &str) -> String {
    let mut rendered = String::new();
    for character in value.chars() {
        match character {
            '\u{1}' => rendered.push_str("<mark>"),
            '\u{2}' => rendered.push_str("</mark>"),
            '&' => rendered.push_str("&amp;"),
            '<' => rendered.push_str("&lt;"),
            '>' => rendered.push_str("&gt;"),
            '"' => rendered.push_str("&quot;"),
            '\'' => rendered.push_str("&#39;"),
            _ => rendered.push(character),
        }
    }
    rendered
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn parse<T: FromStr>(value: &str, name: &str) -> Result<T, FtsError>
where
    T::Err: fmt::Display,
{
    value
        .parse()
        .map_err(|error| FtsError::new("fts_row_invalid", format!("invalid {name}: {error}")))
}

fn storage(error: rusqlite::Error) -> FtsError {
    FtsError::new("fts_storage_failed", error.to_string())
}
