use std::collections::{HashMap, HashSet};

const SCORE_FLOOR_RATIO: f64 = 0.15;

pub struct SearchDocument {
    pub index: usize,
    pub text: String,
}

pub struct SearchHit {
    pub index: usize,
    pub score: f64,
    pub snippet: String,
}

pub fn search(
    query: &str,
    docs: &[SearchDocument],
    limit: usize,
    snippet_chars: usize,
) -> Vec<SearchHit> {
    let query_terms = unique(tokens(query));
    if query_terms.is_empty() || docs.is_empty() || limit == 0 {
        return Vec::new();
    }

    let indexed = docs
        .iter()
        .filter_map(|doc| {
            let terms = tokens(&doc.text);
            if terms.is_empty() {
                return None;
            }
            Some((doc, counts(&terms), terms.len()))
        })
        .collect::<Vec<_>>();
    if indexed.is_empty() {
        return Vec::new();
    }

    let mut df = HashMap::new();
    let total_len = indexed.iter().map(|(_, _, len)| *len).sum::<usize>();
    for (_, counts, _) in &indexed {
        for term in counts.keys() {
            *df.entry(term.clone()).or_insert(0usize) += 1;
        }
    }
    let avg_len = total_len as f64 / indexed.len() as f64;

    let mut hits = indexed
        .into_iter()
        .filter_map(|(doc, counts, len)| {
            let score = bm25_score(&counts, len, &query_terms, &df, docs.len(), avg_len);
            if score <= 0.0 {
                return None;
            }
            Some(SearchHit {
                index: doc.index,
                score,
                snippet: make_snippet(&doc.text, query, &query_terms, snippet_chars),
            })
        })
        .collect::<Vec<_>>();

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(top) = hits.first().map(|hit| hit.score) {
        let cutoff = top * SCORE_FLOOR_RATIO;
        hits.retain(|hit| hit.score >= cutoff || (hit.score - top).abs() < f64::EPSILON);
    }
    hits.truncate(limit);
    hits
}

pub fn estimate_text_tokens(value: &str) -> usize {
    if value.is_empty() {
        return 0;
    }
    let bytes = value.len().div_ceil(4);
    let chars = value.chars().count();
    bytes.max(chars)
}

pub fn compact_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tokens(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for ch in value.chars() {
        if is_cjk(ch) {
            flush_word(&mut out, &mut word);
            out.push(ch.to_string());
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            word.push(ch.to_ascii_lowercase());
        } else {
            flush_word(&mut out, &mut word);
        }
    }
    flush_word(&mut out, &mut word);
    out
}

fn flush_word(out: &mut Vec<String>, word: &mut String) {
    if !word.is_empty() {
        out.push(std::mem::take(word));
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0x3040..=0x309F
            | 0x30A0..=0x30FF
            | 0xAC00..=0xD7AF
    )
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

fn counts(terms: &[String]) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for term in terms {
        *out.entry(term.clone()).or_insert(0) += 1;
    }
    out
}

fn bm25_score(
    counts: &HashMap<String, usize>,
    length: usize,
    query_terms: &[String],
    df: &HashMap<String, usize>,
    total_docs: usize,
    avg_len: f64,
) -> f64 {
    if length == 0 || total_docs == 0 {
        return 0.0;
    }
    let k1 = 1.2;
    let b = 0.75;
    let doc_len = length as f64;
    query_terms.iter().fold(0.0, |score, term| {
        let Some(tf) = counts.get(term).copied() else {
            return score;
        };
        let Some(term_df) = df.get(term).copied() else {
            return score;
        };
        let idf = (1.0 + (total_docs as f64 - term_df as f64 + 0.5) / (term_df as f64 + 0.5)).ln();
        score
            + idf * (tf as f64 * (k1 + 1.0))
                / (tf as f64 + k1 * (1.0 - b + b * doc_len / avg_len.max(1.0)))
    })
}

fn make_snippet(text: &str, query: &str, terms: &[String], max_chars: usize) -> String {
    let compact = compact_whitespace(text);
    let chars = compact.chars().collect::<Vec<_>>();
    if max_chars == 0 || chars.len() <= max_chars {
        return compact;
    }

    let lower = compact.to_lowercase();
    let query = query.trim().to_lowercase();
    let byte_idx = if query.is_empty() {
        None
    } else {
        lower.find(&query)
    }
    .or_else(|| {
        terms.iter().find_map(|term| {
            if term.chars().count() == 1 && !term.chars().next().is_some_and(is_cjk) {
                return None;
            }
            lower.find(term)
        })
    })
    .unwrap_or(0);

    let center = compact[..byte_idx.min(compact.len())].chars().count();
    let mut start = center.saturating_sub(max_chars / 2);
    let mut end = (start + max_chars).min(chars.len());
    if end - start < max_chars {
        start = end.saturating_sub(max_chars);
    }
    if end > chars.len() {
        end = chars.len();
    }
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < chars.len() { "..." } else { "" };
    format!(
        "{prefix}{}{suffix}",
        chars[start..end].iter().collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_latin_and_cjk() {
        assert_eq!(
            tokens("BM25 检索 cache-first"),
            vec!["bm25", "检", "索", "cache", "first"]
        );
    }

    #[test]
    fn ranks_matching_document() {
        let docs = vec![
            SearchDocument {
                index: 0,
                text: "prompt cache stability".into(),
            },
            SearchDocument {
                index: 1,
                text: "dashboard colors".into(),
            },
        ];
        let hits = search("prompt cache", &docs, 4, 80);
        assert_eq!(hits.first().map(|hit| hit.index), Some(0));
    }
}
