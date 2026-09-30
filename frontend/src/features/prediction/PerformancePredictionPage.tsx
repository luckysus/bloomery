import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, BarChart3, CheckCircle2, Database, Loader2, Play, RefreshCw, Upload } from "lucide-react";
import { desktop, type ComputePredictionResult, type ComputeTrainingResult, type SteelDatasetRecord } from "../../bridge/desktop";
import "./prediction.css";

type RunState = "idle" | "loading" | "running" | "error";
const numeric = (dataset: SteelDatasetRecord) => dataset.columns.filter((column) => column.inferredType === "number" && !column.duplicate);
const midpoint = (column: SteelDatasetRecord["columns"][number]) => column.min != null && column.max != null ? (column.min + column.max) / 2 : 0;

export default function PerformancePredictionPage() {
  const [datasets, setDatasets] = useState<SteelDatasetRecord[]>([]);
  const [datasetId, setDatasetId] = useState("");
  const [targetColumn, setTargetColumn] = useState("");
  const [featureColumns, setFeatureColumns] = useState<number[]>([]);
  const [algorithm, setAlgorithm] = useState<"linear_regression" | "elasticnet" | "random_forest" | "hist_gradient_boosting">("linear_regression");
  const [training, setTraining] = useState<RunState>("idle");
  const [prediction, setPrediction] = useState<RunState>("idle");
  const [trainingResult, setTrainingResult] = useState<ComputeTrainingResult | null>(null);
  const [predictionResult, setPredictionResult] = useState<ComputePredictionResult | null>(null);
  const [values, setValues] = useState<Record<number, string>>({});
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [tab, setTab] = useState<"single" | "batch" | "models">("single");
  const dataset = datasets.find((item) => item.id === datasetId);
  const columns = useMemo(() => dataset ? numeric(dataset) : [], [dataset]);

  const load = async () => {
    setTraining("loading"); setError("");
    try {
      const items = await desktop.listSteelDatasets();
      setDatasets(items.filter((item) => item.mappingState === "ready"));
      if (!datasetId && items.some((item) => item.mappingState === "ready")) setDatasetId(items.find((item) => item.mappingState === "ready")!.id);
    } catch (reason) { setError(reason instanceof Error ? reason.message : "无法加载数据集"); }
    finally { setTraining("idle"); }
  };
  useEffect(() => { void load(); }, []);
  useEffect(() => {
    if (!dataset) return;
    const available = numeric(dataset);
    const target = available.find((column) => column.canonicalField === "YS") ?? available[available.length - 1];
    setTargetColumn(target ? String(target.ordinal) : "");
    setFeatureColumns(available.filter((column) => column.ordinal !== target?.ordinal).slice(0, 8).map((column) => column.ordinal));
    setTrainingResult(null); setPredictionResult(null);
  }, [datasetId]);
  const target = Number(targetColumn);
  const featureRecords = columns.filter((column) => featureColumns.includes(column.ordinal));
  const toggleFeature = (ordinal: number) => setFeatureColumns((current) => current.includes(ordinal) ? current.filter((item) => item !== ordinal) : [...current, ordinal]);
  const pollTraining = async (id: string) => {
    for (let attempt = 0; attempt < 60; attempt += 1) {
      const result = await desktop.getComputeTrainingResult(id);
      if (result) { setTrainingResult(result); setTraining("idle"); return; }
      await new Promise((resolve) => window.setTimeout(resolve, 800));
    }
    throw new Error("模型训练超时，请在运行记录中查看任务状态");
  };
  const train = async () => {
    if (!dataset || !targetColumn || featureColumns.length === 0) { setError("请选择数据集、预测目标和至少一个特征"); return; }
    setTraining("running"); setError(""); setPredictionResult(null);
    try {
      const task = await desktop.trainSteelDataset({ datasetId: dataset.id, targetColumn: target, featureColumns, algorithm, splitPolicy: { kind: "random", validationFraction: 0.2, seed: 42 } });
      await pollTraining(task.id);
      setNotice("模型训练完成，可以开始预测");
    } catch (reason) { setTraining("error"); setError(reason instanceof Error ? reason.message : "模型训练失败"); }
  };
  const runPrediction = async () => {
    if (!dataset || !trainingResult) return;
    const featureValues = featureRecords.map((column) => Number(values[column.ordinal] ?? midpoint(column)));
    if (featureValues.some((value) => !Number.isFinite(value))) { setError("输入变量必须是有效数字"); return; }
    setPrediction("running"); setError("");
    try {
      const task = await desktop.predictSteelModel({ datasetId: dataset.id, trainingTaskId: trainingResult.task_id, featureValues });
      for (let attempt = 0; attempt < 60; attempt += 1) {
        const result = await desktop.getComputePredictionResult(task.id);
        if (result) { setPredictionResult(result); setPrediction("idle"); return; }
        await new Promise((resolve) => window.setTimeout(resolve, 600));
      }
      throw new Error("预测任务超时，请在运行记录中查看状态");
    } catch (reason) { setPrediction("error"); setError(reason instanceof Error ? reason.message : "预测失败"); }
  };
  const importDataset = async () => {
    const path = await desktop.openFileDialog({ multiple: false, directory: false, filters: [{ name: "数据文件", extensions: ["csv", "xlsx", "xls", "json"] }] });
    if (typeof path !== "string") return;
    try { await desktop.saveSteelDataset({ sourcePath: path }); setNotice("数据集已保存，请在数据实验室完成列映射并激活"); await load(); } catch (reason) { setError(reason instanceof Error ? reason.message : "数据集导入失败"); }
  };
  return <section className="suna-prediction-page"><header className="suna-prediction-header"><div><span className="suna-module-kicker">SUNA PERFORMANCE PREDICTION</span><h1><BarChart3 size={24} />性能预测</h1><p>基于机器学习模型，快速预测钢铁材料的力学性能。</p></div><div className="suna-prediction-actions"><button className="suna-ghost-button" onClick={importDataset}><Upload size={15} />导入数据</button><button className="suna-icon-button" onClick={() => void load()} aria-label="刷新数据集"><RefreshCw size={16} /></button></div></header><nav className="suna-prediction-tabs"><button className={tab === "single" ? "is-active" : ""} onClick={() => setTab("single")}>单点预测</button><button className={tab === "batch" ? "is-active" : ""} onClick={() => setTab("batch")}>批量预测</button><button className={tab === "models" ? "is-active" : ""} onClick={() => setTab("models")}>模型管理</button></nav>{tab !== "single" && <div className="suna-prediction-notice">{tab === "batch" ? "批量预测将在选择数据集后提交整批记录。当前页面先完成模型训练与单点验证。" : "模型管理显示本次训练结果；持久化模型注册由 Rust 模型仓库负责。"}</div>}
    {error && <div className="suna-prediction-alert"><AlertTriangle size={16} />{error}</div>}{notice && <div className="suna-prediction-notice"><CheckCircle2 size={16} />{notice}</div>}
    <div className="suna-prediction-layout"><section className="suna-prediction-config"><div className="suna-prediction-section-title"><Database size={17} /><div><h2>预测任务配置</h2><span>选择已激活的数据集和训练目标</span></div></div><label>数据集<select value={datasetId} onChange={(event) => setDatasetId(event.target.value)}><option value="">选择数据集</option>{datasets.map((item) => <option value={item.id} key={item.id}>{item.sourceName} · {item.rowCount} 行</option>)}</select></label><label>模型<select value={algorithm} onChange={(event) => setAlgorithm(event.target.value as typeof algorithm)}><option value="linear_regression">Linear Regression</option><option value="elasticnet">ElasticNet</option><option value="random_forest">Random Forest</option><option value="hist_gradient_boosting">HistGradientBoosting</option></select></label><label>预测目标<select value={targetColumn} onChange={(event) => setTargetColumn(event.target.value)}><option value="">选择目标列</option>{columns.map((column) => <option value={column.ordinal} key={column.ordinal}>{column.originalName}</option>)}</select></label><fieldset><legend>输入特征</legend>{columns.length === 0 && <span className="suna-prediction-muted">请先选择已激活的数据集</span>}{columns.map((column) => <label className="suna-prediction-check" key={column.ordinal}><input type="checkbox" checked={featureColumns.includes(column.ordinal)} disabled={column.ordinal === target} onChange={() => toggleFeature(column.ordinal)} />{column.originalName}<small>{column.unit || column.inferredType}</small></label>)}</fieldset><button className="suna-primary-button suna-prediction-train" onClick={() => void train()} disabled={training === "running" || training === "loading"}>{training === "running" ? <Loader2 className="suna-spin" size={16} /> : <Play size={16} />}{training === "running" ? "训练中..." : "训练模型"}</button></section>
      <section className="suna-prediction-result"><div className="suna-prediction-section-title"><BarChart3 size={17} /><div><h2>预测结果</h2><span>{trainingResult ? `${trainingResult.artifact.model_type} · 已完成` : "训练完成后显示结果"}</span></div></div>{trainingResult ? <><div className="suna-prediction-metrics">{Object.entries(trainingResult.artifact.metrics).slice(0, 4).map(([key, metric]) => <div key={key}><span>{key}</span><strong>{typeof metric === "number" ? metric.toFixed(4) : String(metric)}</strong></div>)}</div><div className="suna-prediction-inputs"><h3>输入变量</h3>{featureRecords.map((column) => <label key={column.ordinal}>{column.originalName}<input type="number" value={values[column.ordinal] ?? ""} placeholder={`建议 ${midpoint(column).toFixed(3)}`} onChange={(event) => setValues((current) => ({ ...current, [column.ordinal]: event.target.value }))} /></label>)}<button className="suna-primary-button" onClick={() => void runPrediction()} disabled={prediction === "running"}>{prediction === "running" ? <Loader2 className="suna-spin" size={15} /> : <Play size={15} />}{prediction === "running" ? "预测中..." : "运行预测"}</button></div>{predictionResult && <div className="suna-prediction-output"><span>预测值</span><strong>{predictionResult.predictions.map((value) => Number(value).toFixed(3)).join(", ")}</strong>{predictionResult.confidence != null && <small>置信度 {predictionResult.confidence.toFixed(3)}</small>}{predictionResult.applicability_warnings.length > 0 && <div className="suna-prediction-warning"><AlertTriangle size={14} />输入超出模型适用范围，请谨慎解释结果。</div>}</div>}</> : <div className="suna-prediction-empty"><BarChart3 size={30} /><strong>尚未生成预测</strong><span>训练模型后，这里会显示指标和预测结果。</span></div>}</section></div>
  </section>;
}
