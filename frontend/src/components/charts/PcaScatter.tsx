import { CartesianGrid, ResponsiveContainer, Scatter, ScatterChart, Tooltip, XAxis, YAxis, ZAxis } from "recharts";
import type { MultivariateResult } from "../../bridge/desktop";

/**
 * PCA 投影散点图 + k-means 聚类着色（第 31/32 章）。
 *
 * 主成分与聚类标签都由后端计算（`analyze_steel_dataset_multivariate`），
 * 前端只负责按簇着色绘制，不自行做降维或聚类。
 */
const clusterColors = ["#2563eb", "#c64545", "#3f8b75", "#bf7229", "#7f77dd", "#1d9e75", "#a32d2d", "#534ab7"];

export default function PcaScatter({ result }: { result: MultivariateResult }) {
  const first = result.pca.components[0]?.explainedVarianceRatio ?? 0;
  const second = result.pca.components[1]?.explainedVarianceRatio ?? 0;
  const clusterCount = result.clusters.centroids.length;

  const points = result.pca.scores.map((score, index) => ({
    x: score[0] ?? 0,
    y: score[1] ?? 0,
    cluster: result.clusters.labels[index] ?? 0,
  }));

  return (
    <div className="suna-pca">
      <div className="suna-pca-meta">
        <span>有效样本 <strong>{result.sampleCount}</strong></span>
        <span>排除行 <strong>{result.excludedRowCount}</strong></span>
        <span>PC1 解释 <strong>{(first * 100).toFixed(1)}%</strong></span>
        <span>PC2 解释 <strong>{(second * 100).toFixed(1)}%</strong></span>
        <span>聚类 k=<strong>{clusterCount}</strong></span>
        <span>簇内平方和 <strong>{result.clusters.inertia.toFixed(2)}</strong></span>
      </div>
      <ResponsiveContainer width="100%" height={260}>
        <ScatterChart margin={{ top: 12, right: 12, left: -14, bottom: 4 }}>
          <CartesianGrid stroke="var(--suna-line)" />
          <XAxis
            type="number"
            dataKey="x"
            name={`PC1 (${(first * 100).toFixed(1)}%)`}
            tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }}
          />
          <YAxis
            type="number"
            dataKey="y"
            name={`PC2 (${(second * 100).toFixed(1)}%)`}
            tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }}
          />
          <ZAxis range={[22, 22]} />
          <Tooltip cursor={{ strokeDasharray: "3 3" }} />
          {Array.from({ length: clusterCount }, (_, cluster) => (
            <Scatter
              key={cluster}
              name={`簇 ${cluster + 1}`}
              data={points.filter((point) => point.cluster === cluster)}
              fill={clusterColors[cluster % clusterColors.length]}
              fillOpacity={0.6}
            />
          ))}
        </ScatterChart>
      </ResponsiveContainer>
      <p className="suna-correlation-legend">
        仅使用所选列全部有值的行；缺失行被排除而非插补，因此样本数可能小于总行数。
      </p>
    </div>
  );
}
