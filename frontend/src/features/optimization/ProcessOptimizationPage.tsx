import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Gauge, Loader2, Play, RefreshCw, Upload } from "lucide-react";
import { desktop, type ComputeOptimizationResult, type SteelDatasetRecord } from "../../bridge/desktop";
import "./optimization.css";

type State = "idle" | "loading" | "running" | "error";
const numeric = (dataset: SteelDatasetRecord) => dataset.columns.filter((column) => column.inferredType === "number" && !column.duplicate);

export default function ProcessOptimizationPage() {
  const [datasets, setDatasets] = useState<SteelDatasetRecord[]>([]);
  const [datasetId, setDatasetId] = useState("");
  const [trainingTaskId, setTrainingTaskId] = useState("");
  const [objectiveColumns, setObjectiveColumns] = useState<number[]>([]);
  const [direction, setDirection] = useState<"minimize" | "maximize">("maximize");
  const [trials, setTrials] = useState(80);
  const [state, setState] = useState<State>("loading");
  const [result, setResult] = useState<ComputeOptimizationResult | null>(null);
  const [error, setError] = useState("");
  const dataset = datasets.find((item) => item.id === datasetId);
  const columns = useMemo(() => dataset ? numeric(dataset) : [], [dataset]);
  const load = async () => { setState("loading"); try { const items = await desktop.listSteelDatasets(); setDatasets(items.filter((item) => item.mappingState === "ready")); if (!datasetId) setDatasetId(items.find((item) => item.mappingState === "ready")?.id ?? ""); } catch (reason) { setError(reason instanceof Error ? reason.message : "无法加载数据集"); } finally { setState("idle"); } };
  useEffect(() => { void load(); }, []);
  useEffect(() => { setObjectiveColumns(columns.slice(0, 1).map((column) => column.ordinal)); }, [datasetId]);
  const toggleObjective = (ordinal: number) => setObjectiveColumns((current) => current.includes(ordinal) ? current.filter((item) => item !== ordinal) : [...current, ordinal]);
  const poll = async (id: string) => { for (let i = 0; i < 80; i += 1) { const value = await desktop.getComputeOptimizationResult(id); if (value) { setResult(value); setState("idle"); return; } await new Promise((resolve) => window.setTimeout(resolve, 700)); } throw new Error("优化任务超时，请在运行记录中查看状态"); };
  const run = async () => {
    if (!dataset || !trainingTaskId.trim() || objectiveColumns.length === 0) { setError("请选择数据集、填写已完成的训练任务 ID，并选择优化目标"); return; }
    const bounds = columns.map((column) => ({ min: column.min ?? 0, max: column.max ?? 1 }));
    const fixedValues = columns.map(() => null as number | null);
    setState("running"); setError(""); setResult(null);
    try { const task = await desktop.optimizeSteelProcess({ datasetId: dataset.id, trainingTaskId: trainingTaskId.trim(), direction, objectiveColumns, bounds, fixedValues, constraints: [], trials: Math.max(10, Math.min(1000, trials)), seed: 42 }); await poll(task.id); }
    catch (reason) { setState("error"); setError(reason instanceof Error ? reason.message : "工艺优化失败"); }
  };
  return <section className="suna-optimization-page"><header className="suna-optimization-header"><div><span className="suna-module-kicker">SUNA PROCESS OPTIMIZATION</span><h1><Gauge size={24} />工艺优化</h1><p>在数据和已训练模型约束下探索可行的工艺方案。</p></div><div className="suna-optimization-actions"><button className="suna-ghost-button" onClick={() => void load()}><RefreshCw size={15} />刷新数据</button><button className="suna-ghost-button" onClick={() => setError("请先在数据实验室导入并激活数据集") }><Upload size={15} />数据准备</button></div></header>{error && <div className="suna-optimization-alert"><AlertTriangle size={16} />{error}</div>}<div className="suna-optimization-layout"><section className="suna-optimization-config"><div className="suna-optimization-title"><Gauge size={17} /><div><h2>优化任务配置</h2><span>目标、变量和约束</span></div></div><label>数据集<select value={datasetId} onChange={(event) => setDatasetId(event.target.value)}><option value="">选择已激活数据集</option>{datasets.map((item) => <option value={item.id} key={item.id}>{item.sourceName}</option>)}</select></label><label>训练任务 ID<input value={trainingTaskId} onChange={(event) => setTrainingTaskId(event.target.value)} placeholder="来自性能预测的已完成训练任务" /></label><label>优化方向<select value={direction} onChange={(event) => setDirection(event.target.value as typeof direction)}><option value="maximize">最大化性能</option><option value="minimize">最小化目标</option></select></label><fieldset><legend>优化目标</legend>{columns.map((column) => <label className="suna-optimization-check" key={column.ordinal}><input type="checkbox" checked={objectiveColumns.includes(column.ordinal)} onChange={() => toggleObjective(column.ordinal)} />{column.originalName}<small>{column.unit || column.inferredType}</small></label>)}{columns.length === 0 && <span className="suna-optimization-muted">暂无可用数值列</span>}</fieldset><label>搜索次数<input type="number" min="10" max="1000" step="10" value={trials} onChange={(event) => setTrials(Number(event.target.value))} /></label><button className="suna-primary-button suna-optimization-run" onClick={() => void run()} disabled={state === "running" || state === "loading"}>{state === "running" ? <Loader2 className="suna-spin" size={16} /> : <Play size={16} />}{state === "running" ? "优化中..." : "开始优化"}</button></section><section className="suna-optimization-result"><div className="suna-optimization-title"><Gauge size={17} /><div><h2>候选方案</h2><span>{result ? `${result.trials_completed} 次搜索 · ${result.method}` : "运行任务后显示 Pareto 候选方案"}</span></div></div>{result ? <div className="suna-optimization-table"><div className="suna-optimization-row suna-optimization-row-head"><strong>方案</strong><strong>预测值</strong><strong>目标值</strong><strong>约束</strong></div>{result.recommendations.map((recommendation, index) => <div className="suna-optimization-row" key={index}><span>方案 {String.fromCharCode(65 + index)}</span><span>{recommendation.prediction.toFixed(3)}</span><span>{recommendation.objectives.map((value) => value.toFixed(3)).join(" / ")}</span><span className={recommendation.feasible ? "is-feasible" : "is-invalid"}>{recommendation.feasible ? "可行" : "不可行"}</span></div>)}</div> : <div className="suna-optimization-empty"><Gauge size={30} /><strong>尚未生成方案</strong><span>输入训练任务 ID 后运行优化。</span></div>}</section></div></section>;
}
