//! 数据集数值序列（第 32 章：折线图与散点图的数据来源）。
//!
//! 分析结果只提供统计量，不提供原始行值；折线图与散点图需要按行读取数值。
//! 本模块按需返回被选列的行值序列，并对超大数据集做等距抽样，避免把整表
//! 传到前端。抽样是等距的，因此折线图仍能反映整体走势。

use super::analysis::parse_number;
use serde::{Deserialize, Serialize};

/// 单次返回的最大行数。超过时按等距抽样并标记 `sampled`。
pub const MAX_SERIES_ROWS: usize = 5_000;
/// 单次最多返回的列数，避免载荷过大。
pub const MAX_SERIES_COLUMNS: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetSeriesRequest {
    /// 要取值的列序号；空表示由调用方另行指定。
    #[serde(default)]
    pub columns: Vec<usize>,
    #[serde(default)]
    pub max_rows: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetSeriesColumn {
    pub ordinal: usize,
    pub name: String,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetSeries {
    pub columns: Vec<DatasetSeriesColumn>,
    /// 每行按 `columns` 顺序给出取值；缺失或非法为 `null`。
    pub rows: Vec<Vec<Option<f64>>>,
    pub total_rows: usize,
    pub sampled: bool,
}

/// 按请求列抽取数值序列。
pub fn dataset_series(
    headers: &[String],
    rows: &[Vec<String>],
    request: &DatasetSeriesRequest,
) -> Result<DatasetSeries, String> {
    if headers.is_empty() {
        return Err("dataset series requires at least one column".to_string());
    }
    if request.columns.is_empty() {
        return Err("dataset series requires at least one selected column".to_string());
    }
    if request.columns.len() > MAX_SERIES_COLUMNS {
        return Err(format!(
            "dataset series supports at most {MAX_SERIES_COLUMNS} columns per request"
        ));
    }

    let mut seen = std::collections::BTreeSet::new();
    let mut columns = Vec::with_capacity(request.columns.len());
    for ordinal in &request.columns {
        if *ordinal >= headers.len() {
            return Err("dataset series column is out of range".to_string());
        }
        if !seen.insert(*ordinal) {
            return Err("dataset series column is duplicated".to_string());
        }
        columns.push(DatasetSeriesColumn {
            ordinal: *ordinal,
            name: headers[*ordinal].clone(),
            unit: None,
        });
    }

    let limit = request
        .max_rows
        .unwrap_or(MAX_SERIES_ROWS)
        .clamp(1, MAX_SERIES_ROWS);
    let total_rows = rows.len();
    let sampled = total_rows > limit;
    let indices: Vec<usize> = if sampled {
        // 等距抽样：保留首尾并均匀取点，折线走势不失真。
        let step = total_rows as f64 / limit as f64;
        (0..limit)
            .map(|position| ((position as f64) * step).floor() as usize)
            .filter(|index| *index < total_rows)
            .collect()
    } else {
        (0..total_rows).collect()
    };

    let series_rows = indices
        .iter()
        .map(|row_index| {
            columns
                .iter()
                .map(|column| {
                    rows[*row_index]
                        .get(column.ordinal)
                        .and_then(|value| parse_number(value))
                })
                .collect()
        })
        .collect();

    Ok(DatasetSeries {
        columns,
        rows: series_rows,
        total_rows,
        sampled,
    })
}

/// 把原始字符串行转为数值矩阵（缺失或非法为 `None`），供多元分析使用。
///
/// 与 `dataset_series` 不同，这里不做抽样也不做列选择：调用方需要完整的
/// 完整样本计数，抽样会改变统计结果。
pub fn numeric_matrix(rows: &[Vec<String>]) -> Vec<Vec<Option<f64>>> {
    rows.iter()
        .map(|row| row.iter().map(|value| parse_number(value)).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers() -> Vec<String> {
        vec!["C".to_string(), "Mn".to_string(), "grade".to_string()]
    }

    fn rows() -> Vec<Vec<String>> {
        vec![
            vec!["0.1".into(), "1.0".into(), "Q355B".into()],
            vec!["0.2".into(), "".into(), "Q355B".into()],
            vec!["0.3".into(), "1.4".into(), "Q690".into()],
        ]
    }

    #[test]
    fn returns_selected_numeric_columns_in_row_order() {
        let series = dataset_series(
            &headers(),
            &rows(),
            &DatasetSeriesRequest {
                columns: vec![0, 1],
                max_rows: None,
            },
        )
        .expect("build series");

        assert_eq!(series.total_rows, 3);
        assert!(!series.sampled);
        assert_eq!(series.columns.len(), 2);
        assert_eq!(series.rows[0], vec![Some(0.1), Some(1.0)]);
        // 缺失值保留为 null，不填 0，避免折线图出现虚假的零点。
        assert_eq!(series.rows[1], vec![Some(0.2), None]);
    }

    #[test]
    fn samples_large_datasets_and_keeps_first_row() {
        let big: Vec<Vec<String>> = (0..1_000)
            .map(|index| vec![index.to_string(), index.to_string(), "x".into()])
            .collect();
        let series = dataset_series(
            &headers(),
            &big,
            &DatasetSeriesRequest {
                columns: vec![0],
                max_rows: Some(10),
            },
        )
        .expect("build sampled series");

        assert!(series.sampled);
        assert_eq!(series.total_rows, 1_000);
        assert_eq!(series.rows.len(), 10);
        assert_eq!(series.rows[0][0], Some(0.0));
    }

    #[test]
    fn rejects_out_of_range_and_duplicate_columns() {
        assert!(dataset_series(
            &headers(),
            &rows(),
            &DatasetSeriesRequest {
                columns: vec![9],
                max_rows: None,
            }
        )
        .is_err());
        assert!(dataset_series(
            &headers(),
            &rows(),
            &DatasetSeriesRequest {
                columns: vec![0, 0],
                max_rows: None,
            }
        )
        .is_err());
        assert!(dataset_series(
            &headers(),
            &rows(),
            &DatasetSeriesRequest {
                columns: vec![],
                max_rows: None,
            }
        )
        .is_err());
    }
}
