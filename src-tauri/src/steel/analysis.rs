use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_ANALYSIS_ROWS: usize = 100_000;
const MAX_TOP_VALUES: usize = 5;
const MAX_OUTLIER_EVIDENCE: usize = 128;
const MAX_GROUPS: usize = 128;
const MAX_CORRELATION_COLUMNS: usize = 32;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetAnalysisRequest {
    #[serde(default)]
    pub selected_columns: Vec<usize>,
    #[serde(default)]
    pub outlier_iqr_multiplier: Option<f64>,
    #[serde(default)]
    pub group_by_column: Option<usize>,
    #[serde(default)]
    pub correlation_columns: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetAnalysis {
    pub dataset_id: Option<String>,
    pub source_sha256: Option<String>,
    pub selected_sheet: Option<String>,
    pub row_count: usize,
    pub analyzed_row_count: usize,
    pub excluded_row_count: usize,
    pub columns: Vec<DatasetColumnAnalysis>,
    pub groups: Vec<DatasetGroupSummary>,
    pub correlations: Vec<DatasetCorrelation>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetColumnAnalysis {
    pub ordinal: usize,
    pub name: String,
    pub canonical_field: Option<String>,
    pub unit: Option<String>,
    pub inferred_type: String,
    pub sample_count: usize,
    pub missing_count: usize,
    pub invalid_count: usize,
    pub missing_rate: f64,
    pub distinct_count: usize,
    pub mean: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub min: Option<f64>,
    pub percentile_25: Option<f64>,
    pub median: Option<f64>,
    pub percentile_75: Option<f64>,
    pub max: Option<f64>,
    pub outlier_count: usize,
    pub outlier_rows: Vec<usize>,
    pub top_values: Vec<DatasetValueFrequency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetValueFrequency {
    pub value: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetGroupSummary {
    pub key: String,
    pub row_count: usize,
    pub columns: Vec<DatasetGroupColumnSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetGroupColumnSummary {
    pub ordinal: usize,
    pub sample_count: usize,
    pub mean: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetCorrelation {
    pub left_ordinal: usize,
    pub right_ordinal: usize,
    pub sample_count: usize,
    pub pearson: Option<f64>,
}

pub fn analyze_dataset(
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    request: DatasetAnalysisRequest,
) -> Result<DatasetAnalysis, String> {
    if headers.is_empty() {
        return Err("dataset analysis requires at least one column".to_string());
    }
    let multiplier = request.outlier_iqr_multiplier.unwrap_or(1.5);
    if !multiplier.is_finite() || multiplier <= 0.0 {
        return Err("outlier IQR multiplier must be finite and positive".to_string());
    }

    let selected_columns = selected_columns(&headers, &request.selected_columns)?;
    let analyzed_row_count = rows.len().min(MAX_ANALYSIS_ROWS);
    let mut warnings = Vec::new();
    if rows.len() > MAX_ANALYSIS_ROWS {
        warnings.push(format!(
            "analysis is bounded to the first {MAX_ANALYSIS_ROWS} data rows"
        ));
    }

    let columns = selected_columns
        .into_iter()
        .map(|ordinal| {
            analyze_column(
                ordinal,
                &headers[ordinal],
                &rows[..analyzed_row_count],
                multiplier,
            )
        })
        .collect();
    let groups = summarize_groups(
        &headers,
        &rows[..analyzed_row_count],
        request.group_by_column,
    )?;
    let correlations = summarize_correlations(
        &headers,
        &rows[..analyzed_row_count],
        &request.correlation_columns,
    )?;

    Ok(DatasetAnalysis {
        dataset_id: None,
        source_sha256: None,
        selected_sheet: None,
        row_count: rows.len(),
        analyzed_row_count,
        excluded_row_count: rows.len() - analyzed_row_count,
        columns,
        groups,
        correlations,
        warnings,
    })
}

fn selected_columns(headers: &[String], requested: &[usize]) -> Result<Vec<usize>, String> {
    if requested.is_empty() {
        return Ok((0..headers.len()).collect());
    }
    let mut selected = Vec::with_capacity(requested.len());
    for &ordinal in requested {
        if ordinal >= headers.len() {
            return Err(format!("dataset analysis column {ordinal} is out of range"));
        }
        if selected.contains(&ordinal) {
            return Err(format!("dataset analysis column {ordinal} is duplicated"));
        }
        selected.push(ordinal);
    }
    Ok(selected)
}

fn analyze_column(
    ordinal: usize,
    name: &str,
    rows: &[Vec<String>],
    multiplier: f64,
) -> DatasetColumnAnalysis {
    let raw_values = rows
        .iter()
        .map(|row| row.get(ordinal).map(String::as_str).unwrap_or(""))
        .map(str::trim)
        .collect::<Vec<_>>();
    let missing_count = raw_values.iter().filter(|value| value.is_empty()).count();
    let non_empty = raw_values
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_empty())
        .collect::<Vec<_>>();
    let numeric_column = non_empty
        .iter()
        .any(|(_, value)| parse_number(value).is_some());

    if numeric_column {
        analyze_numeric_column(ordinal, name, raw_values, missing_count, multiplier)
    } else {
        analyze_text_column(ordinal, name, raw_values, missing_count)
    }
}

fn analyze_numeric_column(
    ordinal: usize,
    name: &str,
    raw_values: Vec<&str>,
    missing_count: usize,
    multiplier: f64,
) -> DatasetColumnAnalysis {
    let mut valid = Vec::new();
    let mut invalid_count = 0;
    let mut frequencies = BTreeMap::new();
    for (index, value) in raw_values.iter().enumerate() {
        if value.is_empty() {
            continue;
        }
        match parse_number(value) {
            Some(number) => {
                valid.push((index, number));
                *frequencies.entry(value.to_string()).or_insert(0) += 1;
            }
            None => invalid_count += 1,
        }
    }

    let numbers = valid.iter().map(|(_, number)| *number).collect::<Vec<_>>();
    let mut sorted = numbers.clone();
    sorted.sort_by(f64::total_cmp);
    let (mean, standard_deviation) = moments(&numbers);
    let percentile_25 = percentile(&sorted, 0.25);
    let median = percentile(&sorted, 0.5);
    let percentile_75 = percentile(&sorted, 0.75);
    let (outlier_count, outlier_rows) =
        outliers(&valid, percentile_25, median, percentile_75, multiplier);

    DatasetColumnAnalysis {
        ordinal,
        name: name.to_string(),
        canonical_field: None,
        unit: None,
        inferred_type: "number".to_string(),
        sample_count: numbers.len(),
        missing_count,
        invalid_count,
        missing_rate: rate(missing_count, raw_values.len()),
        distinct_count: frequencies.len(),
        mean,
        standard_deviation,
        min: sorted.first().copied(),
        percentile_25,
        median,
        percentile_75,
        max: sorted.last().copied(),
        outlier_count,
        outlier_rows,
        top_values: Vec::new(),
    }
}

fn summarize_groups(
    headers: &[String],
    rows: &[Vec<String>],
    group_by_column: Option<usize>,
) -> Result<Vec<DatasetGroupSummary>, String> {
    let Some(group_by_column) = group_by_column else {
        return Ok(Vec::new());
    };
    if group_by_column >= headers.len() {
        return Err(format!("group-by column {group_by_column} is out of range"));
    }

    let numeric_columns = (0..headers.len())
        .filter(|ordinal| *ordinal != group_by_column)
        .filter(|ordinal| has_numeric_value(rows, *ordinal))
        .collect::<Vec<_>>();
    let mut grouped = BTreeMap::<String, (usize, Vec<(usize, Vec<f64>)>)>::new();
    for row in rows {
        let key = row
            .get(group_by_column)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("(missing)")
            .to_string();
        if grouped.len() == MAX_GROUPS && !grouped.contains_key(&key) {
            return Err(format!(
                "group-by column has more than {MAX_GROUPS} distinct values"
            ));
        }
        let entry = grouped.entry(key).or_insert_with(|| {
            (
                0,
                (0..numeric_columns.len())
                    .map(|_| (0, Vec::new()))
                    .collect(),
            )
        });
        entry.0 += 1;
        for (position, ordinal) in numeric_columns.iter().copied().enumerate() {
            if let Some(number) = row
                .get(ordinal)
                .and_then(|value| parse_number(value.trim()))
            {
                entry.1[position].0 += 1;
                entry.1[position].1.push(number);
            }
        }
    }

    Ok(grouped
        .into_iter()
        .map(|(key, (row_count, aggregates))| DatasetGroupSummary {
            key,
            row_count,
            columns: numeric_columns
                .iter()
                .copied()
                .zip(aggregates)
                .map(|(ordinal, (sample_count, values))| {
                    let min = values.iter().copied().reduce(f64::min);
                    let max = values.iter().copied().reduce(f64::max);
                    DatasetGroupColumnSummary {
                        ordinal,
                        sample_count,
                        mean: if values.is_empty() {
                            None
                        } else {
                            Some(values.iter().sum::<f64>() / values.len() as f64)
                        },
                        min,
                        max,
                    }
                })
                .collect(),
        })
        .collect())
}

fn summarize_correlations(
    headers: &[String],
    rows: &[Vec<String>],
    requested_columns: &[usize],
) -> Result<Vec<DatasetCorrelation>, String> {
    if requested_columns.is_empty() {
        return Ok(Vec::new());
    }
    if requested_columns.len() > MAX_CORRELATION_COLUMNS {
        return Err(format!(
            "correlation analysis supports at most {MAX_CORRELATION_COLUMNS} columns"
        ));
    }
    let mut columns = Vec::with_capacity(requested_columns.len());
    for &ordinal in requested_columns {
        if ordinal >= headers.len() {
            return Err(format!("correlation column {ordinal} is out of range"));
        }
        if columns.contains(&ordinal) {
            return Err(format!("correlation column {ordinal} is duplicated"));
        }
        if !has_numeric_value(rows, ordinal) {
            return Err(format!(
                "correlation column {} ({}) must contain numeric values",
                ordinal, headers[ordinal]
            ));
        }
        columns.push(ordinal);
    }

    let mut correlations = Vec::new();
    for (left_position, &left_ordinal) in columns.iter().enumerate() {
        for &right_ordinal in columns.iter().skip(left_position + 1) {
            let pairs = rows
                .iter()
                .filter_map(|row| {
                    let left = row
                        .get(left_ordinal)
                        .and_then(|value| parse_number(value.trim()))?;
                    let right = row
                        .get(right_ordinal)
                        .and_then(|value| parse_number(value.trim()))?;
                    Some((left, right))
                })
                .collect::<Vec<_>>();
            correlations.push(DatasetCorrelation {
                left_ordinal,
                right_ordinal,
                sample_count: pairs.len(),
                pearson: pearson(&pairs),
            });
        }
    }
    Ok(correlations)
}

fn has_numeric_value(rows: &[Vec<String>], ordinal: usize) -> bool {
    rows.iter()
        .filter_map(|row| row.get(ordinal))
        .any(|value| parse_number(value.trim()).is_some())
}

fn parse_number(value: &str) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

fn pearson(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.len() < 2 {
        return None;
    }
    let count = pairs.len() as f64;
    let (sum_x, sum_y) = pairs
        .iter()
        .fold((0.0, 0.0), |(x, y), (left, right)| (x + left, y + right));
    let (sum_xx, sum_yy, sum_xy) = pairs.iter().fold((0.0, 0.0, 0.0), |acc, (left, right)| {
        (
            acc.0 + left * left,
            acc.1 + right * right,
            acc.2 + left * right,
        )
    });
    let covariance = count * sum_xy - sum_x * sum_y;
    let variance_x = count * sum_xx - sum_x * sum_x;
    let variance_y = count * sum_yy - sum_y * sum_y;
    let denominator = (variance_x * variance_y).sqrt();
    if denominator <= f64::EPSILON {
        None
    } else {
        Some((covariance / denominator).clamp(-1.0, 1.0))
    }
}

fn analyze_text_column(
    ordinal: usize,
    name: &str,
    raw_values: Vec<&str>,
    missing_count: usize,
) -> DatasetColumnAnalysis {
    let mut frequencies = BTreeMap::new();
    for value in raw_values.iter().filter(|value| !value.is_empty()) {
        *frequencies.entry(value.to_string()).or_insert(0) += 1;
    }
    let mut top_values = frequencies
        .iter()
        .map(|(value, count)| DatasetValueFrequency {
            value: value.clone(),
            count: *count,
        })
        .collect::<Vec<_>>();
    top_values.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.value.cmp(&right.value))
    });
    top_values.truncate(MAX_TOP_VALUES);

    DatasetColumnAnalysis {
        ordinal,
        name: name.to_string(),
        canonical_field: None,
        unit: None,
        inferred_type: "text".to_string(),
        sample_count: raw_values.len() - missing_count,
        missing_count,
        invalid_count: 0,
        missing_rate: rate(missing_count, raw_values.len()),
        distinct_count: frequencies.len(),
        mean: None,
        standard_deviation: None,
        min: None,
        percentile_25: None,
        median: None,
        percentile_75: None,
        max: None,
        outlier_count: 0,
        outlier_rows: Vec::new(),
        top_values,
    }
}

fn moments(values: &[f64]) -> (Option<f64>, Option<f64>) {
    if values.is_empty() {
        return (None, None);
    }
    let mut mean = 0.0;
    let mut squared_delta = 0.0;
    for (index, value) in values.iter().copied().enumerate() {
        let count = (index + 1) as f64;
        let delta = value - mean;
        mean += delta / count;
        squared_delta += delta * (value - mean);
    }
    (
        Some(mean),
        Some((squared_delta / values.len() as f64).sqrt()),
    )
}

fn percentile(sorted: &[f64], fraction: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let position = fraction * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return Some(sorted[lower]);
    }
    let weight = position - lower as f64;
    Some(sorted[lower] + (sorted[upper] - sorted[lower]) * weight)
}

fn outliers(
    values: &[(usize, f64)],
    percentile_25: Option<f64>,
    median: Option<f64>,
    percentile_75: Option<f64>,
    multiplier: f64,
) -> (usize, Vec<usize>) {
    let (Some(percentile_25), Some(median), Some(percentile_75)) =
        (percentile_25, median, percentile_75)
    else {
        return (0, Vec::new());
    };
    if values.len() < 4 {
        return (0, Vec::new());
    }
    let iqr = percentile_75 - percentile_25;
    let mut rows = Vec::new();
    for (index, value) in values {
        let outlier = if iqr == 0.0 {
            *value != median
        } else {
            *value < percentile_25 - multiplier * iqr || *value > percentile_75 + multiplier * iqr
        };
        if outlier {
            if rows.len() < MAX_OUTLIER_EVIDENCE {
                rows.push(index + 2);
            }
        }
    }
    let count = values
        .iter()
        .filter(|(_, value)| {
            if iqr == 0.0 {
                *value != median
            } else {
                *value < percentile_25 - multiplier * iqr
                    || *value > percentile_75 + multiplier * iqr
            }
        })
        .count();
    (count, rows)
}

fn rate(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}
