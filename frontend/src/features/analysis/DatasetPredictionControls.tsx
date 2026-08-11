import { Gauge, LoaderCircle, Play, RotateCcw, Square, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { desktop, type BackgroundTask, type ComputePredictionResult, type ComputeTrainingResult } from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";

type Props = {
  datasetId: string;
  trainingResult: ComputeTrainingResult;
};

const POLL_INTERVAL_MS = 500;

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

function displayValue(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? String(value) : "-";
}

export default function DatasetPredictionControls({ datasetId, trainingResult }: Props) {
  const { t } = useLocale();
  const [values, setValues] = useState(() => trainingResult.artifact.feature_names.map(() => ""));
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [result, setResult] = useState<ComputePredictionResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);

  useEffect(() => {
    setValues(trainingResult.artifact.feature_names.map(() => ""));
    setTask(null);
    setResult(null);
    setError(null);
  }, [trainingResult.task_id]);

  useEffect(() => {
    let mounted = true;
    void desktop.listBackgroundTasks().then((tasks) => {
      const recovered = tasks
        .filter((candidate) => (
          (candidate.kind === "compute_predict_linear_regression" || candidate.kind === "compute_predict_trained_model")
          && candidate.dataset_id === datasetId
        ))
        .sort((left, right) => right.updated_at.localeCompare(left.updated_at))[0];
      if (mounted && recovered) setTask(recovered);
    }).catch((cause) => {
      if (mounted) setError(cause instanceof Error ? cause.message : t("analysisPredictionRefreshError"));
    });
    return () => {
      mounted = false;
    };
  }, [datasetId, t]);

  const taskId = task?.id ?? null;
  const taskState = task?.state ?? null;

  useEffect(() => {
    if (!taskId || !taskState) return;
    if (terminal(taskState)) {
      if (taskState !== "completed") return;
      let mounted = true;
      void desktop.getComputePredictionResult(taskId).then((next) => {
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
          const next = await desktop.getComputePredictionResult(current.id);
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

  const predict = async () => {
    if (busy) return;
    let featureValues: number[];
    try {
      featureValues = values.map((value, index) => {
        const number = Number(value);
        if (!value.trim() || !Number.isFinite(number)) throw new Error(`${trainingResult.artifact.feature_names[index]}: ${t("analysisPredictionInvalidValue")}`);
        return number;
      });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisPredictionInvalidValue"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const queued = await desktop.predictSteelModel({
        datasetId,
        trainingTaskId: trainingResult.task_id,
        featureValues,
      });
      setTask(queued);
      setResult(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisPredictionError"));
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

  return (
    <section className="bloomery-dataset-prediction" data-testid="prediction-controls" aria-labelledby="prediction-heading">
      <div className="bloomery-dataset-prediction-heading">
        <div>
          <p className="bloomery-eyebrow">PREDICT-01</p>
          <h4 id="prediction-heading">{t("analysisPredictionTitle")}</h4>
        </div>
        <Gauge size={16} aria-hidden="true" />
      </div>
      <div className="bloomery-prediction-inputs">
        {trainingResult.artifact.feature_names.map((name, index) => {
          const range = trainingResult.artifact.applicability_range[index];
          return (
            <label key={`${name}-${index}`}>
              <span>{name}</span>
              <input
                data-testid={`prediction-input-${index}`}
                inputMode="decimal"
                value={values[index] ?? ""}
                onChange={(event) => setValues((current) => current.map((item, itemIndex) => itemIndex === index ? event.target.value : item))}
                placeholder={range ? `${range.min ?? "-"} - ${range.max ?? "-"}` : undefined}
                aria-label={`${name} ${t("analysisPredictionInput")}`}
              />
            </label>
          );
        })}
      </div>
      <button type="button" className="bloomery-dataset-prediction-button" onClick={() => void predict()} disabled={busy || Boolean(task && !terminal(task.state))}>
        {busy ? <LoaderCircle size={15} className="bloomery-spin" aria-hidden="true" /> : <Play size={15} aria-hidden="true" />}
        <span>{busy ? t("analysisPredictionStarting") : t("analysisPredictionStart")}</span>
      </button>
      {error && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={15} aria-hidden="true" />{error}</p>}
      {task && <output className="bloomery-prediction-task" data-testid="prediction-task">
        <span>{task.id} - {t(taskStateKeys[task.state])} - {task.progress}%</span>
        {task.can_cancel && <button type="button" data-testid="prediction-cancel" onClick={() => void cancel()} disabled={actionBusy} aria-label={t("analysisPredictionCancel")} title={t("analysisPredictionCancel")}><Square size={14} aria-hidden="true" /><span>{actionBusy ? t("analysisPredictionCancelling") : t("analysisPredictionCancel")}</span></button>}
        {task.can_retry && <button type="button" data-testid="prediction-retry" onClick={() => void retry()} disabled={actionBusy} aria-label={t("analysisPredictionRetry")} title={t("analysisPredictionRetry")}><RotateCcw size={14} aria-hidden="true" /><span>{actionBusy ? t("analysisPredictionRetrying") : t("analysisPredictionRetry")}</span></button>}
      </output>}
      {result && <section className="bloomery-prediction-result" data-testid="prediction-result" aria-labelledby="prediction-result-heading">
        <h5 id="prediction-result-heading">{t("analysisPredictionResult")}</h5>
        <dl>
          <div><dt>{t("analysisTrainingModel")}</dt><dd>{result.model_id}</dd></div>
          <div><dt>{t("analysisPredictionOutput")}</dt><dd>{result.predictions.map(displayValue).join(", ")}</dd></div>
          <div><dt>{t("analysisPredictionInputs")}</dt><dd>{result.input_values.map(displayValue).join(", ")}</dd></div>
        </dl>
        {result.applicability_warnings.map((warning, index) => <p className="bloomery-analysis-warning" data-testid={`prediction-warning-${index}`} key={`${warning.feature}-${warning.index}`}><TriangleAlert size={14} aria-hidden="true" />{warning.feature}: {t("analysisPredictionOutsideRange")}</p>)}
      </section>}
    </section>
  );
}
