import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { DatasetAnalysis, DatasetSeries, MultivariateResult, SteelDatasetRecord } from "../../bridge/desktop";
import { Slider } from "../../components/ui/slider";
import ColumnBoxPlot from "../../components/charts/ColumnBoxPlot";
import ColumnRadarProfile from "../../components/charts/ColumnRadarProfile";
import CorrelationHeatmap from "../../components/charts/CorrelationHeatmap";
import DistributionHistogram from "../../components/charts/DistributionHistogram";
import LineTrendChart from "../../components/charts/LineTrendChart";
import PcaScatter from "../../components/charts/PcaScatter";
import ParetoFrontChart from "../../components/charts/ParetoFrontChart";
import ScatterPlot from "../../components/charts/ScatterPlot";
import DataLabCleaningPanel from "./DataLabCleaningPanel";
import DataLabExplainPanel from "./DataLabExplainPanel";

/**
 * 数据实验室的两个结果面板。
 *
 * 从 `DataLabPage` 抽出，一是让页面壳保持在 300 行预算内，二是把
 * 「统计概览」与「可视化」两组关注点分开，便于单独演进。
 */

export function DataLabAnalysisPanel({
  analysis,
  numericColumns,
  chartData,
  datasetId,
  datasetColumns,
  onCleaned,
}: {
  analysis: DatasetAnalysis;
  numericColumns: DatasetAnalysis["columns"];
  chartData: Array<{ name: string; mean: number }>;
  datasetId: string;
  datasetColumns: SteelDatasetRecord["columns"];
  onCleaned: (message: string) => void;
}) {
  return (
    <>
      <div className="suna-data-metrics">
        <div><span>有效行</span><strong>{analysis.analyzedRowCount}</strong></div>
        <div><span>排除行</span><strong>{analysis.excludedRowCount}</strong></div>
        <div><span>字段数</span><strong>{analysis.columns.length}</strong></div>
        <div><span>数值字段</span><strong>{numericColumns.length}</strong></div>
      </div>
      <div className="suna-data-visuals">
        <div className="suna-data-chart">
          <h3>字段均值分布</h3>
          {chartData.length ? (
            <ResponsiveContainer width="100%" height={210}>
              <BarChart data={chartData} margin={{ top: 12, right: 8, left: -18, bottom: 4 }}>
                <CartesianGrid stroke="var(--suna-line)" vertical={false} />
                <XAxis dataKey="name" tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
                <YAxis tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
                <Tooltip />
                <Bar dataKey="mean" fill="var(--suna-accent)" radius={[3, 3, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          ) : <p>暂无数值字段均值可视化</p>}
        </div>
        <div className="suna-data-quality">
          <h3>数据质量</h3>
          {analysis.columns.slice(0, 6).map((column) => (
            <div key={column.ordinal}>
              <span>{column.name}</span>
              <i><b style={{ width: `${Math.max(2, (1 - column.missingRate) * 100)}%` }} /></i>
              <strong>{(column.missingRate * 100).toFixed(1)}%</strong>
            </div>
          ))}
        </div>
      </div>
      <div className="suna-data-table">
        <div className="suna-data-row head"><strong>字段</strong><strong>类型</strong><strong>缺失率</strong><strong>均值</strong><strong>范围</strong></div>
        {analysis.columns.map((column) => (
          <div className="suna-data-row" key={column.ordinal}>
            <span>{column.name}</span>
            <span>{column.inferredType}</span>
            <span>{(column.missingRate * 100).toFixed(1)}%</span>
            <span>{column.mean == null ? "-" : column.mean.toFixed(3)}</span>
            <span>{column.min == null || column.max == null ? "-" : `${column.min.toFixed(2)} ~ ${column.max.toFixed(2)}`}</span>
          </div>
        ))}
      </div>
      <div className="suna-data-chart">
        <h3>字段分布箱线图</h3>
        <ColumnBoxPlot columns={numericColumns} />
      </div>
      <DataLabCleaningPanel datasetId={datasetId} onCleaned={onCleaned} />
      <DataLabExplainPanel datasetId={datasetId} columns={datasetColumns.filter((column) => column.inferredType === "number" && !column.duplicate)} onMessage={onCleaned} />
    </>
  );
}

export function DataLabVisualizePanel({
  analysis,
  numericColumns,
  distributionColumn,
  distributionCandidates,
  onDistributionChange,
  series,
  multivariate,
  clusterCount,
  onClusterCountChange,
  onClusterCommit,
  busy,
}: {
  analysis: DatasetAnalysis;
  numericColumns: DatasetAnalysis["columns"];
  distributionColumn: DatasetAnalysis["columns"][number] | null;
  distributionCandidates: DatasetAnalysis["columns"];
  onDistributionChange: (ordinal: number) => void;
  series: DatasetSeries | null;
  multivariate: MultivariateResult | null;
  clusterCount: number;
  onClusterCountChange: (value: number) => void;
  onClusterCommit: () => void;
  busy: boolean;
}) {
  return (
    <>
      <div className="suna-data-chart">
        <h3>字段分布直方图</h3>
        <DistributionHistogram
          column={distributionColumn}
          candidates={distributionCandidates}
          onColumnChange={onDistributionChange}
        />
      </div>
      <div className="suna-data-chart">
        <h3>字段相关性热力图</h3>
        <CorrelationHeatmap correlations={analysis.correlations} columns={analysis.columns} />
      </div>
      <div className="suna-data-chart">
        <h3>字段相对水平（雷达图）</h3>
        <ColumnRadarProfile columns={numericColumns} />
      </div>
      <div className="suna-data-chart">
        <h3>字段走势（折线图）</h3>
        {series ? <LineTrendChart series={series} /> : <p>{busy ? "正在加载序列…" : "选择「数据可视化」后自动加载。"}</p>}
      </div>
      <div className="suna-data-chart">
        <h3>字段配对（散点图）</h3>
        {series ? <ScatterPlot series={series} /> : <p>{busy ? "正在加载序列…" : "选择「数据可视化」后自动加载。"}</p>}
      </div>
      <div className="suna-data-chart">
        <h3>Pareto 前沿（多目标权衡）</h3>
        {series ? <ParetoFrontChart series={series} /> : <p>{busy ? "正在加载序列…" : "选择「数据可视化」后自动加载。"}</p>}
      </div>
      <div className="suna-data-chart">
        <div className="suna-pca-head">
          <h3>PCA 投影与聚类</h3>
          <label className="suna-pca-control">
            聚类数 k
            <Slider
              className="suna-pca-slider"
              min={2}
              max={8}
              step={1}
              value={[clusterCount]}
              aria-label="聚类数"
              onValueChange={(value) => onClusterCountChange(value[0] ?? clusterCount)}
              onValueCommit={onClusterCommit}
            />
            <strong>{clusterCount}</strong>
          </label>
        </div>
        {multivariate
          ? <PcaScatter result={multivariate} />
          : <p>{busy ? "正在计算主成分与聚类…" : "选择「数据可视化」后自动计算。"}</p>}
      </div>
    </>
  );
}
