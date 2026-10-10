import { useState } from "react";
import { CheckCircle2, Loader2, Play, Table2 } from "lucide-react";
import { desktop, type ComputeModelMetrics, type SteelTrainingAlgorithm } from "../../bridge/desktop";
import { Checkbox } from "../../components/ui/checkbox";
import { Input } from "../../components/ui/input";
import { TRAINING_ALGORITHMS, algorithmLabel } from "./predictionModel";

/** 第 36 章：多个模型同时训练并预测，用表格对比预测值、误差与指标。 */

type ComparisonRow = {
  algorithm: SteelTrainingAlgorithm;
  metrics: ComputeModelMetrics | null;
  prediction: number | null;
};

const DEFAULT_SELECTION: SteelTrainingAlgorithm[] = [
  "random_forest",
  "hist_gradient_boosting",
  "lightgbm",
  "xgboost",
];

function formatMetric(value: number | null | undefined, digits = 4) {
  return typeof value === "number" && Number.isFinite(value) ? value.toFixed(digits) : "-";
}

export default function ModelComparisonPanel({
  datasetId,
  targetColumn,
  featureColumns,
  featureValues,
  onNotice,
  onError,
}: {
  datasetId: string;
  targetColumn: number;
  featureColumns: number[];
  featureValues: number[];
  onNotice: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [selected, setSelected] = useState<SteelTrainingAlgorithm[]>(DEFAULT_SELECTION);
  const [rows, setRows] = useState<ComparisonRow[]>([]);
  const [busy, setBusy] = useState(false);
  const [actual, setActual] = useState("");

  const toggle = (value: SteelTrainingAlgorithm) =>
    setSelected((current) =>
      current.includes(value) ? current.filter((item) => item !== value) : [...current, value],
    );

  const compare = async () => {
    if (!datasetId || featureColumns.length === 0 || selected.length === 0) {
      onError("请选择数据集、输入特征和至少一个模型");
      return;
    }
    setBusy(true);
    setRows([]);
    try {
      const next: ComparisonRow[] = [];
      for (const algorithm of selected) {
        const task = await desktop.trainSteelDataset({
          datasetId,
          targetColumn,
          featureColumns,
          algorithm,
          splitPolicy: { kind: "random", validationFraction: 0.2, seed: 42 },
        });
        let trained = null;
        for (let attempt = 0; attempt < 90 && !trained; attempt += 1) {
          trained = await desktop.getComputeTrainingResult(task.id);
          if (!trained) await new Promise((resolve) => window.setTimeout(resolve, 800));
        }
        if (!trained) throw new Error("模型训练超时，请在运行记录中查看任务状态");
        const predictionTask = await desktop.predictSteelModel({
          datasetId,
          trainingTaskId: trained.task_id,
          featureValues,
        });
        let predicted = null;
        for (let attempt = 0; attempt < 60 && !predicted; attempt += 1) {
          predicted = await desktop.getComputePredictionResult(predictionTask.id);
          if (!predicted) await new Promise((resolve) => window.setTimeout(resolve, 600));
        }
        if (!predicted) throw new Error("预测任务超时，请在运行记录中查看状态");
        next.push({
          algorithm,
          metrics: trained.artifact.metrics.validation ?? null,
          prediction: predicted.predictions[0] ?? null,
        });
        setRows([...next]);
      }
      onNotice(`已完成 ${next.length} 个模型的对比`);
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "模型比较失败");
    } finally {
      setBusy(false);
    }
  };

  const actualValue = Number(actual);
  const hasActual = actual.trim() !== "" && Number.isFinite(actualValue);
  const best = rows.reduce<ComparisonRow | null>(
    (current, row) =>
      row.metrics?.r2 != null && (current?.metrics?.r2 == null || row.metrics.r2 > current.metrics.r2)
        ? row
        : current,
    null,
  );

  return (
    <section className="suna-prediction-compare">
      <div className="suna-prediction-section-title">
        <Table2 size={17} />
        <div>
          <h2>模型比较</h2>
          <span>用同一组输入同时训练并预测多个模型，对比指标与误差</span>
        </div>
      </div>
      <div className="suna-prediction-compare-picker" role="group" aria-label="参与比较的模型">
        {TRAINING_ALGORITHMS.map((item) => (
          <label className="suna-prediction-check" key={item.value}>
            <Checkbox
              aria-label={item.label}
              checked={selected.includes(item.value)}
              disabled={busy}
              onCheckedChange={() => toggle(item.value)}
            />
            {item.label}
            <small>{item.family}</small>
          </label>
        ))}
      </div>
      <div className="suna-prediction-compare-actions">
        <label>
          实测值（可选）
          <Input
            aria-label="实测值"
            type="number"
            value={actual}
            placeholder="填写实测结果以计算误差"
            onChange={(event) => setActual(event.target.value)}
          />
        </label>
        <button className="suna-primary-button" onClick={() => void compare()} disabled={busy}>
          {busy ? <Loader2 className="suna-spin" size={16} /> : <Play size={16} />}
          {busy ? "训练并比较中..." : "训练并比较"}
        </button>
      </div>
      {rows.length === 0 ? (
        <p className="suna-prediction-muted">选择模型后点击「训练并比较」，结果会显示预测值、实测值、误差、R²、MAE 与 RMSE。</p>
      ) : (
        <div className="suna-prediction-compare-table">
          <div className="suna-prediction-compare-row head">
            <strong>模型</strong><strong>预测值</strong><strong>实测值</strong><strong>误差</strong><strong>R²</strong><strong>MAE</strong><strong>RMSE</strong>
          </div>
          {rows.map((row) => (
            <div className="suna-prediction-compare-row" key={row.algorithm}>
              <span>
                {algorithmLabel(row.algorithm)}
                {best?.algorithm === row.algorithm && <CheckCircle2 size={13} className="suna-prediction-best" aria-label="验证集 R² 最高" />}
              </span>
              <span>{formatMetric(row.prediction, 3)}</span>
              <span>{hasActual ? formatMetric(actualValue, 3) : "-"}</span>
              <span>{hasActual && row.prediction != null ? formatMetric(row.prediction - actualValue, 3) : "-"}</span>
              <span>{formatMetric(row.metrics?.r2)}</span>
              <span>{formatMetric(row.metrics?.mae)}</span>
              <span>{formatMetric(row.metrics?.rmse)}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
