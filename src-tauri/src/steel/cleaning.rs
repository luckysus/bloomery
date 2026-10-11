//! 数据清洗（第 31 章）：重复行、缺失值与异常值的处理。
//!
//! 输入是数据集原始表格（字符串单元格），输出是清洗后的表格与统计摘要。
//! 缺失值用**该列均值**填充、异常值用 **IQR 规则**判定，两者都只作用于
//! 请求里选中的列；未选中任何列时默认覆盖全部列。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

fn default_missing_strategy() -> String {
    "keep".to_string()
}

fn default_outlier_strategy() -> String {
    "keep".to_string()
}

fn default_iqr_multiplier() -> f64 {
    1.5
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetCleaningPlan {
    #[serde(default)]
    pub drop_duplicate_rows: bool,
    #[serde(default = "default_missing_strategy")]
    pub missing_strategy: String,
    #[serde(default = "default_outlier_strategy")]
    pub outlier_strategy: String,
    #[serde(default = "default_iqr_multiplier")]
    pub outlier_iqr_multiplier: f64,
    #[serde(default)]
    pub columns: Vec<usize>,
}

impl Default for DatasetCleaningPlan {
    fn default() -> Self {
        Self {
            drop_duplicate_rows: false,
            missing_strategy: default_missing_strategy(),
            outlier_strategy: default_outlier_strategy(),
            outlier_iqr_multiplier: default_iqr_multiplier(),
            columns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetCleaningSummary {
    pub row_count_before: usize,
    pub row_count_after: usize,
    pub duplicate_rows: usize,
    pub duplicate_rows_removed: usize,
    pub missing_cells: usize,
    pub missing_rows_removed: usize,
    pub filled_cells: usize,
    pub outlier_cells: usize,
    pub outlier_rows_removed: usize,
    pub clipped_cells: usize,
}

fn parse_number(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|item| item.is_finite())
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        let rendered = format!("{value:.6}");
        rendered
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn percentile(sorted: &[f64], fraction: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    if sorted.len() == 1 {
        return sorted.first().copied();
    }
    let position = fraction.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return sorted.get(lower).copied();
    }
    let weight = position - lower as f64;
    Some(sorted[lower] * (1.0 - weight) + sorted[upper] * weight)
}

fn iqr_bounds(rows: &[Vec<String>], ordinal: usize, multiplier: f64) -> Option<(f64, f64)> {
    let mut values: Vec<f64> = rows
        .iter()
        .filter_map(|row| row.get(ordinal).and_then(|value| parse_number(value)))
        .collect();
    if values.len() < 4 {
        return None;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let q1 = percentile(&values, 0.25)?;
    let q3 = percentile(&values, 0.75)?;
    let spread = q3 - q1;
    if spread <= 0.0 {
        return None;
    }
    Some((q1 - multiplier * spread, q3 + multiplier * spread))
}

/// 按计划清洗表格，返回清洗后的行与摘要。
pub fn clean_dataset(
    headers: &[String],
    rows: &[Vec<String>],
    plan: &DatasetCleaningPlan,
) -> Result<(Vec<Vec<String>>, DatasetCleaningSummary), String> {
    if !matches!(
        plan.missing_strategy.as_str(),
        "keep" | "drop_rows" | "fill_mean"
    ) {
        return Err("unsupported missing value strategy".to_string());
    }
    if !matches!(
        plan.outlier_strategy.as_str(),
        "keep" | "drop_rows" | "clip"
    ) {
        return Err("unsupported outlier strategy".to_string());
    }
    if !plan.outlier_iqr_multiplier.is_finite() || plan.outlier_iqr_multiplier < 0.0 {
        return Err("outlier IQR multiplier must be a non-negative number".to_string());
    }
    let selected: Vec<usize> = if plan.columns.is_empty() {
        (0..headers.len()).collect()
    } else {
        let mut columns = plan.columns.clone();
        columns.sort_unstable();
        columns.dedup();
        for ordinal in &columns {
            if *ordinal >= headers.len() {
                return Err(format!("cleaning column {ordinal} is out of range"));
            }
        }
        columns
    };

    let mut summary = DatasetCleaningSummary {
        row_count_before: rows.len(),
        ..Default::default()
    };

    // 1) 重复行：整行完全相同才算重复。
    let mut seen: HashSet<String> = HashSet::new();
    let mut deduplicated: Vec<Vec<String>> = Vec::with_capacity(rows.len());
    for row in rows {
        let key = row.join("\u{1f}");
        if !seen.insert(key) {
            summary.duplicate_rows += 1;
            if plan.drop_duplicate_rows {
                summary.duplicate_rows_removed += 1;
                continue;
            }
        }
        deduplicated.push(row.clone());
    }

    // 2) 缺失值。
    let mut fills: Vec<Option<String>> = vec![None; headers.len()];
    if plan.missing_strategy == "fill_mean" {
        for &ordinal in &selected {
            let values: Vec<f64> = deduplicated
                .iter()
                .filter_map(|row| row.get(ordinal).and_then(|value| parse_number(value)))
                .collect();
            if !values.is_empty() {
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                fills[ordinal] = Some(format_number(mean));
            }
        }
    }
    let mut without_missing: Vec<Vec<String>> = Vec::with_capacity(deduplicated.len());
    for mut row in deduplicated {
        let missing: Vec<usize> = selected
            .iter()
            .copied()
            .filter(|ordinal| {
                row.get(*ordinal)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            })
            .collect();
        if !missing.is_empty() {
            summary.missing_cells += missing.len();
            if plan.missing_strategy == "drop_rows" {
                summary.missing_rows_removed += 1;
                continue;
            }
            if plan.missing_strategy == "fill_mean" {
                for ordinal in missing {
                    let Some(value) = fills[ordinal].clone() else {
                        continue;
                    };
                    if ordinal < row.len() {
                        row[ordinal] = value;
                    } else {
                        while row.len() < ordinal {
                            row.push(String::new());
                        }
                        row.push(value);
                    }
                    summary.filled_cells += 1;
                }
            }
        }
        without_missing.push(row);
    }

    // 3) 异常值：按 IQR 规则判定，可剔除整行或截断到边界。
    let bounds: Vec<Option<(f64, f64)>> = (0..headers.len())
        .map(|ordinal| {
            if selected.contains(&ordinal) {
                iqr_bounds(&without_missing, ordinal, plan.outlier_iqr_multiplier)
            } else {
                None
            }
        })
        .collect();
    let mut cleaned: Vec<Vec<String>> = Vec::with_capacity(without_missing.len());
    for mut row in without_missing {
        let outliers: Vec<usize> = selected
            .iter()
            .copied()
            .filter(|ordinal| {
                let Some(Some((lower, upper))) = bounds.get(*ordinal) else {
                    return false;
                };
                match row.get(*ordinal).and_then(|value| parse_number(value)) {
                    Some(number) => number < *lower || number > *upper,
                    None => false,
                }
            })
            .collect();
        if !outliers.is_empty() {
            summary.outlier_cells += outliers.len();
            if plan.outlier_strategy == "drop_rows" {
                summary.outlier_rows_removed += 1;
                continue;
            }
            if plan.outlier_strategy == "clip" {
                for ordinal in outliers {
                    let Some(Some((lower, upper))) = bounds.get(ordinal) else {
                        continue;
                    };
                    let Some(number) = row.get(ordinal).and_then(|value| parse_number(value))
                    else {
                        continue;
                    };
                    let clamped = number.max(*lower).min(*upper);
                    if (clamped - number).abs() > f64::EPSILON {
                        row[ordinal] = format_number(clamped);
                        summary.clipped_cells += 1;
                    }
                }
            }
        }
        cleaned.push(row);
    }

    summary.row_count_after = cleaned.len();
    Ok((cleaned, summary))
}

/// 把表格序列化成 CSV（RFC 4180 转义），供清洗结果落盘复用。
pub fn to_csv(headers: &[String], rows: &[Vec<String>]) -> String {
    fn escape(value: &str) -> String {
        if value.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        }
    }
    let mut output = String::new();
    output.push_str(
        &headers
            .iter()
            .map(|value| escape(value))
            .collect::<Vec<_>>()
            .join(","),
    );
    output.push('\n');
    for row in rows {
        output.push_str(
            &row.iter()
                .map(|value| escape(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers() -> Vec<String> {
        vec!["temperature".to_string(), "carbon".to_string()]
    }

    #[test]
    fn duplicate_rows_are_counted_and_optional() {
        let rows = vec![
            vec!["900".to_string(), "0.2".to_string()],
            vec!["900".to_string(), "0.2".to_string()],
            vec!["950".to_string(), "0.3".to_string()],
        ];
        let plan = DatasetCleaningPlan {
            drop_duplicate_rows: true,
            ..Default::default()
        };
        let (cleaned, summary) = clean_dataset(&headers(), &rows, &plan).unwrap();
        assert_eq!(summary.duplicate_rows, 1);
        assert_eq!(summary.duplicate_rows_removed, 1);
        assert_eq!(cleaned.len(), 2);

        let keep = DatasetCleaningPlan::default();
        let (cleaned, summary) = clean_dataset(&headers(), &rows, &keep).unwrap();
        assert_eq!(summary.duplicate_rows, 1);
        assert_eq!(cleaned.len(), 3);
    }

    #[test]
    fn missing_values_can_be_dropped_or_filled_with_column_mean() {
        let rows = vec![
            vec!["900".to_string(), "0.2".to_string()],
            vec!["".to_string(), "0.4".to_string()],
            vec!["1000".to_string(), "".to_string()],
        ];
        let drop = DatasetCleaningPlan {
            missing_strategy: "drop_rows".to_string(),
            ..Default::default()
        };
        let (cleaned, summary) = clean_dataset(&headers(), &rows, &drop).unwrap();
        assert_eq!(summary.missing_cells, 2);
        assert_eq!(summary.missing_rows_removed, 2);
        assert_eq!(cleaned.len(), 1);

        let fill = DatasetCleaningPlan {
            missing_strategy: "fill_mean".to_string(),
            ..Default::default()
        };
        let (cleaned, summary) = clean_dataset(&headers(), &rows, &fill).unwrap();
        assert_eq!(summary.filled_cells, 2);
        assert_eq!(cleaned.len(), 3);
        assert_eq!(cleaned[1][0], "950");
        assert_eq!(cleaned[2][1], "0.3");
    }

    #[test]
    fn outliers_follow_the_iqr_rule_and_can_be_clipped() {
        let rows = (0..20)
            .map(|index| vec![format!("{}", 100 + index), format!("{}", index)])
            .chain(std::iter::once(vec!["100000".to_string(), "1".to_string()]))
            .collect::<Vec<_>>();
        let plan = DatasetCleaningPlan {
            outlier_strategy: "clip".to_string(),
            ..Default::default()
        };
        let (cleaned, summary) = clean_dataset(&headers(), &rows, &plan).unwrap();
        assert_eq!(cleaned.len(), 21);
        assert!(summary.clipped_cells >= 1);
        assert!(cleaned[20][0].parse::<f64>().unwrap() < 100_000.0);

        let drop = DatasetCleaningPlan {
            outlier_strategy: "drop_rows".to_string(),
            ..Default::default()
        };
        let (cleaned, summary) = clean_dataset(&headers(), &rows, &drop).unwrap();
        assert!(summary.outlier_rows_removed >= 1);
        assert!(cleaned.len() < 21);
    }

    #[test]
    fn unsupported_strategies_and_columns_are_rejected() {
        let rows = vec![vec!["1".to_string(), "2".to_string()]];
        assert!(clean_dataset(
            &headers(),
            &rows,
            &DatasetCleaningPlan {
                missing_strategy: "magic".to_string(),
                ..Default::default()
            }
        )
        .is_err());
        assert!(clean_dataset(
            &headers(),
            &rows,
            &DatasetCleaningPlan {
                columns: vec![9],
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn csv_export_escapes_separators_and_quotes() {
        let csv = to_csv(
            &["a".to_string(), "b".to_string()],
            &[vec!["x,y".to_string(), "he said \"hi\"".to_string()]],
        );
        assert_eq!(csv, "a,b\n\"x,y\",\"he said \"\"hi\"\"\"\n");
    }
}
