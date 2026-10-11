import { useEffect, useMemo, useState } from "react";
import { Beaker, CalendarPlus, FlaskConical, Loader2, Play, Plus, Trash2 } from "lucide-react";
import { desktop, type ExperimentDesignPoint, type ExperimentDesignResult, type ExperimentVariableRange, type SteelDatasetRecord } from "../../bridge/desktop";
import { Input } from "../../components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

/**
 * 第 44–46 章实验设计工作台。
 *
 * 五种真实算法：全因子 DOE、正交表（Hadamard/L9/L25）、中心复合响应面、
 * 贝叶斯优化（高斯过程 + 期望改进）、主动学习（不确定性 + 覆盖缺口）。
 * 不确定性方法需要已有实验数据：从已激活数据集读取目标列与特征列，
 * 缺失行会被过滤；推荐结果逐条解释并可直接加入实验计划。
 */

const METHODS = [
  { value: "doe", label: "全因子 DOE", needsData: false },
  { value: "orthogonal", label: "正交实验", needsData: false },
  { value: "ccd", label: "响应面（中心复合）", needsData: false },
  { value: "bayesian", label: "贝叶斯优化", needsData: true },
  { value: "active_learning", label: "主动学习", needsData: true },
] as const;

type Variable = { name: string; low: string; high: string };

export function buildExistingMatrix(
  rows: Array<Array<number | null>>,
  targetOrdinal: number,
  featureOrdinals: number[],
): { features: number[][]; targets: number[] } | null {
  const features: number[][] = [];
  const targets: number[] = [];
  for (const row of rows) {
    const target = row[targetOrdinal];
    if (target == null) continue;
    const values: number[] = [];
    let complete = true;
    for (const ordinal of featureOrdinals) {
      const value = row[ordinal];
      if (value == null) { complete = false; break; }
      values.push(value);
    }
    if (!complete) continue;
    features.push(values);
    targets.push(target);
  }
  if (features.length < 5) return null;
  return { features, targets };
}

export default function ExperimentDesignPanel({ datasets, onNotice, onError }: {
  datasets: SteelDatasetRecord[];
  onNotice: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [method, setMethod] = useState<ExperimentDesignResult["method"]>("doe");
  const [variables, setVariables] = useState<Variable[]>([{ name: "", low: "", high: "" }]);
  const [levels, setLevels] = useState(2);
  const [count, setCount] = useState(4);
  const [direction, setDirection] = useState<"minimize" | "maximize">("maximize");
  const [datasetId, setDatasetId] = useState("");
  const [targetColumn, setTargetColumn] = useState("");
  const [featureColumns, setFeatureColumns] = useState<number[]>([]);
  const [result, setResult] = useState<ExperimentDesignResult | null>(null);
  const [busy, setBusy] = useState(false);
  const needsData = METHODS.find((item) => item.value === method)?.needsData ?? false;
  const dataset = datasets.find((item) => item.id === datasetId);
  const numericColumns = useMemo(() => dataset ? dataset.columns.filter((column) => column.inferredType === "number" && !column.duplicate) : [], [dataset]);

  useEffect(() => { setTargetColumn(""); setFeatureColumns([]); }, [datasetId]);

  const update = (index: number, key: keyof Variable, value: string) =>
    setVariables((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, [key]: value } : item));

  const toggleFeature = (ordinal: number) =>
    setFeatureColumns((current) => current.includes(ordinal) ? current.filter((item) => item !== ordinal) : [...current, ordinal]);

  const run = async () => {
    onError(""); setResult(null);
    const valid = variables.filter((item) => item.name.trim() && item.low.trim() !== "" && item.high.trim() !== "");
    if (valid.length === 0) { onError("请至少填写一个完整的变量范围"); return; }
    if (valid.some((item) => !Number.isFinite(Number(item.low)) || !Number.isFinite(Number(item.high)))) {
      onError("变量上下限必须是数字"); return;
    }
    let existing;
    if (needsData) {
      if (!dataset || targetColumn === "" || featureColumns.length === 0) {
        onError("贝叶斯优化与主动学习需要选择数据集、目标列和至少一个特征列"); return;
      }
      const series = await desktop.readSteelDatasetSeries({ datasetId: dataset.id, columns: [...featureColumns, Number(targetColumn)] }).catch((reason: unknown) => {
        onError(reason instanceof Error ? reason.message : "无法读取已有实验数据"); return null;
      });
      if (!series) return;
      const built = buildExistingMatrix(series.rows, Number(targetColumn), featureColumns);
      if (!built) { onError("完整数据行不足 5 行（缺失值会被过滤），无法拟合代理模型"); return; }
      existing = { feature_names: valid.map((item) => item.name.trim()), features: built.features, targets: built.targets };
    }
    setBusy(true);
    try {
      const task = await desktop.designExperiments({
        method,
        variables: valid.map((item) => ({ name: item.name.trim(), low: Number(item.low), high: Number(item.high) })),
        levels,
        count,
        direction,
        seed: 42,
        existing,
      });
      let completed: ExperimentDesignResult | null = null;
      for (let attempt = 0; attempt < 60 && !completed; attempt += 1) {
        completed = await desktop.getComputeDesignResult(task.id);
        if (!completed) await new Promise((resolve) => window.setTimeout(resolve, 500));
      }
      if (!completed) throw new Error("实验设计任务超时，请在运行记录中查看状态");
      setResult(completed);
      onNotice(`已生成 ${completed.points.length} 组${methodLabel(completed.method)}实验组合`);
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "实验设计失败");
    } finally {
      setBusy(false);
    }
  };

  const addAllToPlan = async () => {
    if (!result) return;
    onError("");
    let created = 0;
    try {
      for (const [index, point] of result.points.entries()) {
        await desktop.createExperimentPlan({
          source: "manual",
          title: `${methodLabel(result.method)} 组合 ${index + 1}（${result.variables.map((name) => `${name}=${formatValue(point.values[name])}`).join("，")}）`,
          objectiveNote: `${direction === "maximize" ? "最大化" : "最小化"}目标`,
          variables: result.variables.map((name) => ({ name, value: point.values[name] })),
        });
        created += 1;
      }
      onNotice(`已把 ${created} 组实验加入实验计划`);
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : `无法加入实验计划（已创建 ${created} 组）`);
    }
  };

  return (
    <section className="suna-design-panel">
      <div className="suna-experiment-title"><Beaker size={17} /><div><h2>实验设计工作台</h2><span>DOE、正交、响应面、贝叶斯与主动学习</span></div></div>
      <div className="suna-design-form">
        <label>设计方法<Select value={method} onValueChange={(value) => setMethod(value as ExperimentDesignResult["method"])}>
          <SelectTrigger aria-label="设计方法"><SelectValue /></SelectTrigger>
          <SelectContent>{METHODS.map((item) => <SelectItem value={item.value} key={item.value}>{item.label}</SelectItem>)}</SelectContent>
        </Select></label>
        <div className="suna-design-variable-head"><strong>变量范围</strong>
          <button className="suna-icon-button" onClick={() => setVariables((current) => [...current, { name: "", low: "", high: "" }])} aria-label="添加变量"><Plus size={15} /></button>
        </div>
        {variables.map((item, index) => (
          <div className="suna-experiment-variable" key={index}>
            <Input value={item.name} placeholder="变量名" onChange={(event) => update(index, "name", event.target.value)} />
            <Input value={item.low} placeholder="下限" onChange={(event) => update(index, "low", event.target.value)} />
            <Input value={item.high} placeholder="上限" onChange={(event) => update(index, "high", event.target.value)} />
            {variables.length > 1 && <button className="suna-icon-button" onClick={() => setVariables((current) => current.filter((_, itemIndex) => itemIndex !== index))} aria-label="删除变量"><Trash2 size={14} /></button>}
          </div>
        ))}
        {method === "doe" && <label>水平数<Input type="number" min={2} max={5} value={levels} onChange={(event) => setLevels(Number(event.target.value))} /></label>}
        {needsData && <>
          <label>已有实验数据（数据集）<Select value={datasetId} onValueChange={setDatasetId}>
            <SelectTrigger aria-label="数据集"><SelectValue /></SelectTrigger>
            <SelectContent><SelectItem value="">选择已激活数据集</SelectItem>
              {datasets.map((item) => <SelectItem value={item.id} key={item.id}>{item.sourceName}</SelectItem>)}
            </SelectContent>
          </Select></label>
          <label>目标列<Select value={targetColumn} onValueChange={setTargetColumn}>
            <SelectTrigger aria-label="目标列"><SelectValue /></SelectTrigger>
            <SelectContent><SelectItem value="">选择目标列</SelectItem>
              {numericColumns.map((column) => <SelectItem value={String(column.ordinal)} key={column.ordinal}>{column.originalName}</SelectItem>)}
            </SelectContent>
          </Select></label>
          <fieldset><legend>特征列（至少一个）</legend>
            {numericColumns.filter((column) => column.ordinal !== Number(targetColumn)).map((column) => (
              <label className="suna-optimization-check" key={column.ordinal}>
                <input type="checkbox" checked={featureColumns.includes(column.ordinal)} onChange={() => toggleFeature(column.ordinal)} />
                {column.originalName}
              </label>
            ))}
          </fieldset>
        </>}
        {needsData && <label>推荐组数<Input type="number" min={1} max={20} value={count} onChange={(event) => setCount(Number(event.target.value))} /></label>}
        {needsData && <label>优化方向<Select value={direction} onValueChange={(value) => setDirection(value as typeof direction)}>
          <SelectTrigger aria-label="优化方向"><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="maximize">最大化目标</SelectItem><SelectItem value="minimize">最小化目标</SelectItem></SelectContent>
        </Select></label>}
        <button className="suna-primary-button" onClick={() => void run()} disabled={busy}>
          {busy ? <Loader2 className="suna-spin" size={15} /> : <Play size={15} />}{busy ? "设计中..." : "生成实验组合"}
        </button>
      </div>
      {result && (
        <section className="suna-design-result">
          <div className="suna-experiment-title"><FlaskConical size={17} /><div><h2>推荐结果</h2><span>{methodLabel(result.method)} · {result.points.length} 组</span></div>
            <button className="suna-ghost-button" onClick={() => void addAllToPlan()}><CalendarPlus size={14} />全部加入实验计划</button>
          </div>
          <ul className="suna-design-notes">{result.notes.map((note, index) => <li key={index}>{note}</li>)}</ul>
          <div className="suna-optimization-table">
            <div className="suna-optimization-row suna-optimization-row-head"><strong>#</strong><strong>组合</strong>{result.points[0]?.expected_improvement != null && <strong>期望改进</strong>}{result.points[0]?.predicted_std != null && <strong>预测σ</strong>}{result.points[0]?.coverage_gap != null && <strong>覆盖缺口</strong>}</div>
            {result.points.map((point, index) => <DesignRow point={point} index={index} variables={result.variables} key={index} />)}
          </div>
        </section>
      )}
    </section>
  );
}

function DesignRow({ point, index, variables }: { point: ExperimentDesignPoint; index: number; variables: string[] }) {
  return (
    <div className="suna-optimization-row">
      <span>{index + 1}</span>
      <span className="suna-optimization-values">{variables.map((name) => `${name}=${formatValue(point.values[name])}`).join("，")}</span>
      {point.expected_improvement != null && <span>{point.expected_improvement.toExponential(2)}</span>}
      {point.predicted_std != null && <span>{point.predicted_std.toFixed(4)}</span>}
      {point.coverage_gap != null && <span>{point.coverage_gap.toFixed(4)}</span>}
    </div>
  );
}

function methodLabel(method: ExperimentDesignResult["method"]): string {
  return METHODS.find((item) => item.value === method)?.label ?? method;
}

function formatValue(value: number | undefined): string {
  return value == null ? "-" : Math.abs(value) >= 1000 ? value.toFixed(0) : value.toFixed(2);
}
