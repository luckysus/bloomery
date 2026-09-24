const ASCII_CHARS_PER_TOKEN: usize = 4;

pub fn estimate_tokens(text: &str) -> usize {
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

pub(super) fn truncate_to_tokens(text: &str, limit: usize) -> String {
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
