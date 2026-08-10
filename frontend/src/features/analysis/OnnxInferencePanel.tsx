import { Boxes, LoaderCircle, Play, RotateCcw, Square, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import {
  desktop,
  type BackgroundTask,
  type ComputeOnnxPredictionResult,
} from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";

const POLL_INTERVAL_MS = 500;
const MAX_PREVIEW_ROWS = 20;

const MANIFEST_TEMPLATE = JSON.stringify(
  {
    model_id: "",
    model_version: "",
    inputs: [{ name: "X", dtype: "float32", shape: [-1, 1] }],
    outputs: [{ name: "Y", dtype: "float32", shape: [-1, 1] }],
    preprocessing: { feature_names: [], means: [], scales: [] },
    applicability_range: [],
    confidence: { kind: "applicability_distance" },
  },
  null,
  2,
);

const terminal = (state: BackgroundTask["state"]) =>
  state === "completed" || state === "failed" || state === "cancelled" || state === "interrupted";

const taskStateKeys: Record<BackgroundTask["state"], MessageKey> = {
  queued: "analysisPredictionQueued",
  running: "analysisPredictionRunning",
  waiting_external: "analysisPredictionWaitingExternal",
  paused: "analysisPredictionPaused",
  completed: "analysisPredictionCompleted",
  failed: "analysisPredictionFailed",
  cancelled: "analysisPredictionCancelled",
  interrupted: "analysisPredictionInterrupted",
};

function formatCell(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? String(value) : "-";
}

function formatRow(row: unknown) {
  if (Array.isArray(row)) return row.map(formatCell).join(", ");
  return formatCell(row);
}

function parseFeatures(text: string): number[][] | null {
  const rows = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  if (rows.length === 0) return null;
  const matrix: number[][] = [];
  let width: number | null = null;
  for (const row of rows) {
    const values = row.split(/[,\s]+/).map((item) => Number(item));
    if (values.some((value) => !Number.isFinite(value))) return null;
    if (width === null) width = values.length;
    if (values.length !== width) return null;
    matrix.push(values);
  }
  return matrix;
}

export default function OnnxInferencePanel() {
  const { t } = useLocale();
  const [modelPath, setModelPath] = useState<string | null>(null);
  const [modelSha256, setModelSha256] = useState<string | null>(null);
  const [manifestText, setManifestText] = useState(MANIFEST_TEMPLATE);
  const [featuresText, setFeaturesText] = useState("");
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [result, setResult] = useState<ComputeOnnxPredictionResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [picking, setPicking] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);

  useEffect(() => {
    let mounted = true;
    void desktop.listBackgroundTasks().then((tasks) => {
      const recovered = tasks
        .filter((candidate) => candidate.kind === "compute_predict_onnx")
        .sort((left, right) => right.updated_at.localeCompare(left.updated_at))[0];
      if (mounted && recovered) setTask(recovered);
    }).catch((cause) => {
      if (mounted) setError(cause instanceof Error ? cause.message : t("analysisPredictionRefreshError"));
    });
    return () => {
      mounted = false;
    };
  }, [t]);

  const taskId = task?.id ?? null;
  const taskState = task?.state ?? null;

  useEffect(() => {
    if (!taskId || !taskState) return;
    if (terminal(taskState)) {
      if (taskState !== "completed") return;
      let mounted = true;
      void desktop.getComputeOnnxPredictionResult(taskId).then((next) => {
        if (mounted) setResult(next);
      }).catch((cause) => {
        if (mounted) setError(cause instanceof Error ? cause.message : t("analysisPredictionRefreshError"));
      });
      return () => {
        mounted = false;
      };
    }

    let mounted = true;
    const refresh = async () => {
      try {
        const tasks = await desktop.listBackgroundTasks();
        const current = tasks.find((candidate) => candidate.id === taskId);
        if (!mounted || !current) return;
        setTask(current);
        if (current.state === "completed") {
          const next = await desktop.getComputeOnnxPredictionResult(current.id);
          if (mounted) setResult(next);
        }
      } catch (cause) {
        if (mounted) setError(cause instanceof Error ? cause.message : t("analysisPredictionRefreshError"));
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_INTERVAL_MS);
    return () => {
      mounted = false;
      window.clearInterval(timer);
    };
  }, [taskId, taskState, t]);

  const pickModel = async () => {
    if (picking) return;
    setPicking(true);
    setError(null);
    try {
      const selected = await desktop.openFileDialog({
        multiple: false,
        filters: [{ name: "ONNX models", extensions: ["onnx"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      const hash = await desktop.hashOnnxModelFile(selected);
      setModelPath(selected);
      setModelSha256(hash);
      setResult(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisOnnxHashError"));
    } finally {
      setPicking(false);
    }
  };

  const predict = async () => {
    if (busy || !modelPath || !modelSha256) return;
    let manifest: unknown;
    try {
      manifest = JSON.parse(manifestText);
    } catch {
      setError(t("analysisOnnxManifestInvalid"));
      return;
    }
    if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
      setError(t("analysisOnnxManifestInvalid"));
      return;
    }
    const features = parseFeatures(featuresText);
    if (!features) {
      setError(featuresText.trim() ? t("analysisOnnxFeaturesInvalid") : t("analysisOnnxFeaturesEmpty"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const queued = await desktop.predictOnnxModel({
        modelPath,
        modelSha256,
        manifest: manifest as Record<string, unknown>,
        features,
      });
      setTask(queued);
      setResult(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisOnnxError"));
      setTask(null);
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    if (!task || !task.can_cancel || actionBusy) return;
    setActionBusy(true);
    setError(null);
    try {
      setTask(await desktop.cancelBackgroundTask(task.id));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisPredictionActionError"));
    } finally {
      setActionBusy(false);
    }
  };

  const retry = async () => {
    if (!task || !task.can_retry || actionBusy) return;
    setActionBusy(true);
    setError(null);
    setResult(null);
    try {
      setTask(await desktop.retryBackgroundTask(task.id));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisPredictionActionError"));
    } finally {
      setActionBusy(false);
    }
  };

  const predictionRows = result
    ? Array.isArray(result.predictions[0]) || Array.isArray(result.predictions)
      ? (Array.isArray(result.predictions[0]) ? (result.predictions as number[][]) : [result.predictions as number[]])
      : []
    : [];

  return (
    <section className="bloomery-onnx-panel" data-testid="onnx-inference-panel" aria-labelledby="onnx-heading">
      <div className="bloomery-section-heading">
        <div>
          <p className="bloomery-eyebrow">INFER-02</p>
          <h2 id="onnx-heading">{t("analysisOnnxTitle")}</h2>
        </div>
        <Boxes size={18} aria-hidden="true" />
      </div>
      <p className="bloomery-analysis-copy">{t("analysisOnnxCopy")}</p>

      <div className="bloomery-onnx-model-row">
        <button type="button" onClick={() => void pickModel()} disabled={picking} data-testid="onnx-pick-model">
          {picking ? <LoaderCircle size={15} className="bloomery-spin" aria-hidden="true" /> : <Boxes size={15} aria-hidden="true" />}
          <span>{picking ? t("analysisOnnxPicking") : t("analysisOnnxPickModel")}</span>
        </button>
        {modelPath && (
          <span className="bloomery-onnx-model-path" data-testid="onnx-model-path" title={modelPath}>
            {modelPath.split(/[\\/]/).pop()}
            <code data-testid="onnx-model-hash">{modelSha256 ? `${modelSha256.slice(0, 12)}…` : ""}</code>
          </span>
        )}
      </div>

      <label htmlFor="onnx-manifest">{t("analysisOnnxManifest")}</label>
      <textarea
        id="onnx-manifest"
        data-testid="onnx-manifest"
        className="bloomery-onnx-manifest"
        rows={10}
        spellCheck={false}
        value={manifestText}
        onChange={(event) => setManifestText(event.target.value)}
      />

      <label htmlFor="onnx-features">{t("analysisOnnxFeatures")}</label>
      <textarea
        id="onnx-features"
        data-testid="onnx-features"
        className="bloomery-onnx-features"
        rows={4}
        spellCheck={false}
        placeholder={t("analysisOnnxFeaturesPlaceholder")}
        value={featuresText}
        onChange={(event) => setFeaturesText(event.target.value)}
      />

      <button
        type="button"
        className="bloomery-dataset-prediction-button"
        data-testid="onnx-start"
        onClick={() => void predict()}
        disabled={busy || !modelPath || !modelSha256 || Boolean(task && !terminal(task.state))}
      >
        {busy ? <LoaderCircle size={15} className="bloomery-spin" aria-hidden="true" /> : <Play size={15} aria-hidden="true" />}
        <span>{busy ? t("analysisOnnxStarting") : t("analysisOnnxStart")}</span>
      </button>

      {error && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={15} aria-hidden="true" />{error}</p>}
      {task && <output className="bloomery-prediction-task" data-testid="onnx-task">
        <span>{task.id} - {t(taskStateKeys[task.state])} - {task.progress}%</span>
        {task.can_cancel && <button type="button" data-testid="onnx-cancel" onClick={() => void cancel()} disabled={actionBusy} aria-label={t("analysisPredictionCancel")} title={t("analysisPredictionCancel")}><Square size={14} aria-hidden="true" /><span>{actionBusy ? t("analysisPredictionCancelling") : t("analysisPredictionCancel")}</span></button>}
        {task.can_retry && <button type="button" data-testid="onnx-retry" onClick={() => void retry()} disabled={actionBusy} aria-label={t("analysisPredictionRetry")} title={t("analysisPredictionRetry")}><RotateCcw size={14} aria-hidden="true" /><span>{actionBusy ? t("analysisPredictionRetrying") : t("analysisPredictionRetry")}</span></button>}
      </output>}

      {result && <section className="bloomery-prediction-result" data-testid="onnx-result" aria-labelledby="onnx-result-heading">
        <h5 id="onnx-result-heading">{t("analysisOnnxResult")}</h5>
        <dl>
          <div><dt>{t("analysisTrainingModel")}</dt><dd>{result.model_id} / {result.model_version}</dd></div>
          <div><dt>{t("analysisOnnxOpset")}</dt><dd>{result.opset_version}</dd></div>
          <div><dt>{t("analysisOnnxOperators")}</dt><dd>{result.operators.join(", ")}</dd></div>
          <div><dt>SHA-256</dt><dd className="bloomery-onnx-hash">{result.model_sha256}</dd></div>
        </dl>
        <h6>{t("analysisOnnxPredictions")}</h6>
        <ul className="bloomery-onnx-predictions" data-testid="onnx-predictions">
          {predictionRows.slice(0, MAX_PREVIEW_ROWS).map((row, index) => (
            <li key={`prediction-${index}`}>
              <span>{formatRow(row)}</span>
              {result.confidence && typeof result.confidence[index] === "number" && (
                <em>{t("analysisOnnxConfidence")} {result.confidence[index].toFixed(3)}</em>
              )}
            </li>
          ))}
        </ul>
        {result.applicability_warnings.slice(0, MAX_PREVIEW_ROWS).map((warning, index) => (
          <p className="bloomery-analysis-warning" data-testid={`onnx-warning-${index}`} key={`${warning.row}-${warning.index}`}>
            <TriangleAlert size={14} aria-hidden="true" />
            {warning.feature}: {t("analysisPredictionOutsideRange")}
          </p>
        ))}
      </section>}
    </section>
  );
}
