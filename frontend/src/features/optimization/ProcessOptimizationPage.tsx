import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, CalendarPlus, CheckCircle2, Download, Gauge, Loader2, Play, RefreshCw, Upload } from "lucide-react";
import { desktop, type BackgroundTask, type ComputeOptimizationResult, type SteelDatasetRecord } from "../../bridge/desktop";
import { columnDisplayName } from "../prediction/predictionModel";
import { Checkbox } from "../../components/ui/checkbox";
import { Input } from "../../components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";
import OptimizationParetoChart from "./OptimizationParetoChart";
import "./optimization.css";

/** 第 38–43 章：工艺优化（算法可选、训练任务联动、Pareto 前沿、候选入库）。 */

type State = "idle" | "loading" | "running" | "error";
const numeric = (dataset: SteelDatasetRecord) => dataset.columns.filter((column) => column.inferredType === "number" && !column.duplicate);

const ALGORITHMS = [
  { value: "nsga2", label: "NSGA-II（多目标）", multiOnly: false },
  { value: "tpe", label: "贝叶斯优化 TPE", multiOnly: false },
  { value: "ga", label: "遗传算法 GA", multiOnly: true },
  { value: "pso", label: "粒子群 PSO", multiOnly: true },
  { value: "grid", label: "网格搜索", multiOnly: false },
] as const;

export default function ProcessOptimizationPage() {
  const [datasets, setDatasets] = useState<SteelDatasetRecord[]>([]);
  const [datasetId, setDatasetId] = useState("");
  const [trainingTasks, setTrainingTasks] = useState<BackgroundTask[]>([]);
  const [trainingTaskId, setTrainingTaskId] = useState("");
  const [algorithm, setAlgorithm] = useState<string>("");
  const [objectiveColumns, setObjectiveColumns] = useState<number[]>([]);
  const [direction, setDirection] = useState<"minimize" | "maximize">("maximize");
  const [trials, setTrials] = useState(80);
  const [state, setState] = useState<State>("idle");
  const [result, setResult] = useState<ComputeOptimizationResult | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const dataset = datasets.find((item) => item.id === datasetId);
  const columns = useMemo(() => dataset ? numeric(dataset) : [], [dataset]);
  const multiObjective = objectiveColumns.length > 1;

  const load = async () => {
    setState("loading");
    try {
      const items = await desktop.listSteelDatasets();
      setDatasets(items.filter((item) => item.mappingState === "ready"));
      if (!datasetId) setDatasetId(items.find((item) => item.mappingState === "ready")?.id ?? "");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "无法加载数据集");
    } finally {
      setState("idle");
    }
  };
  useEffect(() => { void load(); }, []);

  // 第 38 章体验：从已完成的训练任务里选模型，不再手工填写任务 ID。
  useEffect(() => {
    setTrainingTaskId("");
    if (!datasetId) { setTrainingTasks([]); return; }
    let cancelled = false;
    void (async () => {
      try {
        const tasks = await desktop.listBackgroundTasks();
        if (cancelled) return;
        const usable = tasks
          .filter((task) => task.kind.startsWith("compute_train") && task.state === "completed" && task.dataset_id === datasetId)
          .sort((left, right) => right.updated_at.localeCompare(left.updated_at));
        setTrainingTasks(usable);
        setTrainingTaskId(usable[0]?.id ?? "");
      } catch (reason) {
        if (!cancelled) setError(reason instanceof Error ? reason.message : "无法加载训练任务");
      }
    })();
    return () => { cancelled = true; };
  }, [datasetId]);

  useEffect(() => { setObjectiveColumns(columns.slice(0, 1).map((column) => column.ordinal)); }, [datasetId]);

  const toggleObjective = (ordinal: number) => setObjectiveColumns((current) => current.includes(ordinal) ? current.filter((item) => item !== ordinal) : [...current, ordinal]);

  const poll = async (id: string) => {
    for (let i = 0; i < 80; i += 1) {
      const value = await desktop.getComputeOptimizationResult(id);
      if (value) return value;
      await new Promise((resolve) => window.setTimeout(resolve, 700));
    }
    throw new Error("优化任务超时，请在运行记录中查看状态");
  };

  const run = async () => {
    if (!dataset || !trainingTaskId || objectiveColumns.length === 0) {
      setError("请选择数据集、已完成的训练任务，并至少选择一个优化目标");
      return;
    }
    if (multiObjective && (algorithm === "ga" || algorithm === "pso")) {
      setError("遗传算法与粒子群只支持单目标，多目标请选择 NSGA-II 或网格搜索");
      return;
    }
    const bounds = columns.map((column) => ({ min: column.min ?? 0, max: column.max ?? 1 }));
    const fixedValues = columns.map(() => null as number | null);
    setState("running"); setError(""); setResult(null);
    try {
      const task = await desktop.optimizeSteelProcess({
        datasetId: dataset.id,
        trainingTaskId,
        direction,
        objectiveColumns,
        bounds,
        fixedValues,
        constraints: [],
        trials: Math.max(10, Math.min(500, trials)),
        seed: 42,
        algorithm: algorithm === "" ? undefined : (algorithm as "nsga2" | "tpe" | "ga" | "pso" | "grid"),
      });
      const completed = await poll(task.id);
      setResult(completed);
      setNotice(`优化完成：${completed.method} · ${completed.trials_completed} 次评估 · 前沿 ${completed.pareto_front.length} 个解`);
    } catch (reason) {
      setState("error");
      setError(reason instanceof Error ? reason.message : "工艺优化失败");
    } finally {
      setState("idle");
    }
  };

  const importDataset = async () => {
    const path = await desktop.openFileDialog({ multiple: false, directory: false, filters: [{ name: "数据文件", extensions: ["csv", "xlsx", "xls", "json"] }] });
    if (typeof path !== "string") return;
    try {
      await desktop.saveSteelDataset({ sourcePath: path });
      setNotice("数据集已保存，请在数据实验室完成列映射并激活");
      await load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "数据集导入失败");
    }
  };

  const exportResult = async () => {
    if (!result || !dataset) return;
    const path = await desktop.saveFileDialog({ defaultPath: `${dataset.sourceName.replace(/\.[^.]+$/, "")}-优化方案.json` });
    if (!path) return;
    const blob = new Blob([JSON.stringify(result, null, 2)], { type: "application/json;charset=utf-8" });
    const anchor = document.createElement("a");
    anchor.href = URL.createObjectURL(blob);
    anchor.download = path.split(/[\\/]/).pop() || "optimization.json";
    anchor.click();
    URL.revokeObjectURL(anchor.href);
    setNotice("候选方案已导出为 JSON");
  };

  const addToPlan = async (index: number) => {
    if (!result || !dataset) return;
    const recommendation = result.recommendations[index];
    const boundsByName = new Map(columns.map((column) => [column.originalName, column]));
    try {
      await desktop.createExperimentPlan({
        source: "optimization",
        title: `方案 ${String.fromCharCode(65 + (index % 26))}（${dataset.sourceName}）`,
        objectiveNote: `${direction === "maximize" ? "最大化" : "最小化"} ${result.objectives.join(" / ")}`,
        variables: result.feature_names.map((name) => {
          const column = boundsByName.get(name);
          return {
            name,
            low: column?.min ?? undefined,
            high: column?.max ?? undefined,
            value: recommendation.values[name],
          };
        }),
        recommendation: { ...recommendation, training_task_id: trainingTaskId, method: result.method, seed: result.deterministic_seed },
      });
      setNotice("候选方案已加入实验计划，可在「实验助手」查看");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "无法加入实验计划");
    }
  };

  return (
    <section className="suna-optimization-page">
      <header className="suna-optimization-header">
        <div>
          <span className="suna-module-kicker">SUNA PROCESS OPTIMIZATION</span>
          <h1><Gauge size={24} />工艺优化</h1>
          <p>在数据和已训练模型约束下探索可行的工艺方案。</p>
        </div>
        <div className="suna-optimization-actions">
          <button className="suna-ghost-button" onClick={() => void load()}><RefreshCw size={15} />刷新数据</button>
          <button className="suna-ghost-button" onClick={() => void importDataset()}><Upload size={15} />导入数据</button>
        </div>
      </header>
      {error && <div className="suna-optimization-alert"><AlertTriangle size={16} />{error}</div>}
      {notice && <div className="suna-optimization-notice"><CheckCircle2 size={16} />{notice}</div>}
      <div className="suna-optimization-layout">
        <section className="suna-optimization-config">
          <div className="suna-optimization-title"><Gauge size={17} /><div><h2>优化任务配置</h2><span>目标、变量和约束</span></div></div>
          <label>数据集<Select value={datasetId} onValueChange={setDatasetId}>
            <SelectTrigger aria-label="数据集"><SelectValue /></SelectTrigger>
            <SelectContent><SelectItem value="">选择已激活数据集</SelectItem>
              {datasets.map((item) => <SelectItem value={item.id} key={item.id}>{item.sourceName}</SelectItem>)}
            </SelectContent>
          </Select></label>
          <label>训练模型<Select value={trainingTaskId} onValueChange={setTrainingTaskId}>
            <SelectTrigger aria-label="训练模型"><SelectValue /></SelectTrigger>
            <SelectContent>
              {trainingTasks.map((task) => <SelectItem value={task.id} key={task.id}>{`训练 ${task.id.slice(0, 8)} · ${new Date(task.updated_at).toLocaleString()}`}</SelectItem>)}
            </SelectContent>
          </Select></label>
          {!trainingTaskId && <span className="suna-optimization-muted">该数据集暂无已完成的训练任务，请先在「性能预测」完成训练。</span>}
          <label>优化算法<Select value={algorithm} onValueChange={setAlgorithm}>
            <SelectTrigger aria-label="优化算法"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="">自动（多目标 NSGA-II / 单目标 TPE）</SelectItem>
              {ALGORITHMS.map((item) => (
                <SelectItem value={item.value} key={item.value} disabled={item.multiOnly && multiObjective}>
                  {item.label}{item.multiOnly && multiObjective ? "（仅单目标）" : ""}
                </SelectItem>
              ))}
            </SelectContent>
          </Select></label>
          <label>优化方向<Select value={direction} onValueChange={(value) => setDirection(value as typeof direction)}>
            <SelectTrigger aria-label="优化方向"><SelectValue /></SelectTrigger>
            <SelectContent><SelectItem value="maximize">最大化性能</SelectItem><SelectItem value="minimize">最小化目标</SelectItem></SelectContent>
          </Select></label>
          <fieldset><legend>优化目标（{multiObjective ? "多目标" : "单目标"}）</legend>
            {columns.map((column) => (
              <label className="suna-optimization-check" key={column.ordinal}>
                <Checkbox aria-label={column.originalName} checked={objectiveColumns.includes(column.ordinal)} onCheckedChange={() => toggleObjective(column.ordinal)} />
                {columnDisplayName(column)}<small>{column.unit || column.inferredType}</small>
              </label>
            ))}
            {columns.length === 0 && <span className="suna-optimization-muted">暂无可用数值列</span>}
          </fieldset>
          <label>搜索次数<Input type="number" min="10" max="500" step="10" value={trials} onChange={(event) => setTrials(Number(event.target.value))} /></label>
          <button className="suna-primary-button suna-optimization-run" onClick={() => void run()} disabled={state === "running" || state === "loading"}>
            {state === "running" ? <Loader2 className="suna-spin" size={16} /> : <Play size={16} />}{state === "running" ? "优化中..." : "开始优化"}
          </button>
        </section>
        <section className="suna-optimization-result">
          <div className="suna-optimization-title"><Gauge size={17} /><div><h2>候选方案</h2><span>{result ? `${result.method} · ${result.trials_completed} 次评估 · 前沿 ${result.pareto_front.length} 解` : "运行任务后显示 Pareto 候选方案"}</span></div>
            {result && <button className="suna-ghost-button" onClick={() => void exportResult()}><Download size={14} />导出 JSON</button>}
          </div>
          {result && result.recommendations.length > 0 && (
            <OptimizationParetoChart front={result.pareto_front} recommendations={result.recommendations} objectives={result.objectives} />
          )}
          {result ? (
            <div className="suna-optimization-table">
              <div className="suna-optimization-row suna-optimization-row-head"><strong>方案</strong><strong>预测值</strong><strong>目标值</strong><strong>工艺取值</strong><strong>操作</strong></div>
              {result.recommendations.map((recommendation, index) => (
                <div className="suna-optimization-row" key={index}>
                  <span>方案 {String.fromCharCode(65 + (index % 26))}</span>
                  <span>{recommendation.prediction.toFixed(3)}</span>
                  <span>{recommendation.objectives.map((value) => value.toFixed(3)).join(" / ")}</span>
                  <span className="suna-optimization-values">{result.feature_names.map((name) => `${name}=${recommendation.values[name]?.toFixed(2) ?? "-"}`).join("，")}</span>
                  <span className="suna-optimization-actions-cell"><button className="suna-ghost-button" onClick={() => void addToPlan(index)}><CalendarPlus size={13} />加入实验计划</button></span>
                </div>
              ))}
            </div>
          ) : (
            <div className="suna-optimization-empty"><Gauge size={30} /><strong>尚未生成方案</strong><span>选择训练模型后运行优化。</span></div>
          )}
        </section>
      </div>
    </section>
  );
}
