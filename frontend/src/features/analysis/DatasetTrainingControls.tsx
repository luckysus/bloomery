import { BrainCircuit, Check, LoaderCircle, Play, RotateCcw, Square, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import {
  desktop,
  type BackgroundTask,
  type ComputeTrainingResult,
  type SteelDatasetRecord,
  type TrainSteelDatasetRequest,
} from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";
import DatasetPredictionControls from "./DatasetPredictionControls";
import OptimizationPanel from "./OptimizationPanel";

type Props = {
  dataset: SteelDatasetRecord;
};

const POLL_INTERVAL_MS = 500;
type TrainingAlgorithm = NonNullable<TrainSteelDatasetRequest["algorithm"]>;

const trainingAlgorithms: Array<{ value: TrainingAlgorithm; label: MessageKey }> = [
  { value: "linear_regression", label: "analysisTrainingAlgorithmLinear" },
  { value: "elasticnet", label: "analysisTrainingAlgorithmElasticnet" },
  { value: "random_forest", label: "analysisTrainingAlgorithmRandomForest" },
  { value: "hist_gradient_boosting", label: "analysisTrainingAlgorithmHistGradientBoosting" },
];

const taskStateKeys: Record<BackgroundTask["state"], MessageKey> = {
  queued: "analysisTrainingQueued",
  running: "analysisTrainingRunning",
  waiting_external: "analysisTrainingWaitingExternal",
  paused: "analysisTrainingPaused",
  completed: "analysisTrainingCompleted",
  failed: "analysisTrainingFailed",
  cancelled: "analysisTrainingCancelled",
  interrupted: "analysisTrainingInterrupted",
};

function isTerminal(state: BackgroundTask["state"]) {
  return state === "completed" || state === "failed" || state === "cancelled" || state === "interrupted";
}

function formatMetric(value: unknown) {
  if (typeof value === "number") return Number.isFinite(value) ? String(value) : "-";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value) ?? "-";
  } catch {
    return "-";
  }
}

function resultRange(result: ComputeTrainingResult) {
  return result.artifact.applicability_range
    .map(({ min, max }) => `${min ?? "-"} - ${max ?? "-"}`)
    .join(", ");
}

export default function DatasetTrainingControls({ dataset }: Props) {
  const { t } = useLocale();
  const numericColumns = dataset.columns.filter((column) => column.inferredType === "number");
  const defaultTarget = numericColumns[numericColumns.length - 1]?.ordinal ?? null;
  const [targetColumn, setTargetColumn] = useState<number | null>(defaultTarget);
  const [featureColumns, setFeatureColumns] = useState<number[]>(() =>
    numericColumns.filter((column) => column.ordinal !== defaultTarget).map((column) => column.ordinal),
  );
  const [algorithm, setAlgorithm] = useState<TrainingAlgorithm>("linear_regression");
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [result, setResult] = useState<ComputeTrainingResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);

  const taskId = task?.id ?? null;
  const taskState = task?.state ?? null;
  const activeTask = Boolean(task && !isTerminal(task.state));
  const trainingBlocked = dataset.truncated;

  useEffect(() => {
    let mounted = true;
    void desktop.listBackgroundTasks().then((tasks) => {
      const recovered = tasks
        .filter((candidate) => (
          (candidate.kind === "compute_train_linear_regression" || candidate.kind === "compute_train_sklearn_model")
          && candidate.dataset_id === dataset.id
        ))
        .sort((left, right) => right.updated_at.localeCompare(left.updated_at))[0];
      if (mounted && recovered) setTask(recovered);
    }).catch((cause) => {
      if (mounted) setError(cause instanceof Error ? cause.message : t("analysisTrainingRefreshError"));
    });
    return () => {
      mounted = false;
    };
  }, [dataset.id, t]);

  useEffect(() => {
    if (!taskId || !taskState || isTerminal(taskState)) {
      if (!taskId || taskState !== "completed") return;
      let mounted = true;
      void desktop.getComputeTrainingResult(taskId).then((next) => {
        if (mounted) setResult(next);
      }).catch((cause) => {
        if (mounted) setError(cause instanceof Error ? cause.message : t("analysisTrainingRefreshError"));
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
          const next = await desktop.getComputeTrainingResult(current.id);
          if (mounted) setResult(next);
        }
      } catch (cause) {
        if (mounted) setError(cause instanceof Error ? cause.message : t("analysisTrainingRefreshError"));
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_INTERVAL_MS);
    return () => {
      mounted = false;
      window.clearInterval(timer);
    };
  }, [taskId, taskState, t]);

  const columnLabel = (ordinal: number) => {
    const column = dataset.columns[ordinal];
    return column?.canonicalField || column?.originalName || String(ordinal);
  };

  const changeTarget = (value: string) => {
    const next = value ? Number(value) : null;
    setTargetColumn(next);
    if (next !== null) setFeatureColumns((current) => current.filter((ordinal) => ordinal !== next));
    setTask(null);
    setResult(null);
    setError(null);
  };

  const toggleFeature = (ordinal: number) => {
    setFeatureColumns((current) => current.includes(ordinal)
      ? current.filter((item) => item !== ordinal)
      : [...current, ordinal].sort((left, right) => left - right));
    setTask(null);
    setResult(null);
    setError(null);
  };

  const changeAlgorithm = (value: string) => {
    if (!trainingAlgorithms.some((candidate) => candidate.value === value)) return;
    setAlgorithm(value as TrainingAlgorithm);
    setTask(null);
    setResult(null);
    setError(null);
  };

  const train = async () => {
    if (trainingBlocked) {
      setError(t("analysisTrainingTruncated"));
      return;
    }
    if (busy || activeTask || targetColumn === null || featureColumns.length === 0) {
      setError(t("analysisTrainingNeedColumns"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const queued = await desktop.trainSteelDataset({
        datasetId: dataset.id,
        targetColumn,
        featureColumns,
        splitPolicy: { kind: "random", validationFraction: 0.2, seed: 0 },
        algorithm,
      });
      setTask(queued);
      setResult(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisTrainingError"));
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
      setError(cause instanceof Error ? cause.message : t("analysisTrainingActionError"));
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
      setError(cause instanceof Error ? cause.message : t("analysisTrainingActionError"));
    } finally {
      setActionBusy(false);
    }
  };

  return (
    <section className="bloomery-dataset-training" data-testid={`training-controls-${dataset.id}`} aria-labelledby={`training-heading-${dataset.id}`}>
      <div className="bloomery-dataset-training-heading">
        <div>
          <p className="bloomery-eyebrow">MODEL-01</p>
          <h3 id={`training-heading-${dataset.id}`}>{t("analysisTrainingTitle")}</h3>
        </div>
        <BrainCircuit size={18} aria-hidden="true" />
      </div>
      <label className="bloomery-training-target">
        <span>{t("analysisTrainingTarget")}</span>
        <select data-testid={`training-target-${dataset.id}`} value={targetColumn === null ? "" : String(targetColumn)} onChange={(event) => changeTarget(event.target.value)} disabled={activeTask || trainingBlocked}>
          <option value="">{t("analysisTrainingChooseTarget")}</option>
          {numericColumns.map((column) => <option key={column.ordinal} value={column.ordinal}>{columnLabel(column.ordinal)}</option>)}
        </select>
      </label>
      <label className="bloomery-training-target">
        <span>{t("analysisTrainingAlgorithm")}</span>
        <select
          data-testid={`training-algorithm-${dataset.id}`}
          value={algorithm}
          onChange={(event) => changeAlgorithm(event.target.value)}
          disabled={activeTask || trainingBlocked}
        >
          {trainingAlgorithms.map((candidate) => (
            <option key={candidate.value} value={candidate.value}>{t(candidate.label)}</option>
          ))}
        </select>
      </label>
      <fieldset className="bloomery-training-features">
        <legend>{t("analysisTrainingFeatures")}</legend>
        <div>
          {numericColumns.filter((column) => column.ordinal !== targetColumn).map((column) => (
            <label key={column.ordinal}>
              <input
                type="checkbox"
                data-testid={`training-feature-${dataset.id}-${column.ordinal}`}
                checked={featureColumns.includes(column.ordinal)}
                onChange={() => toggleFeature(column.ordinal)}
                disabled={activeTask || trainingBlocked}
              />
              <span>{columnLabel(column.ordinal)}</span>
            </label>
          ))}
          {numericColumns.length <= 1 && <span>{t("analysisTrainingNoFeatures")}</span>}
        </div>
      </fieldset>
      {trainingBlocked && <p className="bloomery-analysis-error" data-testid={`training-truncated-${dataset.id}`}><TriangleAlert size={15} aria-hidden="true" />{t("analysisTrainingTruncated")}</p>}
      <button className="bloomery-dataset-training-button" type="button" onClick={() => void train()} disabled={busy || activeTask || trainingBlocked}>
        {busy ? <LoaderCircle className="bloomery-spin" size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
        <span>{busy ? t("analysisTrainingStarting") : t("analysisTrainingStart")}</span>
      </button>
      {error && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={15} aria-hidden="true" />{error}</p>}
      {task && <output className="bloomery-training-task" data-testid={`training-task-${dataset.id}`}>
        <span>{task.id} - {t(taskStateKeys[task.state])} - {task.progress}%</span>
        {task.can_cancel && <button type="button" data-testid={`training-cancel-${dataset.id}`} onClick={() => void cancel()} disabled={actionBusy} aria-label={t("analysisTrainingCancel")} title={t("analysisTrainingCancel")}><Square size={14} aria-hidden="true" /><span>{actionBusy ? t("analysisTrainingCancelling") : t("analysisTrainingCancel")}</span></button>}
        {task.can_retry && <button type="button" data-testid={`training-retry-${dataset.id}`} onClick={() => void retry()} disabled={actionBusy} aria-label={t("analysisTrainingRetry")} title={t("analysisTrainingRetry")}><RotateCcw size={14} aria-hidden="true" /><span>{actionBusy ? t("analysisTrainingRetrying") : t("analysisTrainingRetry")}</span></button>}
      </output>}
      {result && <section className="bloomery-training-result" data-testid={`training-result-${dataset.id}`} aria-labelledby={`training-result-heading-${dataset.id}`}>
        <div className="bloomery-training-result-heading">
          <div><p className="bloomery-eyebrow">MODEL OUTPUT</p><h4 id={`training-result-heading-${dataset.id}`}>{t("analysisTrainingResult")}</h4></div>
          <Check size={16} aria-hidden="true" />
        </div>
        <dl className="bloomery-training-result-details">
          <div><dt>{t("analysisTrainingModel")}</dt><dd>{result.artifact.model_id}</dd></div>
          <div><dt>{t("analysisTrainingModelType")}</dt><dd>{result.artifact.model_type}</dd></div>
          <div><dt>{t("analysisTrainingFeatures")}</dt><dd>{result.artifact.feature_names.join(", ") || "-"}</dd></div>
          <div><dt>{t("analysisTrainingApplicability")}</dt><dd>{resultRange(result) || "-"}</dd></div>
          {Object.entries(result.artifact.metrics).map(([key, value]) => <div key={key} data-testid={`training-metric-${key}-${dataset.id}`}><dt>{key}</dt><dd>{formatMetric(value)}</dd></div>)}
        </dl>
      </section>}
      {result && <DatasetPredictionControls datasetId={dataset.id} trainingResult={result} />}
      {result?.artifact.model_type === "linear_regression" && (
        <OptimizationPanel datasetId={dataset.id} trainingResult={result} />
      )}
    </section>
  );
}
