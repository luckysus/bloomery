import { useEffect, useState } from "react";
import { BarChart3, Database, FileSpreadsheet, Loader2, RefreshCw, Search, Upload } from "lucide-react";
import { desktop, type DatasetAnalysis, type DatasetSeries, type MultivariateResult, type SteelDatasetRecord } from "../../bridge/desktop";
import { Input } from "../../components/ui/input";
import { DataLabAnalysisPanel, DataLabVisualizePanel } from "./DataLabPanels";
import "./data-lab.css";

type DataTab = "manage" | "analysis" | "visualize";

const tabs: Array<{ id: DataTab; label: string }> = [
  { id: "manage", label: "数据管理" },
  { id: "analysis", label: "数据分析" },
  { id: "visualize", label: "数据可视化" },
];

export default function DataLabPage() {
  const [datasets, setDatasets] = useState<SteelDatasetRecord[]>([]);
  const [selected, setSelected] = useState("");
  const [analysis, setAnalysis] = useState<DatasetAnalysis | null>(null);
  const [tab, setTab] = useState<DataTab>("manage");
  const [distributionOrdinal, setDistributionOrdinal] = useState<number | null>(null);
  const [series, setSeries] = useState<DatasetSeries | null>(null);
  const [multivariate, setMultivariate] = useState<MultivariateResult | null>(null);
  const [clusterCount, setClusterCount] = useState(3);
  const [visualizeBusy, setVisualizeBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [query, setQuery] = useState("");

  const dataset = datasets.find((item) => item.id === selected);

  const load = async () => {
    setBusy(true);
    try {
      const items = await desktop.listSteelDatasets();
      setDatasets(items);
      if (!selected) setSelected(items[0]?.id ?? "");
    } catch (reason) {
      setMessage(reason instanceof Error ? reason.message : "无法加载数据集");
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => { void load(); }, []);

  const importFile = async () => {
    const path = await desktop.openFileDialog({ multiple: false, directory: false, filters: [{ name: "数据文件", extensions: ["csv", "xlsx", "xls", "json"] }] });
    if (typeof path !== "string") return;
    try {
      await desktop.saveSteelDataset({ sourcePath: path });
      setMessage("数据集已保存，请完成列映射后激活");
      await load();
    } catch (reason) {
      setMessage(reason instanceof Error ? reason.message : "导入失败");
    }
  };

  const analyze = async () => {
    if (!dataset) return;
    setBusy(true);
    try {
      // 相关性只在显式给出字段时才计算：这里传入全部数值列（最多 12 个，避免矩阵过大）。
      const correlationColumns = dataset.columns
        .filter((column) => column.inferredType === "number" && !column.duplicate)
        .map((column) => column.ordinal)
        .slice(0, 12);
      const result = await desktop.analyzeSteelDataset({
        datasetId: dataset.id,
        correlationColumns,
        outlierIqrMultiplier: 1.5,
      });
      setAnalysis(result);
      const numeric = result.columns.filter((column) => column.inferredType === "number" && column.distribution.length > 0);
      setDistributionOrdinal(numeric[0]?.ordinal ?? null);
    } catch (reason) {
      setMessage(reason instanceof Error ? reason.message : "数据分析失败");
    } finally {
      setBusy(false);
    }
  };

  const openTab = (next: DataTab) => {
    setTab(next);
    if (next === "manage" || !dataset) return;
    if (!analysis) void analyze();
    if (next === "visualize" && !series) void loadVisualization();
  };

  // 参与多元分析的数值列：最多 6 个，避免 PCA/聚类的维度与载荷难以解读。
  const numericOrdinals = (dataset?.columns ?? [])
    .filter((column) => column.inferredType === "number" && !column.duplicate)
    .map((column) => column.ordinal)
    .slice(0, 6);

  const loadVisualization = async (clusters = clusterCount) => {
    if (!dataset || numericOrdinals.length < 2) {
      setMessage("散点图与 PCA 至少需要两个数值字段");
      return;
    }
    setVisualizeBusy(true);
    try {
      const [seriesResult, multivariateResult] = await Promise.all([
        desktop.readSteelDatasetSeries({ datasetId: dataset.id, columns: numericOrdinals }),
        desktop.analyzeSteelDatasetMultivariate({
          datasetId: dataset.id,
          columns: numericOrdinals,
          components: 2,
          clusters,
        }),
      ]);
      setSeries(seriesResult);
      setMultivariate(multivariateResult);
    } catch (reason) {
      setMessage(reason instanceof Error ? reason.message : "无法加载可视化数据");
    } finally {
      setVisualizeBusy(false);
    }
  };

  const numericColumns = analysis?.columns.filter((column) => column.inferredType === "number") ?? [];
  const distributionCandidates = numericColumns.filter((column) => column.distribution.length > 0);
  const distributionColumn = distributionCandidates.find((column) => column.ordinal === distributionOrdinal) ?? distributionCandidates[0] ?? null;
  const chartData = numericColumns.filter((column) => column.mean != null).slice(0, 8)
    .map((column) => ({ name: column.name, mean: Number(column.mean?.toFixed(3)) }));
  const filteredDatasets = datasets.filter((item) => item.sourceName.toLocaleLowerCase().includes(query.toLocaleLowerCase()));

  return (
    <section className="suna-data-page">
      <header className="suna-data-header">
        <div>
          <span className="suna-module-kicker">SUNA DATA LABORATORY</span>
          <h1><BarChart3 size={24} />数据实验室</h1>
          <p>管理实验数据，检查质量并为预测与优化准备可信数据。</p>
        </div>
        <div className="suna-data-actions">
          <button className="suna-ghost-button" onClick={() => void load()}><RefreshCw size={15} />刷新</button>
          <button className="suna-primary-button" onClick={() => void importFile()}><Upload size={15} />导入数据</button>
        </div>
      </header>

      <nav className="suna-data-tabs">
        {tabs.map((item) => (
          <button
            key={item.id}
            type="button"
            className={tab === item.id ? "is-active" : ""}
            aria-pressed={tab === item.id}
            onClick={() => openTab(item.id)}
          >
            {item.label}
          </button>
        ))}
      </nav>

      {message && <div className="suna-data-message">{message}</div>}

      <div className="suna-data-layout">
        <section className="suna-data-list">
          <div className="suna-data-title">
            <Database size={17} />
            <div><h2>数据文件</h2><span>{datasets.length} 个数据集</span></div>
          </div>
          <label className="suna-data-search">
            <Search size={14} />
            <Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索数据文件" />
          </label>
          <div className="suna-data-files">
            {filteredDatasets.map((item) => (
              <button
                className={`suna-data-item ${item.id === selected ? "is-active" : ""}`}
                key={item.id}
                onClick={() => { setSelected(item.id); setAnalysis(null); setTab("manage"); }}
              >
                <FileSpreadsheet size={17} />
                <span>
                  <strong>{item.sourceName}</strong>
                  <small>{item.rowCount} 行 · {item.columnCount} 列 · {item.format}</small>
                </span>
                <em>{item.mappingState === "ready" ? "已激活" : "待映射"}</em>
              </button>
            ))}
            {filteredDatasets.length === 0 && <div className="suna-data-empty">暂无数据集，请导入 CSV 或 Excel。</div>}
          </div>
        </section>

        <section className="suna-data-analysis">
          <div className="suna-data-analysis-head">
            <div className="suna-data-title">
              <BarChart3 size={17} />
              <div>
                <h2>{dataset ? dataset.sourceName : "实验数据"}</h2>
                <span>{dataset ? `${dataset.rowCount} 条记录 · ${dataset.columnCount} 个字段` : "选择数据文件查看分析"}</span>
              </div>
            </div>
            {dataset && (
              <button className="suna-primary-button" onClick={() => void analyze()} disabled={busy}>
                {busy ? <Loader2 className="suna-spin" size={15} /> : <BarChart3 size={15} />}分析数据
              </button>
            )}
          </div>

          {tab === "manage" && (
            <div className="suna-data-empty large">
              <Database size={30} />
              <strong>选择数据集后开始分析</strong>
              <span>「数据分析」查看质量与分布，「数据可视化」查看相关性、分布与对比图。</span>
            </div>
          )}

          {tab !== "manage" && !analysis && (
            <div className="suna-data-empty large">
              <BarChart3 size={30} />
              <strong>{busy ? "正在分析…" : "尚未生成分析结果"}</strong>
              <span>点击「分析数据」或重新选择数据集。</span>
            </div>
          )}

          {tab === "analysis" && analysis && (
            <DataLabAnalysisPanel analysis={analysis} numericColumns={numericColumns} chartData={chartData} datasetId={selected} datasetColumns={dataset?.columns ?? []} onCleaned={(next) => { setMessage(next); void load(); }} />
          )}

          {tab === "visualize" && analysis && (
            <DataLabVisualizePanel
              analysis={analysis}
              numericColumns={numericColumns}
              distributionColumn={distributionColumn}
              distributionCandidates={distributionCandidates}
              onDistributionChange={setDistributionOrdinal}
              series={series}
              multivariate={multivariate}
              clusterCount={clusterCount}
              onClusterCountChange={setClusterCount}
              onClusterCommit={() => void loadVisualization()}
              busy={visualizeBusy}
            />
          )}
        </section>
      </div>
    </section>
  );
}
