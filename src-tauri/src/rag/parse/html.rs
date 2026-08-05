use super::{
    collapse_whitespace, offsets, DocumentBlock, ParseError, ParseWarning, ParsedDocument,
};
use html5ever::tokenizer::{
    BufferQueue, EndTag, StartTag, Tag, TagToken, Token, TokenSink, TokenSinkResult, Tokenizer,
    TokenizerOpts,
};
use std::cell::RefCell;

pub(super) fn parse(source: &str) -> Result<ParsedDocument, ParseError> {
    let sink = HtmlSink::new(source);
    let input = BufferQueue::default();
    input.push_back(source.into());
    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    Ok(tokenizer.sink.finish())
}

struct HtmlSink {
    state: RefCell<HtmlState>,
}

struct HtmlState {
    document: ParsedDocument,
    line_offsets: Vec<usize>,
    source_len: usize,
    capture: Option<Capture>,
    list: Option<ListCapture>,
    table: Option<TableCapture>,
    ignored_depth: usize,
}

struct Capture {
    kind: CaptureKind,
    text: String,
    line: u64,
}

enum CaptureKind {
    Heading(u8),
    Paragraph,
    ListItem,
    TableCell,
}

struct ListCapture {
    ordered: bool,
    items: Vec<String>,
    line: u64,
}

struct TableCapture {
    rows: Vec<Vec<String>>,
    row: Vec<String>,
    line: u64,
}

impl HtmlSink {
    fn new(source: &str) -> Self {
        let mut line_offsets = vec![0];
        line_offsets.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        );
        Self {
            state: RefCell::new(HtmlState {
                document: ParsedDocument::empty(),
                line_offsets,
                source_len: source.len(),
                capture: None,
                list: None,
                table: None,
                ignored_depth: 0,
            }),
        }
    }

    fn finish(self) -> ParsedDocument {
        self.state.into_inner().document
    }
}

impl TokenSink for HtmlSink {
    type Handle = ();

    fn process_token(&self, token: Token, line: u64) -> TokenSinkResult<Self::Handle> {
        let mut state = self.state.borrow_mut();
        match token {
            TagToken(tag) if tag.kind == StartTag => start_tag(&mut state, tag, line),
            TagToken(tag) if tag.kind == EndTag => end_tag(&mut state, tag, line),
            Token::CharacterTokens(text) if state.ignored_depth == 0 => {
                if let Some(capture) = &mut state.capture {
                    capture.text.push_str(&text);
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

fn start_tag(state: &mut HtmlState, tag: Tag, line: u64) {
    let name = tag.name.as_ref();
    if matches!(name, "script" | "style" | "template") {
        state.ignored_depth += 1;
        return;
    }
    if state.ignored_depth > 0 {
        return;
    }
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            state.capture = Some(Capture {
                kind: CaptureKind::Heading(name[1..].parse().unwrap_or(1)),
                text: String::new(),
                line,
            });
        }
        "p" => begin_capture(state, CaptureKind::Paragraph, line),
        "ul" | "ol" => {
            state.list = Some(ListCapture {
                ordered: name == "ol",
                items: Vec::new(),
                line,
            });
        }
        "li" => begin_capture(state, CaptureKind::ListItem, line),
        "table" => {
            state.table = Some(TableCapture {
                rows: Vec::new(),
                row: Vec::new(),
                line,
            });
        }
        "td" | "th" => begin_capture(state, CaptureKind::TableCell, line),
        "img" => image(state, &tag, line),
        _ => {}
    }
}

fn end_tag(state: &mut HtmlState, tag: Tag, line: u64) {
    let name = tag.name.as_ref();
    if matches!(name, "script" | "style" | "template") {
        state.ignored_depth = state.ignored_depth.saturating_sub(1);
        return;
    }
    if state.ignored_depth > 0 {
        return;
    }
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" => {
            let Some(capture) = state.capture.take() else {
                return;
            };
            let text = collapse_whitespace(&capture.text);
            if text.is_empty() {
                return;
            }
            let location = line_location(state, capture.line, line);
            state.document.blocks.push(match capture.kind {
                CaptureKind::Heading(level) => DocumentBlock::Heading {
                    level,
                    text,
                    location,
                },
                _ => DocumentBlock::Paragraph { text, location },
            });
        }
        "li" => {
            if let Some(capture) = state.capture.take() {
                if let Some(list) = &mut state.list {
                    list.items.push(collapse_whitespace(&capture.text));
                }
            }
        }
        "ul" | "ol" => {
            if let Some(list) = state.list.take() {
                if !list.items.is_empty() {
                    state.document.blocks.push(DocumentBlock::List {
                        ordered: list.ordered,
                        items: list.items,
                        location: line_location(state, list.line, line),
                    });
                }
            }
        }
        "td" | "th" => {
            if let Some(capture) = state.capture.take() {
                if let Some(table) = &mut state.table {
                    table.row.push(collapse_whitespace(&capture.text));
                }
            }
        }
        "tr" => {
            if let Some(table) = &mut state.table {
                if !table.row.is_empty() {
                    table.rows.push(std::mem::take(&mut table.row));
                }
            }
        }
        "table" => {
            if let Some(table) = state.table.take() {
                if !table.rows.is_empty() {
                    state.document.blocks.push(DocumentBlock::Table {
                        rows: table.rows,
                        location: line_location(state, table.line, line),
                    });
                }
            }
        }
        _ => {}
    }
}

fn begin_capture(state: &mut HtmlState, kind: CaptureKind, line: u64) {
    state.capture = Some(Capture {
        kind,
        text: String::new(),
        line,
    });
}

fn image(state: &mut HtmlState, tag: &Tag, line: u64) {
    let attribute = |name: &str| {
        tag.attrs
            .iter()
            .find(|attribute| attribute.name.local.as_ref() == name)
            .map(|attribute| attribute.value.to_string())
            .unwrap_or_default()
    };
    let source = attribute("src");
    let location = line_location(state, line, line);
    state.document.blocks.push(DocumentBlock::Image {
        alt: attribute("alt"),
        asset_index: None,
        location: location.clone(),
    });
    state.document.warnings.push(ParseWarning {
        code: if source.starts_with("http://") || source.starts_with("https://") {
            "remote_asset_ignored".to_string()
        } else {
            "external_asset_not_embedded".to_string()
        },
        message: "HTML image references are not loaded during parsing".to_string(),
        location: Some(location),
    });
}

fn line_location(
    state: &HtmlState,
    start_line: u64,
    end_line: u64,
) -> crate::rag::model::SourceLocation {
    let start = state
        .line_offsets
        .get(start_line.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(0);
    let end = state
        .line_offsets
        .get(end_line as usize)
        .copied()
        .unwrap_or(state.source_len)
        .max(start + 1);
    offsets(start, end)
}
