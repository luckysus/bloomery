import { BrainCircuit, LoaderCircle, Play, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { desktop, type BackgroundTask, type SteelDatasetRecord } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

type Props = {
  dataset: SteelDatasetRecord;
};

export default function DatasetTrainingControls({ dataset }: Props) {
  const { t } = useLocale();
  const numericColumns = dataset.columns.filter((column) => column.inferredType === "number");
  const defaultTarget = numericColumns[numericColumns.length - 1]?.ordinal ?? null;
  const [targetColumn, setTargetColumn] = useState<number | null>(defaultTarget);
  const [featureColumns, setFeatureColumns] = useState<number[]>(() =>
    numericColumns.filter((column) => column.ordinal !== defaultTarget).map((column) => column.ordinal),
  );
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const columnLabel = (ordinal: number) => {
    const column = dataset.columns[ordinal];
    return column?.canonicalField || column?.originalName || String(ordinal);
  };

  const changeTarget = (value: string) => {
    const next = value ? Number(value) : null;
    setTargetColumn(next);
    if (next !== null) setFeatureColumns((current) => current.filter((ordinal) => ordinal !== next));
    setTask(null);
    setError(null);
  };

  const toggleFeature = (ordinal: number) => {
    setFeatureColumns((current) => current.includes(ordinal)
      ? current.filter((item) => item !== ordinal)
      : [...current, ordinal].sort((left, right) => left - right));
    setTask(null);
    setError(null);
  };

  const train = async () => {
    if (busy || targetColumn === null || featureColumns.length === 0) {
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
      });
      setTask(queued);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("analysisTrainingError"));
      setTask(null);
    } finally {
      setBusy(false);
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
        <select data-testid={`training-target-${dataset.id}`} value={targetColumn === null ? "" : String(targetColumn)} onChange={(event) => changeTarget(event.target.value)}>
          <option value="">{t("analysisTrainingChooseTarget")}</option>
          {numericColumns.map((column) => <option key={column.ordinal} value={column.ordinal}>{columnLabel(column.ordinal)}</option>)}
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
              />
              <span>{columnLabel(column.ordinal)}</span>
            </label>
          ))}
          {numericColumns.length <= 1 && <span>{t("analysisTrainingNoFeatures")}</span>}
        </div>
      </fieldset>
      <button className="bloomery-dataset-training-button" type="button" onClick={() => void train()} disabled={busy}>
        {busy ? <LoaderCircle className="bloomery-spin" size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
        <span>{busy ? t("analysisTrainingStarting") : t("analysisTrainingStart")}</span>
      </button>
      {error && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={15} aria-hidden="true" />{error}</p>}
      {task && <output className="bloomery-training-task" data-testid={`training-task-${dataset.id}`}>
        {task.id} · {task.state} · {task.progress}%
      </output>}
    </section>
  );
}
