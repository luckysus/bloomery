use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

pub const DEFAULT_MODEL_LIMIT: usize = 8_192;
const ASCII_CHARS_PER_TOKEN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextSource {
    Security,
    System,
    Domain,
    Permission,
    CurrentRequest,
    RecentTurn { newest_first_rank: usize },
    ToolEvidence,
    ExplicitMemory,
    HistoricalSummary,
}

impl ContextSource {
    const fn priority(self) -> u8 {
        match self {
            Self::Security | Self::System | Self::Domain | Self::Permission => 0,
            Self::CurrentRequest => 1,
            Self::RecentTurn { .. } => 2,
            Self::ToolEvidence => 3,
            Self::ExplicitMemory => 4,
            Self::HistoricalSummary => 5,
        }
    }

    const fn is_required(self) -> bool {
        matches!(
            self,
            Self::Security | Self::System | Self::Domain | Self::Permission | Self::CurrentRequest
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextItem {
    pub id: String,
    pub source: ContextSource,
    pub content: String,
}

impl ContextItem {
    pub fn new(id: impl Into<String>, source: ContextSource, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemTokenEstimate {
    pub id: String,
    pub original_tokens: usize,
    pub included_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TruncationRecord {
    pub id: String,
    pub original_tokens: usize,
    pub included_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextReport {
    pub model_limit: usize,
    pub input_limit: usize,
    pub output_reservation: usize,
    pub estimated_included_tokens: usize,
    pub included_ids: Vec<String>,
    pub omitted_ids: Vec<String>,
    pub included_items: Vec<ContextItem>,
    pub item_token_estimates: Vec<ItemTokenEstimate>,
    pub truncations: Vec<TruncationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextBudgetError {
    MissingCurrentRequest,
    EmptyCurrentRequest,
    MultipleCurrentRequests {
        count: usize,
    },
    DuplicateItemId {
        id: String,
    },
    DuplicateRecentTurnRank {
        newest_first_rank: usize,
    },
    OutputReservationExceedsModelLimit {
        output_reservation: usize,
        model_limit: usize,
    },
    RequiredContentExceedsLimit {
        required_tokens: usize,
        input_limit: usize,
        model_limit: usize,
    },
}

impl fmt::Display for ContextBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCurrentRequest => formatter.write_str("current request is required"),
            Self::EmptyCurrentRequest => formatter.write_str("current request cannot be empty"),
            Self::MultipleCurrentRequests { count } => {
                write!(formatter, "exactly one current request is required, found {count}")
            }
            Self::DuplicateItemId { id } => write!(formatter, "duplicate context item id: {id}"),
            Self::DuplicateRecentTurnRank {
                newest_first_rank,
            } => write!(
                formatter,
                "duplicate recent turn rank: {newest_first_rank}"
            ),
            Self::OutputReservationExceedsModelLimit {
                output_reservation,
                model_limit,
            } => write!(
                formatter,
                "output reservation {output_reservation} exceeds model limit {model_limit}"
            ),
            Self::RequiredContentExceedsLimit {
                required_tokens,
                input_limit,
                model_limit,
            } => write!(
                formatter,
                "required context needs {required_tokens} tokens, input limit is {input_limit}, model limit is {model_limit}"
            ),
        }
    }
}

impl std::error::Error for ContextBudgetError {}

pub fn budget_context(
    items: &[ContextItem],
    model_limit: Option<usize>,
    output_reservation: usize,
) -> Result<ContextReport, ContextBudgetError> {
    validate_items(items)?;
    let current_request_count = items
        .iter()
        .filter(|item| item.source == ContextSource::CurrentRequest)
        .count();
    match current_request_count {
        0 => return Err(ContextBudgetError::MissingCurrentRequest),
        1 => {}
        count => return Err(ContextBudgetError::MultipleCurrentRequests { count }),
    }
    if items
        .iter()
        .find(|item| item.source == ContextSource::CurrentRequest)
        .is_some_and(|item| item.content.trim().is_empty())
    {
        return Err(ContextBudgetError::EmptyCurrentRequest);
    }

    let model_limit = model_limit.unwrap_or(DEFAULT_MODEL_LIMIT);
    if output_reservation > model_limit {
        return Err(ContextBudgetError::OutputReservationExceedsModelLimit {
            output_reservation,
            model_limit,
        });
    }
    let input_limit = model_limit - output_reservation;
    let original_tokens = items
        .iter()
        .map(|item| estimate_tokens(&item.content))
        .collect::<Vec<_>>();
    let required_tokens = items
        .iter()
        .zip(&original_tokens)
        .filter(|(item, _)| item.source.is_required())
        .fold(0usize, |total, (_, tokens)| total.saturating_add(*tokens));
    if required_tokens > input_limit {
        return Err(ContextBudgetError::RequiredContentExceedsLimit {
            required_tokens,
            input_limit,
            model_limit,
        });
    }

    let mut included_items = Vec::new();
    let mut included_ids = Vec::new();
    let mut omitted_ids = Vec::new();
    let mut truncations = Vec::new();
    let mut included_tokens_by_index = vec![0usize; items.len()];
    let mut estimated_included_tokens = 0usize;
    let mut recent_blocked = false;

    for index in selection_order(items) {
        let item = &items[index];
        let original = original_tokens[index];

        if item.source.is_required() {
            include_item(
                item,
                original,
                &mut included_items,
                &mut included_ids,
                &mut included_tokens_by_index[index],
                &mut estimated_included_tokens,
            );
            continue;
        }

        if matches!(item.source, ContextSource::RecentTurn { .. }) {
            if recent_blocked || original > input_limit.saturating_sub(estimated_included_tokens) {
                recent_blocked = true;
                omitted_ids.push(item.id.clone());
            } else {
                include_item(
                    item,
                    original,
                    &mut included_items,
                    &mut included_ids,
                    &mut included_tokens_by_index[index],
                    &mut estimated_included_tokens,
                );
            }
            continue;
        }

        let remaining = input_limit.saturating_sub(estimated_included_tokens);
        if original <= remaining {
            include_item(
                item,
                original,
                &mut included_items,
                &mut included_ids,
                &mut included_tokens_by_index[index],
                &mut estimated_included_tokens,
            );
        } else if remaining == 0 {
            omitted_ids.push(item.id.clone());
        } else {
            let content = truncate_to_tokens(&item.content, remaining);
            let included = estimate_tokens(&content);
            if included == 0 {
                omitted_ids.push(item.id.clone());
                continue;
            }
            let mut truncated_item = item.clone();
            truncated_item.content = content;
            included_ids.push(item.id.clone());
            included_tokens_by_index[index] = included;
            estimated_included_tokens = estimated_included_tokens.saturating_add(included);
            included_items.push(truncated_item);
            truncations.push(TruncationRecord {
                id: item.id.clone(),
                original_tokens: original,
                included_tokens: included,
            });
        }
    }

    let item_token_estimates = items
        .iter()
        .enumerate()
        .map(|(index, item)| ItemTokenEstimate {
            id: item.id.clone(),
            original_tokens: original_tokens[index],
            included_tokens: included_tokens_by_index[index],
        })
        .collect();

    Ok(ContextReport {
        model_limit,
        input_limit,
        output_reservation,
        estimated_included_tokens,
        included_ids,
        omitted_ids,
        included_items,
        item_token_estimates,
        truncations,
    })
}

pub fn estimate_tokens(text: &str) -> usize {
    // Conservative deterministic heuristic: ASCII word runs cost one token per four chars; every other character costs one.
    let mut tokens = 0usize;
    let mut ascii_run = 0usize;
    for character in text.chars() {
        tokens = tokens.saturating_add(next_token_cost(character, &mut ascii_run));
    }
    tokens
}

fn next_token_cost(character: char, ascii_run: &mut usize) -> usize {
    if character.is_ascii_alphanumeric() || character == '_' {
        let cost = usize::from((*ascii_run).is_multiple_of(ASCII_CHARS_PER_TOKEN));
        *ascii_run = ascii_run.saturating_add(1);
        cost
    } else {
        *ascii_run = 0;
        1
    }
}

fn validate_items(items: &[ContextItem]) -> Result<(), ContextBudgetError> {
    for (index, item) in items.iter().enumerate() {
        if items[..index].iter().any(|previous| previous.id == item.id) {
            return Err(ContextBudgetError::DuplicateItemId {
                id: item.id.clone(),
            });
        }
        if let ContextSource::RecentTurn { newest_first_rank } = item.source {
            if items[..index].iter().any(|previous| {
                matches!(
                    previous.source,
                    ContextSource::RecentTurn {
                        newest_first_rank: previous_rank
                    } if previous_rank == newest_first_rank
                )
            }) {
                return Err(ContextBudgetError::DuplicateRecentTurnRank { newest_first_rank });
            }
        }
    }
    Ok(())
}

fn selection_order(items: &[ContextItem]) -> Vec<usize> {
    let mut indices = (0..items.len()).collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        let left_source = items[*left].source;
        let right_source = items[*right].source;
        left_source
            .priority()
            .cmp(&right_source.priority())
            .then_with(|| match (left_source, right_source) {
                (
                    ContextSource::RecentTurn {
                        newest_first_rank: left_rank,
                    },
                    ContextSource::RecentTurn {
                        newest_first_rank: right_rank,
                    },
                ) => left_rank.cmp(&right_rank),
                _ => Ordering::Equal,
            })
    });
    indices
}

fn include_item(
    item: &ContextItem,
    tokens: usize,
    included_items: &mut Vec<ContextItem>,
    included_ids: &mut Vec<String>,
    included_tokens: &mut usize,
    estimated_included_tokens: &mut usize,
) {
    included_items.push(item.clone());
    included_ids.push(item.id.clone());
    *included_tokens = tokens;
    *estimated_included_tokens = estimated_included_tokens.saturating_add(tokens);
}

fn truncate_to_tokens(text: &str, limit: usize) -> String {
    let mut tokens = 0usize;
    let mut ascii_run = 0usize;
    let mut end = 0usize;
    for (index, character) in text.char_indices() {
        let cost = next_token_cost(character, &mut ascii_run);
        if tokens.saturating_add(cost) > limit {
            break;
        }
        tokens = tokens.saturating_add(cost);
        end = index + character.len_utf8();
    }
    text[..end].to_string()
}
