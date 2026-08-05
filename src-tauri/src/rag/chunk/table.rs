use super::{count_tokens, split_text};

pub(super) fn windows(
    rows: &[Vec<String>],
    header_rows: usize,
    target_tokens: usize,
    max_tokens: usize,
) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }
    let header_count = header_rows.min(rows.len());
    let header = rows[..header_count]
        .iter()
        .map(|row| render_row(row))
        .collect::<Vec<_>>()
        .join("\n");
    if header_count == rows.len() {
        return split_text(&header, max_tokens.max(1), 0)
            .into_iter()
            .map(str::to_string)
            .collect();
    }
    let header_tokens = count_tokens(&header);
    if header_tokens >= max_tokens {
        return split_text(
            &rows
                .iter()
                .map(|row| render_row(row))
                .collect::<Vec<_>>()
                .join("\n"),
            max_tokens,
            0,
        )
        .into_iter()
        .map(str::to_string)
        .collect();
    }

    let mut output = Vec::new();
    let mut current = header.clone();
    let mut has_data = false;
    for row in &rows[header_count..] {
        let row = render_row(row);
        let candidate = format!("{current}\n{row}");
        if has_data && count_tokens(&candidate) > target_tokens {
            output.push(current);
            current = header.clone();
            has_data = false;
        }
        let candidate = format!("{current}\n{row}");
        if count_tokens(&candidate) <= max_tokens {
            current = candidate;
            has_data = true;
            continue;
        }
        if has_data {
            output.push(current);
        }
        let row_budget = max_tokens.saturating_sub(header_tokens).max(1);
        for part in split_text(&row, row_budget, 0) {
            output.push(format!("{header}\n{part}"));
        }
        current = header.clone();
        has_data = false;
    }
    if has_data || output.is_empty() {
        output.push(current);
    }
    output
}

fn render_row(row: &[String]) -> String {
    format!(
        "| {} |",
        row.iter()
            .map(|cell| cell.replace('|', "\\|"))
            .collect::<Vec<_>>()
            .join(" | ")
    )
}
