//! 多元分析：PCA 主成分分析与 k-means 聚类（第 31/32 章）。
//!
//! 全部为纯 Rust 实现，不引入新的数值依赖，便于用 `cargo test` 验证。
//! 两者都只使用**完整样本**（所选列全部有值）的行，缺失行会被计数并回报，
//! 不做插补——插补会掩盖数据质量问题。

use serde::{Deserialize, Serialize};

/// 参与计算的最大行数，避免超大数据集拖垮交互。
pub const MAX_MULTIVARIATE_ROWS: usize = 20_000;
/// 参与计算的最大列数。
pub const MAX_MULTIVARIATE_COLUMNS: usize = 12;
/// 幂迭代的最大轮数。
const POWER_ITERATIONS: usize = 200;
/// k-means 的最大迭代轮数。
const KMEANS_ITERATIONS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MultivariateRequest {
    pub columns: Vec<usize>,
    #[serde(default)]
    pub components: Option<usize>,
    #[serde(default)]
    pub clusters: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PcaComponent {
    /// 各原始列在该主成分上的载荷。
    pub loadings: Vec<f64>,
    /// 该主成分解释的方差比例。
    pub explained_variance_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PcaResult {
    pub components: Vec<PcaComponent>,
    /// 前两个主成分上的投影坐标，用于散点图。
    pub scores: Vec<Vec<f64>>,
    /// 各列标准化时使用的均值与标准差，便于解释载荷。
    pub means: Vec<f64>,
    pub standard_deviations: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KMeansResult {
    pub labels: Vec<usize>,
    pub centroids: Vec<Vec<f64>>,
    pub inertia: f64,
    pub iterations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MultivariateResult {
    pub column_ordinals: Vec<usize>,
    pub sample_count: usize,
    /// 因所选列存在缺失而被排除的行数。
    pub excluded_row_count: usize,
    pub pca: PcaResult,
    pub clusters: KMeansResult,
}

/// 确定性线性同余发生器：保证同样的输入得到同样的聚类结果。
struct Lcg(u64);

impl Lcg {
    fn next_unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

/// 抽取完整样本矩阵（仅保留所选列全部有值的行）。
fn complete_case_matrix(
    rows: &[Vec<Option<f64>>],
    columns: &[usize],
) -> (Vec<Vec<f64>>, usize) {
    let mut matrix = Vec::new();
    let mut excluded = 0usize;
    for row in rows {
        let mut values = Vec::with_capacity(columns.len());
        let mut complete = true;
        for ordinal in columns {
            match row.get(*ordinal).and_then(|value| *value) {
                Some(value) if value.is_finite() => values.push(value),
                _ => {
                    complete = false;
                    break;
                }
            }
        }
        if complete {
            matrix.push(values);
        } else {
            excluded += 1;
        }
    }
    (matrix, excluded)
}

/// 对矩阵做按列标准化（减均值、除标准差），返回标准化矩阵与原始均值/标准差。
fn standardize(matrix: &[Vec<f64>], column_count: usize) -> (Vec<Vec<f64>>, Vec<f64>, Vec<f64>) {
    let row_count = matrix.len();
    let mut means = vec![0.0; column_count];
    for row in matrix {
        for (index, value) in row.iter().enumerate() {
            means[index] += value;
        }
    }
    for mean in &mut means {
        *mean /= row_count as f64;
    }
    let mut deviations = vec![0.0; column_count];
    for row in matrix {
        for (index, value) in row.iter().enumerate() {
            deviations[index] += (value - means[index]).powi(2);
        }
    }
    // 方差为 0 的列除以 1，避免产生 NaN；该列在载荷中自然为 0。
    let scales: Vec<f64> = deviations
        .iter()
        .map(|value| {
            let variance = value / row_count as f64;
            if variance > 1e-12 {
                variance.sqrt()
            } else {
                1.0
            }
        })
        .collect();

    let normalized = matrix
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(index, value)| (value - means[index]) / scales[index])
                .collect()
        })
        .collect();
    (normalized, means, scales)
}

/// 计算 d×d 协方差矩阵。
fn covariance(matrix: &[Vec<f64>], column_count: usize) -> Vec<Vec<f64>> {
    let row_count = matrix.len();
    let mut covariance = vec![vec![0.0; column_count]; column_count];
    for row in matrix {
        for left in 0..column_count {
            for right in left..column_count {
                covariance[left][right] += row[left] * row[right];
            }
        }
    }
    for left in 0..column_count {
        for right in left..column_count {
            let value = covariance[left][right] / row_count as f64;
            covariance[left][right] = value;
            covariance[right][left] = value;
        }
    }
    covariance
}

/// 幂迭代求最大特征值对应的特征向量。
fn power_iteration(matrix: &[Vec<f64>], dimension: usize, seed: u64) -> (Vec<f64>, f64) {
    let mut rng = Lcg(seed);
    let mut vector: Vec<f64> = (0..dimension).map(|_| rng.next_unit() - 0.5).collect();
    normalize(&mut vector);
    let mut eigenvalue = 0.0;

    for _ in 0..POWER_ITERATIONS {
        let mut next = vec![0.0; dimension];
        for row in 0..dimension {
            let mut sum = 0.0;
            for column in 0..dimension {
                sum += matrix[row][column] * vector[column];
            }
            next[row] = sum;
        }
        let norm = normalize(&mut next);
        if norm < 1e-12 {
            break;
        }
        let shift = next
            .iter()
            .zip(vector.iter())
            .map(|(left, right)| (left - right).abs())
            .fold(0.0_f64, f64::max);
        vector = next;
        eigenvalue = norm;
        if shift < 1e-10 {
            break;
        }
    }
    (vector, eigenvalue)
}

fn normalize(vector: &mut [f64]) -> f64 {
    let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm > 1e-12 {
        for value in vector.iter_mut() {
            *value /= norm;
        }
    }
    norm
}

/// 从协方差矩阵提取前 k 个主成分（幂迭代 + 收缩 deflation）。
fn principal_components(
    covariance: &[Vec<f64>],
    dimension: usize,
    count: usize,
) -> Vec<(Vec<f64>, f64)> {
    let mut residual = covariance.to_vec();
    let mut components = Vec::with_capacity(count);
    for index in 0..count {
        let (vector, eigenvalue) = power_iteration(&residual, dimension, 9_781 + index as u64 * 31);
        // deflation：去掉该方向上已解释的方差。
        for row in 0..dimension {
            for column in 0..dimension {
                residual[row][column] -= eigenvalue * vector[row] * vector[column];
            }
        }
        components.push((vector, eigenvalue.max(0.0)));
    }
    components
}

/// k-means++ 初始化。
fn init_centroids(matrix: &[Vec<f64>], k: usize, seed: u64) -> Vec<Vec<f64>> {
    let mut rng = Lcg(seed);
    let mut centroids = vec![matrix[0].clone()];
    while centroids.len() < k {
        let distances: Vec<f64> = matrix
            .iter()
            .map(|row| {
                centroids
                    .iter()
                    .map(|centroid| squared_distance(row, centroid))
                    .fold(f64::INFINITY, f64::min)
            })
            .collect();
        let total: f64 = distances.iter().sum();
        if total <= 0.0 {
            // 所有点重合：补一个与首点相同的中心即可，不引入随机噪声。
            centroids.push(matrix[0].clone());
            continue;
        }
        let mut target = rng.next_unit() * total;
        let mut picked = matrix.len() - 1;
        for (index, distance) in distances.iter().enumerate() {
            target -= distance;
            if target <= 0.0 {
                picked = index;
                break;
            }
        }
        centroids.push(matrix[picked].clone());
    }
    centroids
}

fn squared_distance(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum()
}

/// Lloyd 迭代。
fn kmeans(matrix: &[Vec<f64>], k: usize, seed: u64) -> KMeansResult {
    let dimension = matrix[0].len();
    let mut centroids = init_centroids(matrix, k, seed);
    let mut labels = vec![0usize; matrix.len()];
    let mut iterations = 0usize;

    for round in 0..KMEANS_ITERATIONS {
        iterations = round + 1;
        let mut changed = false;
        for (index, row) in matrix.iter().enumerate() {
            let mut best = 0usize;
            let mut best_distance = f64::INFINITY;
            for (cluster, centroid) in centroids.iter().enumerate() {
                let distance = squared_distance(row, centroid);
                if distance < best_distance {
                    best_distance = distance;
                    best = cluster;
                }
            }
            if labels[index] != best {
                labels[index] = best;
                changed = true;
            }
        }

        let mut sums = vec![vec![0.0; dimension]; k];
        let mut counts = vec![0usize; k];
        for (index, row) in matrix.iter().enumerate() {
            let cluster = labels[index];
            counts[cluster] += 1;
            for (position, value) in row.iter().enumerate() {
                sums[cluster][position] += value;
            }
        }
        for cluster in 0..k {
            if counts[cluster] == 0 {
                // 空簇保持原中心，不重新播种，保证结果可复现。
                continue;
            }
            for position in 0..dimension {
                centroids[cluster][position] = sums[cluster][position] / counts[cluster] as f64;
            }
        }
        if !changed {
            break;
        }
    }

    let inertia = matrix
        .iter()
        .enumerate()
        .map(|(index, row)| squared_distance(row, &centroids[labels[index]]))
        .sum();

    KMeansResult { labels, centroids, inertia, iterations }
}

/// PCA + k-means 主入口。
pub fn multivariate_analysis(
    rows: &[Vec<Option<f64>>],
    request: &MultivariateRequest,
) -> Result<MultivariateResult, String> {
    if request.columns.len() < 2 {
        return Err("multivariate analysis requires at least two columns".to_string());
    }
    if request.columns.len() > MAX_MULTIVARIATE_COLUMNS {
        return Err(format!(
            "multivariate analysis supports at most {MAX_MULTIVARIATE_COLUMNS} columns"
        ));
    }
    let (mut matrix, mut excluded) = complete_case_matrix(rows, &request.columns);
    if matrix.len() > MAX_MULTIVARIATE_ROWS {
        // 超出预算的行被截断，并计入排除数以保持计数诚实。
        excluded += matrix.len() - MAX_MULTIVARIATE_ROWS;
        matrix.truncate(MAX_MULTIVARIATE_ROWS);
    }
    if matrix.len() < 3 {
        return Err(
            "multivariate analysis requires at least three complete rows".to_string(),
        );
    }

    let dimension = request.columns.len();
    let (normalized, means, scales) = standardize(&matrix, dimension);
    let covariance = covariance(&normalized, dimension);
    let total_variance: f64 = (0..dimension).map(|index| covariance[index][index]).sum();

    let component_count = request.components.unwrap_or(2).clamp(1, dimension);
    let raw_components = principal_components(&covariance, dimension, component_count);
    let components = raw_components
        .iter()
        .map(|(loadings, eigenvalue)| PcaComponent {
            loadings: loadings.clone(),
            explained_variance_ratio: if total_variance > 1e-12 {
                eigenvalue / total_variance
            } else {
                0.0
            },
        })
        .collect();

    let scores = normalized
        .iter()
        .map(|row| {
            raw_components
                .iter()
                .map(|(loadings, _)| row.iter().zip(loadings.iter()).map(|(a, b)| a * b).sum())
                .collect()
        })
        .collect();

    let cluster_count = request
        .clusters
        .unwrap_or(3)
        .clamp(1, matrix.len().min(8));
    let clusters = kmeans(&normalized, cluster_count, 20_261);

    Ok(MultivariateResult {
        column_ordinals: request.columns.clone(),
        sample_count: matrix.len(),
        excluded_row_count: excluded,
        pca: PcaResult { components, scores, means, standard_deviations: scales },
        clusters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(columns: Vec<usize>) -> MultivariateRequest {
        MultivariateRequest { columns, components: Some(2), clusters: Some(2) }
    }

    /// 构造两个强相关列：第二列近似第一列的 2 倍。
    fn correlated_rows() -> Vec<Vec<Option<f64>>> {
        (0..20)
            .map(|index| {
                let value = index as f64;
                vec![Some(value), Some(value * 2.0), Some(if index < 10 { 0.0 } else { 10.0 })]
            })
            .collect()
    }

    #[test]
    fn pca_puts_most_variance_on_the_first_component() {
        let result = multivariate_analysis(&correlated_rows(), &request(vec![0, 1])).expect("pca");
        assert_eq!(result.sample_count, 20);
        assert_eq!(result.excluded_row_count, 0);
        assert_eq!(result.pca.components.len(), 2);
        // 两列完全线性相关，第一主成分应解释几乎全部方差。
        assert!(result.pca.components[0].explained_variance_ratio > 0.99);
        assert!(result.pca.components[1].explained_variance_ratio < 0.01);
        assert_eq!(result.pca.scores.len(), 20);
    }

    #[test]
    fn clusters_separate_two_distinct_groups() {
        let result = multivariate_analysis(&correlated_rows(), &request(vec![0, 2])).expect("kmeans");
        assert_eq!(result.clusters.labels.len(), 20);
        assert_eq!(result.clusters.centroids.len(), 2);
        // 第 2 列只有 0 与 10 两个取值，聚类应把它们分开。
        let first = result.clusters.labels[0];
        let last = result.clusters.labels[19];
        assert_ne!(first, last);
        assert!(result.clusters.inertia >= 0.0);
    }

    #[test]
    fn incomplete_rows_are_excluded_not_imputed() {
        let mut rows = correlated_rows();
        rows[0][0] = None;
        rows[1][1] = None;
        let result = multivariate_analysis(&rows, &request(vec![0, 1])).expect("pca");
        assert_eq!(result.sample_count, 18);
        assert_eq!(result.excluded_row_count, 2);
    }

    #[test]
    fn rejects_insufficient_input() {
        assert!(multivariate_analysis(&correlated_rows(), &request(vec![0])).is_err());
        let rows = vec![vec![Some(1.0), Some(2.0)]];
        assert!(multivariate_analysis(&rows, &request(vec![0, 1])).is_err());
    }

    #[test]
    fn same_input_produces_same_clusters() {
        let first = multivariate_analysis(&correlated_rows(), &request(vec![0, 2])).expect("kmeans");
        let second = multivariate_analysis(&correlated_rows(), &request(vec![0, 2])).expect("kmeans");
        assert_eq!(first.clusters.labels, second.clusters.labels);
    }
}
