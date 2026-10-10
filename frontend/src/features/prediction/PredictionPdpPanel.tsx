import { useState } from "react";
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Loader2, Play } from "lucide-react";
import { desktop } from "../../bridge/desktop";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

/**
 * 第 37 章 Partial Dependence。
 *
 * 让某个特征在其取值范围内取网格点，其余特征保持当前输入，逐点预测并画出
 * 平均响应曲线。这里复用既有的单点预测任务，因此对**所有模型族**都成立。
 */
const GRID_STEPS = 8;

export default function PredictionPdpPanel({
  datasetId,
  trainingTaskId,
  featureNames,
  featureValues,
  ranges,
  onError,
}: {
  datasetId: string;
  trainingTaskId: string;
  featureNames: string[];
  featureValues: number[];
  ranges: Array<{ min: number | null; max: number | null }>;
  onError: (message: string) => void;
}) {
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [curve, setCurve] = useState<Array<{ feature: number; prediction: number }>>([]);

  const compute = async () => {
    const range = ranges[index];
    const fallback = featureValues[index] ?? 0;
    const min = range?.min ?? fallback;
    const max = range?.max ?? fallback;
    if (!Number.isFinite(min) || !Number.isFinite(max) || max <= min) {
      onError("该特征的取值范围无效，无法计算 Partial Dependence");
      return;
    }
    setBusy(true);
    setCurve([]);
    try {
      const points: Array<{ feature: number; prediction: number }> = [];
      for (let step = 0; step <= GRID_STEPS; step += 1) {
        const value = min + (max - min) * (step / GRID_STEPS);
        const row = featureValues.map((item, position) => (position === index ? value : item));
        const task = await desktop.predictSteelModel({ datasetId, trainingTaskId, featureValues: row });
        let result = null;
        for (let attempt = 0; attempt < 60 && !result; attempt += 1) {
          result = await desktop.getComputePredictionResult(task.id);
          if (!result) await new Promise((resolve) => window.setTimeout(resolve, 400));
        }
        if (!result) throw new Error("Partial Dependence 预测超时");
        points.push({ feature: value, prediction: Number(result.predictions[0] ?? 0) });
        setCurve([...points]);
      }
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "Partial Dependence 计算失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="suna-prediction-pdp">
      <div className="suna-prediction-section-title">
        <div>
          <h3>Partial Dependence</h3>
          <span>固定其余输入，观察单个特征变化对预测值的影响</span>
        </div>
        <div className="suna-prediction-pdp-controls">
          <Select value={String(index)} onValueChange={(value) => { setIndex(Number(value)); setCurve([]); }}>
            <SelectTrigger aria-label="PDP 特征"><SelectValue /></SelectTrigger>
            <SelectContent>
              {featureNames.map((name, position) => <SelectItem value={String(position)} key={`${name}-${position}`}>{name}</SelectItem>)}
            </SelectContent>
          </Select>
          <button className="suna-ghost-button" onClick={() => void compute()} disabled={busy}>
            {busy ? <Loader2 className="suna-spin" size={14} /> : <Play size={14} />}
            {busy ? "计算中..." : "计算 PDP"}
          </button>
        </div>
      </div>
      {curve.length > 0 ? (
        <ResponsiveContainer width="100%" height={200}>
          <LineChart data={curve} margin={{ top: 10, right: 12, left: -14, bottom: 4 }}>
            <CartesianGrid stroke="var(--suna-line)" vertical={false} />
            <XAxis dataKey="feature" type="number" domain={["dataMin", "dataMax"]} tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
            <YAxis tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
            <Tooltip />
            <Line type="monotone" dataKey="prediction" stroke="var(--suna-primary)" dot={false} />
          </LineChart>
        </ResponsiveContainer>
      ) : (
        <p className="suna-prediction-muted">选择特征后点击「计算 PDP」。其余输入沿用上方填写值。</p>
      )}
    </div>
  );
}
