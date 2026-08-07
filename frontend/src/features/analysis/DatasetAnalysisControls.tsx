import type { SteelDatasetRecord } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

type Props = {
  dataset: SteelDatasetRecord;
  groupByColumn: number | null;
  correlationColumns: number[];
  onGroupByColumnChange: (ordinal: number | null) => void;
  onCorrelationColumnToggle: (ordinal: number) => void;
};

export default function DatasetAnalysisControls({
  dataset,
  groupByColumn,
  correlationColumns,
  onGroupByColumnChange,
  onCorrelationColumnToggle,
}: Props) {
  const { t } = useLocale();
  const numericColumns = dataset.preview.columns
    .map((column, ordinal) => ({ column, ordinal }))
    .filter(({ column }) => column.inferredType === "number");

  return (
    <div className="bloomery-dataset-analysis-controls" data-testid={`dataset-analysis-controls-${dataset.id}`}>
      <label>
        <span>{t("analysisDatasetGroupBy")}</span>
        <select
          data-testid={`dataset-group-by-${dataset.id}`}
          value={groupByColumn === null ? "" : String(groupByColumn)}
          onChange={(event) => onGroupByColumnChange(event.target.value ? Number(event.target.value) : null)}
        >
          <option value="">{t("analysisDatasetNoGrouping")}</option>
          {dataset.preview.columns.map((column, ordinal) => (
            <option key={ordinal} value={ordinal}>{column.name}</option>
          ))}
        </select>
      </label>
      <fieldset>
        <legend>{t("analysisDatasetCorrelation")}</legend>
        <div className="bloomery-dataset-correlation-options">
          {numericColumns.map(({ column, ordinal }) => (
            <label key={ordinal}>
              <input
                type="checkbox"
                data-testid={`dataset-correlation-${dataset.id}-${ordinal}`}
                checked={correlationColumns.includes(ordinal)}
                onChange={() => onCorrelationColumnToggle(ordinal)}
              />
              <span>{column.name}</span>
            </label>
          ))}
          {numericColumns.length === 0 && <span>{t("analysisDatasetNoNumericColumns")}</span>}
        </div>
      </fieldset>
    </div>
  );
}
