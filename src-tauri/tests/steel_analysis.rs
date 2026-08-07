use bloomery::steel::{analyze_dataset, DatasetAnalysisRequest, DatasetValueFrequency};

#[test]
fn summarizes_numeric_distribution_and_iqr_outlier_evidence() {
    let result = analyze_dataset(
        vec!["heat_id".to_string(), "temperature".to_string()],
        vec![
            vec!["H-01".to_string(), "10".to_string()],
            vec!["H-02".to_string(), "11".to_string()],
            vec!["H-03".to_string(), "12".to_string()],
            vec!["H-04".to_string(), "13".to_string()],
            vec!["H-05".to_string(), "100".to_string()],
            vec!["H-06".to_string(), String::new()],
        ],
        DatasetAnalysisRequest::default(),
    )
    .expect("analyze dataset");

    assert_eq!(result.row_count, 6);
    assert_eq!(result.analyzed_row_count, 6);
    assert_eq!(result.excluded_row_count, 0);
    let column = &result.columns[1];
    assert_eq!(column.sample_count, 5);
    assert_eq!(column.missing_count, 1);
    assert_eq!(column.invalid_count, 0);
    assert_eq!(column.mean, Some(29.2));
    assert_eq!(column.median, Some(12.0));
    assert_eq!(column.outlier_count, 1);
    assert_eq!(column.outlier_rows, vec![6]);
}

#[test]
fn summarizes_text_frequency_and_numeric_invalid_values() {
    let result = analyze_dataset(
        vec!["grade".to_string(), "speed".to_string()],
        vec![
            vec!["Q355B".to_string(), "10".to_string()],
            vec!["Q355B".to_string(), "bad".to_string()],
            vec!["Q235B".to_string(), String::new()],
            vec![String::new(), "20".to_string()],
        ],
        DatasetAnalysisRequest::default(),
    )
    .expect("analyze dataset");

    let grade = &result.columns[0];
    assert_eq!(grade.sample_count, 3);
    assert_eq!(grade.missing_count, 1);
    assert_eq!(grade.distinct_count, 2);
    assert_eq!(
        grade.top_values,
        vec![
            DatasetValueFrequency {
                value: "Q355B".to_string(),
                count: 2,
            },
            DatasetValueFrequency {
                value: "Q235B".to_string(),
                count: 1,
            },
        ]
    );

    let speed = &result.columns[1];
    assert_eq!(speed.sample_count, 2);
    assert_eq!(speed.missing_count, 1);
    assert_eq!(speed.invalid_count, 1);
}

#[test]
fn rejects_invalid_selection_and_outlier_threshold() {
    let rows = vec![vec!["1".to_string()]];
    let error = analyze_dataset(
        vec!["value".to_string()],
        rows.clone(),
        DatasetAnalysisRequest {
            selected_columns: vec![1],
            ..DatasetAnalysisRequest::default()
        },
    )
    .expect_err("out of range column must fail");
    assert!(error.contains("out of range"));

    let error = analyze_dataset(
        vec!["value".to_string()],
        rows,
        DatasetAnalysisRequest {
            outlier_iqr_multiplier: Some(0.0),
            ..DatasetAnalysisRequest::default()
        },
    )
    .expect_err("non-positive outlier threshold must fail");
    assert!(error.contains("outlier IQR multiplier"));
}

#[test]
fn summarizes_numeric_columns_by_group() {
    let result = analyze_dataset(
        vec![
            "grade".to_string(),
            "temperature".to_string(),
            "yield".to_string(),
        ],
        vec![
            vec!["Q355B".to_string(), "10".to_string(), "350".to_string()],
            vec!["Q355B".to_string(), "12".to_string(), "360".to_string()],
            vec!["Q235B".to_string(), "20".to_string(), "300".to_string()],
            vec![String::new(), "8".to_string(), "280".to_string()],
        ],
        DatasetAnalysisRequest {
            group_by_column: Some(0),
            ..DatasetAnalysisRequest::default()
        },
    )
    .expect("group analysis");

    assert_eq!(result.groups.len(), 3);
    assert_eq!(result.groups[0].key, "(missing)");
    assert_eq!(result.groups[1].key, "Q235B");
    assert_eq!(result.groups[2].key, "Q355B");
    assert_eq!(result.groups[2].row_count, 2);
    let temperature = &result.groups[2].columns[0];
    assert_eq!(temperature.ordinal, 1);
    assert_eq!(temperature.sample_count, 2);
    assert_eq!(temperature.mean, Some(11.0));
    assert_eq!(temperature.min, Some(10.0));
    assert_eq!(temperature.max, Some(12.0));
}

#[test]
fn computes_pairwise_pearson_correlations_for_requested_columns() {
    let result = analyze_dataset(
        vec![
            "temperature".to_string(),
            "strength".to_string(),
            "grade".to_string(),
        ],
        vec![
            vec!["1".to_string(), "2".to_string(), "A".to_string()],
            vec!["2".to_string(), "4".to_string(), "B".to_string()],
            vec!["3".to_string(), "6".to_string(), "A".to_string()],
            vec!["4".to_string(), "5".to_string(), "B".to_string()],
        ],
        DatasetAnalysisRequest {
            correlation_columns: vec![0, 1],
            ..DatasetAnalysisRequest::default()
        },
    )
    .expect("correlation analysis");

    assert_eq!(result.correlations.len(), 1);
    let correlation = &result.correlations[0];
    assert_eq!(correlation.left_ordinal, 0);
    assert_eq!(correlation.right_ordinal, 1);
    assert_eq!(correlation.sample_count, 4);
    assert!((correlation.pearson.unwrap() - 0.831521841).abs() < 1e-6);
}

#[test]
fn rejects_invalid_group_and_correlation_columns() {
    let rows = vec![vec!["A".to_string(), "1".to_string()]];
    let error = analyze_dataset(
        vec!["grade".to_string(), "value".to_string()],
        rows.clone(),
        DatasetAnalysisRequest {
            group_by_column: Some(2),
            ..DatasetAnalysisRequest::default()
        },
    )
    .expect_err("invalid group column must fail");
    assert!(error.contains("group-by column 2"));

    let error = analyze_dataset(
        vec!["grade".to_string(), "value".to_string()],
        rows,
        DatasetAnalysisRequest {
            correlation_columns: vec![0, 1],
            ..DatasetAnalysisRequest::default()
        },
    )
    .expect_err("text correlation column must fail");
    assert!(error.contains("numeric"));
}
