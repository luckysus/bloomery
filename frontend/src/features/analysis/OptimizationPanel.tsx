import { LoaderCircle, Play, RotateCcw, Route, Square, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  desktop,
  type BackgroundTask,
  type ComputeOptimizationResult,
  type ComputeTrainingResult,
} from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";

const POLL_INTERVAL_MS = 500;
const MAX_RECOMMENDATIONS = 8;

type Props = {
  datasetId: string;
  trainingResult: ComputeTrainingResult;
};

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

function isTerminal(state: BackgroundTask["state"]) {
  return state === "completed" || state === "failed" || state === "cancelled" || state === "interrupted";
}

function formatValue(value: number) {
  return Number.isFinite(value) ? String(Number(value.toFixed(4))) : "-";
}

export default function OptimizationPanel({ datasetId, trainingResult }: Props) {
  const { t } = useLocale();
  const featureNames = trainingResult.artifact.feature_names;
  const ranges = trainingResult.artifact.applicability_range;

  const [direction, setDirection] = useState<"minimize" | "maximize">("minimize");
  const [objectives, setObjectives] = useState<number[]>(() => (featureNames.length > 0 ? [0] : []));
  const [boundsMin, setBoundsMin] = useState<string[]>(() =>
    ranges.map((range) => (range.min !== null ? String(range.min) : "")),
  );
  const [boundsMax, setBoundsMax] = useState<string[]>(() =>
    ranges.map((range) => (range.max !== null ? String(range.max) : "")),
  );
  const [fixedValues, setFixedValues] = useState<string[]>(() => featureNames.map(() => ""));
  const [constraintEnabled, setConstraintEnabled] = useState(false);
  const [constraintKind, setConstraintKind] = useState<"equality" | "inequality">("inequality");
  const [constraintCoefficients, setConstraintCoefficients] = useState<string[]>(() =>
    featureNames.map(() => "0"),
  );
  const [constraintValue, setConstraintValue] = useState("0");
  const [constraintTolerance, setConstraintTolerance] = useState("0.000001");
  const [trials, setTrials] = useState("48");
  const [seed, setSeed] = useState("0");
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [result, setResult] = useState<ComputeOptimizationResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);

  const taskId = task?.id ?? null;
  const taskState = task?.state ?? null;
  const activeTask = Boolean(task && !isTerminal(task.state));

  useEffect(() => {
    let mounted = true;
    void desktop.listBackgroundTasks().then((tasks) => {
      const recovered = tasks
        .filter((candidate) => candidate.kind === "compute_optimize_constrained" && candidate.dataset_id === datasetId)
        .sort((left, right) => right.updated_at.localeCompare(left.updated_at))[0];
      if (mounted && recovered) setTask(recovered);
    }).catch((cause) => {
      if (mounted) setError(cause instanceof Error ? cause.message : t("analysisPredictionRefreshError"));
    });
    return () => {
      mounted = false;
    };
  }, [datasetId, t]);

  useEffect(() => {
    if (!taskId || !taskState) return;
    if (isTerminal(taskState)) {
      if (taskState !== "completed") return;
      let mounted = true;
      void desktop.getComputeOptimizationResult(taskId).then((next) => {
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
          const next = await desktop.getComputeOptimizationResult(current.id);
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

  const toggleObjective = (index: number) => {
    setObjectives((current) => current.includes(index)
      ? current.filter((item) => item !== index)
      : [...current, index].sort((left, right) => left - right));
    setResult(null);
  };

  const start = async () => {
    if (busy || activeTask) return;
    if (objectives.length === 0 || objectives.length > 4) {
      setError(t("analysisOptimizationNeedObjectives"));
      return;
    }
    const bounds: Array<{ min: number; max: number }> = [];
    for (let index = 0; index < featureNames.length; index += 1) {
      const minimum = Number(boundsMin[index]);
      const maximum = Number(boundsMax[index]);
      if (!Number.isFinite(minimum) || !Number.isFinite(maximum) || minimum > maximum) {
        setError(t("analysisOptimizationBoundsInvalid"));
        return;
      }
      bounds.push({ min: minimum, max: maximum });
    }
    const fixed: Array<number | null> = [];
    for (const raw of fixedValues) {
      if (!raw.trim()) {
        fixed.push(null);
        continue;
      }
      const value = Number(raw);
      if (!Number.isFinite(value)) {
        setError(t("analysisOptimizationFixedInvalid"));
        return;
      }
      fixed.push(value);
    }
    const constraints: Array<{ kind: "equality" | "inequality"; coefficients: number[]; value: number; tolerance?: number }> = [];
    if (constraintEnabled) {
      const coefficients = constraintCoefficients.map((raw) => Number(raw));
      const target = Number(constraintValue);
      const tolerance = Number(constraintTolerance);
      if (
        coefficients.some((value) => !Number.isFinite(value))
        || coefficients.every((value) => value === 0)
        || !Number.isFinite(target)
        || !Number.isFinite(tolerance)
        || tolerance < 0
      ) {
        setError(t("analysisOptimizationConstraintInvalid"));
        return;
      }
      constraints.push({ kind: constraintKind, coefficients, value: target, tolerance });
    }
    const trialCount = Number(trials);
    const seedValue = Number(seed);
    if (!Number.isInteger(trialCount) || trialCount < 1 || trialCount > 500 || !Number.isInteger(seedValue)) {
      setError(t("analysisOptimizationTrialsInvalid"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const queued = await desktop.optimizeSteelProcess({
        datasetId,
        trainingTaskId: trainingResult.task_id,
        direction,
        objectiveColumns: objectives,
        bounds,
        fixedValues: fixed,
        constraints,
        trials: trialCount,
        seed: seedValue,
      });
      setTask(queued);
      setResult(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisOptimizationError"));
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

  const recommendations = useMemo(
    () => (result ? result.recommendations.slice(0, MAX_RECOMMENDATIONS) : []),
    [result],
  );

  return (
    <section className="bloomery-optimization-panel" data-testid={`optimization-panel-${datasetId}`} aria-labelledby={`optimization-heading-${datasetId}`}>
      <div className="bloomery-section-heading">
        <div>
          <h4 id={`optimization-heading-${datasetId}`}>{t("analysisOptimizationTitle")}</h4>
        </div>
        <Route size={18} aria-hidden="true" />
      </div>

      <fieldset className="bloomery-optimization-direction">
        <legend>{t("analysisOptimizationDirection")}</legend>
        <div className="bloomery-segmented-control" role="group" aria-label={t("analysisOptimizationDirection")}>
          {(["minimize", "maximize"] as const).map((value) => (
            <button
              key={value}
              type="button"
              aria-pressed={direction === value}
              className={direction === value ? "is-selected" : ""}
              onClick={() => { setDirection(value); setResult(null); }}
              disabled={activeTask}
            >
              {t(value === "minimize" ? "analysisOptimizationMinimize" : "analysisOptimizationMaximize")}
            </button>
          ))}
        </div>
      </fieldset>

      <fieldset className="bloomery-training-features">
        <legend>{t("analysisOptimizationObjectives")}</legend>
        <div>
          {featureNames.map((name, index) => (
            <label key={name}>
              <input
                type="checkbox"
                data-testid={`optimization-objective-${datasetId}-${index}`}
                checked={objectives.includes(index)}
                onChange={() => toggleObjective(index)}
                disabled={activeTask}
              />
              <span>{name}</span>
            </label>
          ))}
        </div>
      </fieldset>

      <div className="bloomery-optimization-bounds">
        <p>{t("analysisOptimizationBounds")}</p>
        {featureNames.map((name, index) => (
          <label key={name}>
            <span>{name}</span>
            <input
              inputMode="decimal"
              type="number"
              step="any"
              data-testid={`optimization-bound-min-${datasetId}-${index}`}
              value={boundsMin[index] ?? ""}
              onChange={(event) => setBoundsMin((current) => current.map((item, position) => (position === index ? event.target.value : item)))}
              disabled={activeTask}
              aria-label={`${name} min`}
            />
            <input
              inputMode="decimal"
              type="number"
              step="any"
              data-testid={`optimization-bound-max-${datasetId}-${index}`}
              value={boundsMax[index] ?? ""}
              onChange={(event) => setBoundsMax((current) => current.map((item, position) => (position === index ? event.target.value : item)))}
              disabled={activeTask}
              aria-label={`${name} max`}
            />
          </label>
        ))}
      </div>

      <div className="bloomery-optimization-bounds">
        <p>{t("analysisOptimizationFixed")}</p>
        {featureNames.map((name, index) => (
          <label key={name}>
            <span>{name}</span>
            <input
              inputMode="decimal"
              type="number"
              step="any"
              data-testid={`optimization-fixed-${datasetId}-${index}`}
              value={fixedValues[index] ?? ""}
              onChange={(event) => setFixedValues((current) => current.map((item, position) => (position === index ? event.target.value : item)))}
              disabled={activeTask}
              aria-label={`${name} fixed`}
            />
          </label>
        ))}
      </div>

      <label className="bloomery-optimization-constraint-toggle">
        <input
          type="checkbox"
          data-testid={`optimization-constraint-toggle-${datasetId}`}
          checked={constraintEnabled}
          onChange={(event) => { setConstraintEnabled(event.target.checked); setResult(null); }}
          disabled={activeTask}
        />
        <span>{t("analysisOptimizationConstraintEnable")}</span>
      </label>
      {constraintEnabled && (
        <div className="bloomery-optimization-constraint">
          <label>
            <span>{t("analysisOptimizationConstraintKind")}</span>
            <select
              data-testid={`optimization-constraint-kind-${datasetId}`}
              value={constraintKind}
              onChange={(event) => setConstraintKind(event.target.value as "equality" | "inequality")}
              disabled={activeTask}
            >
              <option value="inequality">{t("analysisOptimizationConstraintInequality")}</option>
              <option value="equality">{t("analysisOptimizationConstraintEquality")}</option>
            </select>
          </label>
          {featureNames.map((name, index) => (
            <label key={name}>
              <span>{name}</span>
              <input
                inputMode="decimal"
                type="number"
                step="any"
                data-testid={`optimization-constraint-coefficient-${datasetId}-${index}`}
                value={constraintCoefficients[index] ?? "0"}
                onChange={(event) => setConstraintCoefficients((current) => current.map((item, position) => (position === index ? event.target.value : item)))}
                disabled={activeTask}
                aria-label={`${name} coefficient`}
              />
            </label>
          ))}
          <label>
            <span>{t("analysisOptimizationConstraintValue")}</span>
            <input
              inputMode="decimal"
              type="number"
              step="any"
              data-testid={`optimization-constraint-value-${datasetId}`}
              value={constraintValue}
              onChange={(event) => setConstraintValue(event.target.value)}
              disabled={activeTask}
            />
          </label>
          <label>
            <span>{t("analysisOptimizationConstraintTolerance")}</span>
            <input
              inputMode="decimal"
              type="number"
              step="any"
              data-testid={`optimization-constraint-tolerance-${datasetId}`}
              value={constraintTolerance}
              onChange={(event) => setConstraintTolerance(event.target.value)}
              disabled={activeTask}
            />
          </label>
        </div>
      )}

      <div className="bloomery-optimization-run-settings">
        <label>
          <span>{t("analysisOptimizationTrials")}</span>
          <input
            inputMode="numeric"
            type="number"
            step="1"
            data-testid={`optimization-trials-${datasetId}`}
            value={trials}
            onChange={(event) => setTrials(event.target.value)}
            disabled={activeTask}
          />
        </label>
        <label>
          <span>{t("analysisOptimizationSeed")}</span>
          <input
            inputMode="numeric"
            type="number"
            step="1"
            data-testid={`optimization-seed-${datasetId}`}
            value={seed}
            onChange={(event) => setSeed(event.target.value)}
            disabled={activeTask}
          />
        </label>
      </div>

      <button
        type="button"
        className="bloomery-dataset-prediction-button"
        data-testid={`optimization-start-${datasetId}`}
        onClick={() => void start()}
        disabled={busy || activeTask}
      >
        {busy ? <LoaderCircle size={15} className="bloomery-spin" aria-hidden="true" /> : <Play size={15} aria-hidden="true" />}
        <span>{busy ? t("analysisOptimizationStarting") : t("analysisOptimizationStart")}</span>
      </button>

      {error && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={15} aria-hidden="true" />{error}</p>}
      {task && <output className="bloomery-prediction-task" data-testid={`optimization-task-${datasetId}`}>
        <span>{task.id} - {t(taskStateKeys[task.state])} - {task.progress}%</span>
        {task.can_cancel && <button type="button" data-testid={`optimization-cancel-${datasetId}`} onClick={() => void cancel()} disabled={actionBusy} aria-label={t("analysisPredictionCancel")} title={t("analysisPredictionCancel")}><Square size={14} aria-hidden="true" /><span>{actionBusy ? t("analysisPredictionCancelling") : t("analysisPredictionCancel")}</span></button>}
        {task.can_retry && <button type="button" data-testid={`optimization-retry-${datasetId}`} onClick={() => void retry()} disabled={actionBusy} aria-label={t("analysisPredictionRetry")} title={t("analysisPredictionRetry")}><RotateCcw size={14} aria-hidden="true" /><span>{actionBusy ? t("analysisPredictionRetrying") : t("analysisPredictionRetry")}</span></button>}
      </output>}

      {result && <section className="bloomery-prediction-result" data-testid={`optimization-result-${datasetId}`} aria-labelledby={`optimization-result-heading-${datasetId}`}>
        <h5 id={`optimization-result-heading-${datasetId}`}>{t("analysisOptimizationResult")}</h5>
        <dl>
          <div><dt>{t("analysisOptimizationMethod")}</dt><dd>{result.method}</dd></div>
          <div><dt>{t("analysisOptimizationTrialsCompleted")}</dt><dd>{result.trials_completed}</dd></div>
          <div><dt>{t("analysisOptimizationSeed")}</dt><dd>{result.deterministic_seed}</dd></div>
        </dl>
        <ul className="bloomery-optimization-recommendations" data-testid={`optimization-recommendations-${datasetId}`}>
          {recommendations.map((recommendation, index) => (
            <li key={`recommendation-${index}`}>
              <span data-testid={`optimization-recommendation-values-${datasetId}-${index}`}>
                {result.feature_names.map((name) => `${name}=${formatValue(recommendation.values[name] ?? Number.NaN)}`).join(", ")}
              </span>
              <em>
                {t("analysisOptimizationPrediction")} {formatValue(recommendation.prediction)}
                {" · "}
                {t("analysisOptimizationFeasible")}: {recommendation.feasible ? "✓" : "✗"}
              </em>
            </li>
          ))}
        </ul>
      </section>}
    </section>
  );
}
