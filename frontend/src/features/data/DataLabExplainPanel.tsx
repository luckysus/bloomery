import { useState } from "react";
import { Loader2, Play } from "lucide-react";
import { desktop, type ShapExplanation, type SteelDatasetRecord } from "../../bridge/desktop";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";
import ShapBarChart from "../../components/charts/ShapBarChart";
import { featureImportanceShares } from "../prediction/predictionModel";

/**
 * 第 31 章「特征重要性 / SHAP」。
 *
 * 数据实验室里直接拟合一个线性模型：特征重要性取标准化系数绝对值，
 * SHAP 用线性模型的精确解（`explain_steel_model`），输入取各列中位区间中点。
 */
export default function DataLabExplainPanel({
  datasetId,
  columns,
  onMessage,
}: {
  datasetId: string;
  columns: SteelDatasetRecord["columns"];
  onMessage: (message: string) => void;
}) {
  const [target, setTarget] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [importance, setImportance] = useState<Array<{ name: string; share: number }>>([]);
  const [explanation, setExplanation] = useState<ShapExplanation | null>(null);

  const run = async () => {
    const targetColumn = Number(target);
    const features = columns.filter((column) => column.ordinal !== targetColumn).slice(0, 8);
    if (!datasetId || target === "" || features.length === 0) {
      setError("请选择预测目标，并确保还有其它数值列可作为特征");
      return;
    }
    setBusy(true);
    setError("");
    setImportance([]);
    setExplanation(null);
    try {
      const task = await desktop.trainSteelDataset({
        datasetId,
        targetColumn,
        featureColumns: features.map((column) => column.ordinal),
        algorithm: "linear_regression",
        splitPolicy: { kind: "random", validationFraction: 0.2, seed: 42 },
      });
      let trained = null;
      for (let attempt = 0; attempt < 90 && !trained; attempt += 1) {
        trained = await desktop.getComputeTrainingResult(task.id);
        if (!trained) await new Promise((resolve) => window.setTimeout(resolve, 800));
      }
      if (!trained) throw new Error("特征重要性训练超时，请在运行记录中查看任务状态");
      setImportance(featureImportanceShares(trained.artifact.feature_names, trained.artifact.feature_importance));
      const midpoints = features.map((column) =>
        column.min != null && column.max != null ? (column.min + column.max) / 2 : 0,
      );
      setExplanation(await desktop.explainSteelModel({ modelId: trained.artifact.model_id, features: midpoints }));
      onMessage("已生成特征重要性与 SHAP 解释");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "特征重要性计算失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="suna-data-chart">
      <h3>特征重要性 / SHAP</h3>
      <div className="suna-data-cleaning">
        <label>
          预测目标
          <Select value={target} onValueChange={setTarget}>
            <SelectTrigger aria-label="特征重要性目标"><SelectValue /></SelectTrigger>
            <SelectContent>
              {columns.map((column) => <SelectItem value={String(column.ordinal)} key={column.ordinal}>{column.originalName}</SelectItem>)}
            </SelectContent>
          </Select>
        </label>
        <button className="suna-primary-button" onClick={() => void run()} disabled={busy}>
          {busy ? <Loader2 className="suna-spin" size={15} /> : <Play size={15} />}
          {busy ? "计算中..." : "计算重要性"}
        </button>
      </div>
      {error && <p className="suna-data-cleaning-error" role="alert">{error}</p>}
      {importance.length > 0 && (
        <div className="suna-prediction-importance">
          {importance.map((item) => (
            <div key={item.name}>
              <span>{item.name}</span>
              <i><b style={{ width: `${Math.max(2, item.share * 100)}%` }} /></i>
              <strong>{(item.share * 100).toFixed(1)}%</strong>
            </div>
          ))}
        </div>
      )}
      {explanation && <ShapBarChart explanation={explanation} />}
    </div>
  );
}
