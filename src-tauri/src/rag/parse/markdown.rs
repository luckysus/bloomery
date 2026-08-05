use super::{offsets, DocumentBlock, ParseError, ParseWarning, ParsedDocument};

struct Line<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

pub(super) fn parse(source: &str) -> Result<ParsedDocument, ParseError> {
    let lines = lines(source);
    let mut document = ParsedDocument::empty();
    let mut index = 0usize;
    while index < lines.len() {
        if lines[index].text.trim().is_empty() {
            index += 1;
            continue;
        }
        if let Some((level, text)) = heading(lines[index].text) {
            document.blocks.push(DocumentBlock::Heading {
                level,
                text: text.to_string(),
                location: offsets(lines[index].start, lines[index].end),
            });
            index += 1;
            continue;
        }
        if let Some((ordered, item)) = list_item(lines[index].text) {
            let start = lines[index].start;
            let mut end = lines[index].end;
            let mut items = vec![item.to_string()];
            index += 1;
            while index < lines.len() {
                let Some((next_ordered, item)) = list_item(lines[index].text) else {
                    break;
                };
                if next_ordered != ordered {
                    break;
                }
                items.push(item.to_string());
                end = lines[index].end;
                index += 1;
            }
            document.blocks.push(DocumentBlock::List {
                ordered,
                items,
                location: offsets(start, end),
            });
            continue;
        }
        if let Some(formula) = formula(lines[index].text) {
            document.blocks.push(DocumentBlock::Formula {
                text: formula.to_string(),
                location: offsets(lines[index].start, lines[index].end),
            });
            index += 1;
            continue;
        }
        if index + 1 < lines.len()
            && is_table_row(lines[index].text)
            && is_table_separator(lines[index + 1].text)
        {
            let start = lines[index].start;
            let mut end = lines[index + 1].end;
            let mut rows = vec![table_row(lines[index].text)];
            index += 2;
            while index < lines.len() && is_table_row(lines[index].text) {
                rows.push(table_row(lines[index].text));
                end = lines[index].end;
                index += 1;
            }
            document.blocks.push(DocumentBlock::Table {
                rows,
                location: offsets(start, end),
            });
            continue;
        }
        if let Some((alt, target)) = image(lines[index].text) {
            let location = offsets(lines[index].start, lines[index].end);
            document.blocks.push(DocumentBlock::Image {
                alt: alt.to_string(),
                asset_index: None,
                location: location.clone(),
            });
            document.warnings.push(ParseWarning {
                code: if target.starts_with("http://") || target.starts_with("https://") {
                    "remote_asset_ignored".to_string()
                } else {
                    "external_asset_not_embedded".to_string()
                },
                message: "Markdown image references are not loaded during parsing".to_string(),
                location: Some(location),
            });
            index += 1;
            continue;
        }

        let start = lines[index].start;
        let mut end = lines[index].end;
        let mut paragraph = vec![lines[index].text.trim()];
        index += 1;
        while index < lines.len()
            && !lines[index].text.trim().is_empty()
            && heading(lines[index].text).is_none()
            && list_item(lines[index].text).is_none()
            && formula(lines[index].text).is_none()
            && image(lines[index].text).is_none()
            && !(index + 1 < lines.len()
                && is_table_row(lines[index].text)
                && is_table_separator(lines[index + 1].text))
        {
            paragraph.push(lines[index].text.trim());
            end = lines[index].end;
            index += 1;
        }
        document.blocks.push(DocumentBlock::Paragraph {
            text: paragraph.join(" "),
            location: offsets(start, end),
        });
    }
    Ok(document)
}

fn lines(source: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut offset = 0usize;
    for segment in source.split_inclusive('\n') {
        let text = segment.trim_end_matches(['\r', '\n']);
        lines.push(Line {
            text,
            start: offset,
            end: offset + text.len(),
        });
        offset += segment.len();
    }
    lines
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let count = line.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&count) && line.as_bytes().get(count) == Some(&b' ') {
        Some((count as u8, line[count + 1..].trim()))
    } else {
        None
    }
}

fn list_item(line: &str) -> Option<(bool, &str)> {
    let trimmed = line.trim_start();
    if let Some(item) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
    {
        return Some((false, item.trim()));
    }
    let marker = trimmed.find(". ")?;
    trimmed[..marker]
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| (true, trimmed[marker + 2..].trim()))
}

fn formula(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    (trimmed.len() >= 4 && trimmed.starts_with("$$") && trimmed.ends_with("$$"))
        .then(|| trimmed[2..trimmed.len() - 2].trim())
}

fn is_table_row(line: &str) -> bool {
    line.trim().starts_with('|') && line.trim().ends_with('|')
}

fn is_table_separator(line: &str) -> bool {
    let cells = table_row(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let cell = cell.trim_matches(':');
            cell.len() >= 3 && cell.bytes().all(|byte| byte == b'-')
        })
}

fn table_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn image(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("![")?;
    let separator = rest.find("](")?;
    let target = rest.get(separator + 2..)?.strip_suffix(')')?;
    Some((&rest[..separator], target.trim()))
}

pub(super) fn image_targets(source: &str) -> Vec<String> {
    lines(source)
        .into_iter()
        .filter_map(|line| image(line.text).map(|(_, target)| target.to_string()))
        .collect()
}
