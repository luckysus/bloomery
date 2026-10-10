import { useEffect, useState } from "react";
import { Check, CircleAlert, Loader2, RefreshCw, Star, Trash2 } from "lucide-react";
import { desktop, type SteelDatasetRecord, type SteelModelRecord } from "../../bridge/desktop";
import { algorithmLabel, formatMetricValue } from "./predictionModel";

/**
 * 第 35 章模型中心：已注册模型版本的卡片列表。
 *
 * 每张卡片展示文档要求的字段——模型名称、任务、数据集、R²/MAE/RMSE、
 * 更新时间与版本——并提供设为活动版本与删除（活动版本不可删）两个操作。
 */

type CardMetrics = { r2: number | null; mae: number | null; rmse: number | null } | null;

function parseArtifact(model: SteelModelRecord): Record<string, unknown> | null {
  if (!model.artifact_json) return null;
  try {
    const parsed = JSON.parse(model.artifact_json) as unknown;
    return typeof parsed === "object" && parsed !== null ? (parsed as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

export function modelCardTitle(model: SteelModelRecord): string {
  if (model.kind === "onnx") {
    const manifest = parseManifest(model);
    const modelId = typeof manifest?.model_id === "string" ? String(manifest.model_id) : "";
    return `ONNX 模型${modelId ? ` ${modelId.slice(0, 12)}` : ""}`;
  }
  const artifact = parseArtifact(model);
  const modelType = typeof artifact?.model_type === "string" ? artifact.model_type : null;
  return modelType ? algorithmLabel(modelType) : model.kind;
}

function parseManifest(model: SteelModelRecord): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(model.manifest_json) as unknown;
    return typeof parsed === "object" && parsed !== null ? (parsed as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

export function modelCardMetrics(model: SteelModelRecord): CardMetrics {
  const artifact = parseArtifact(model);
  const metrics = artifact?.metrics as
    | { train?: CardMetrics; validation?: CardMetrics }
    | undefined;
  return metrics?.validation ?? metrics?.train ?? null;
}

function datasetName(lineageId: string, datasets: SteelDatasetRecord[]): string {
  const datasetId = lineageId.includes(":") ? lineageId.slice(lineageId.indexOf(":") + 1) : "";
  return datasets.find((item) => item.id === datasetId)?.sourceName || datasetId || "—";
}

function formatTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export default function ModelRegistryPanel({
  datasets,
  onNotice,
  onError,
}: {
  datasets: SteelDatasetRecord[];
  onNotice: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [models, setModels] = useState<SteelModelRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState("");

  const load = async () => {
    setLoading(true);
    try {
      setModels(await desktop.listAllSteelModels());
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "无法加载模型列表");
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => { void load(); }, []);

  const activate = async (model: SteelModelRecord) => {
    setBusyId(model.id);
    onError("");
    try {
      await desktop.setActiveSteelModel(model.id);
      onNotice(`已将「${modelCardTitle(model)}」v${model.version} 设为活动版本`);
      await load();
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "无法切换活动版本");
    } finally {
      setBusyId("");
    }
  };

  const remove = async (model: SteelModelRecord) => {
    setBusyId(model.id);
    onError("");
    try {
      await desktop.deleteSteelModel(model.id);
      onNotice(`已删除「${modelCardTitle(model)}」v${model.version}`);
      await load();
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "无法删除该模型版本");
    } finally {
      setBusyId("");
    }
  };

  return (
    <section className="suna-registry-panel">
      <div className="suna-registry-head">
        <p>模型中心汇总所有已注册的模型版本：训练完成的模型会自动注册，ONNX 导出的模型在注册后也会出现在这里。</p>
        <button className="suna-ghost-button" onClick={() => void load()} disabled={loading}>
          {loading ? <Loader2 className="suna-spin" size={15} /> : <RefreshCw size={15} />}刷新
        </button>
      </div>
      <div className="suna-management-grid">
        {models.map((model) => {
          const metrics = modelCardMetrics(model);
          const busy = busyId === model.id;
          return (
            <article className="suna-management-card suna-registry-card" key={model.id}>
              <div className="suna-registry-card-head">
                <h2>{modelCardTitle(model)}</h2>
                <span className={`suna-management-badge ${model.is_active ? "" : "is-muted"}`}>
                  {model.is_active ? <><Check size={12} />活动版本</> : `v${model.version}`}
                </span>
              </div>
              <dl className="suna-registry-facts">
                <div><dt>数据集</dt><dd>{datasetName(model.lineage_id, datasets)}</dd></div>
                <div><dt>任务</dt><dd>{model.source_task_id ? model.source_task_id.slice(0, 8) : "—"}</dd></div>
                <div><dt>版本</dt><dd>v{model.version} · {model.kind}</dd></div>
                <div><dt>更新时间</dt><dd>{formatTime(model.created_at)}</dd></div>
                <div><dt>R²</dt><dd>{formatMetricValue(metrics?.r2)}</dd></div>
                <div><dt>MAE</dt><dd>{formatMetricValue(metrics?.mae)}</dd></div>
                <div><dt>RMSE</dt><dd>{formatMetricValue(metrics?.rmse)}</dd></div>
              </dl>
              <div className="suna-registry-actions">
                <button
                  className="suna-ghost-button"
                  onClick={() => void activate(model)}
                  disabled={busy || model.is_active}
                >
                  {busy ? <Loader2 className="suna-spin" size={14} /> : <Star size={14} />}设为活动
                </button>
                <button
                  className="suna-ghost-button"
                  onClick={() => void remove(model)}
                  disabled={busy || model.is_active}
                  title={model.is_active ? "活动版本不可删除，请先切换到其他版本" : "删除该版本"}
                >
                  <Trash2 size={14} />删除
                </button>
              </div>
            </article>
          );
        })}
      </div>
      {models.length === 0 && !loading && (
        <div className="suna-prediction-empty">
          <CircleAlert size={30} />
          <strong>尚无已注册的模型</strong>
          <span>在「单点预测」完成一次训练后，模型会自动注册并出现在这里。</span>
        </div>
      )}
    </section>
  );
}
