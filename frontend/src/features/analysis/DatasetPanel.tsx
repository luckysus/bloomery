import { BarChart3, CircleCheck, FolderOpen, LoaderCircle, Power, Save, Table2, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { desktop, type DatasetAnalysis, type DatasetColumnMapping, type DatasetPreview, type SteelDatasetRecord } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import DatasetAnalysisControls from "./DatasetAnalysisControls";
import DatasetAnalysisResult from "./DatasetAnalysisResult";
import DatasetTrainingControls from "./DatasetTrainingControls";

function columnRange(column: DatasetPreview["columns"][number]) {
  return column.min !== null && column.max !== null ? `${column.min} - ${column.max}` : "-";
}

type MappingDraft = { canonicalField: string; unit: string };

export default function DatasetPanel() {
  const { t } = useLocale();
  const [dataset, setDataset] = useState<DatasetPreview | null>(null);
  const [datasetBusy, setDatasetBusy] = useState(false);
  const [datasetError, setDatasetError] = useState<string | null>(null);
  const [datasetSourcePath, setDatasetSourcePath] = useState("");
  const [datasetSaveBusy, setDatasetSaveBusy] = useState(false);
  const [datasetSaveError, setDatasetSaveError] = useState<string | null>(null);
  const [datasetSaved, setDatasetSaved] = useState(false);
  const [savedDatasets, setSavedDatasets] = useState<SteelDatasetRecord[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(true);
  const [datasetActivateBusyId, setDatasetActivateBusyId] = useState<string | null>(null);
  const [datasetAnalysis, setDatasetAnalysis] = useState<DatasetAnalysis | null>(null);
  const [datasetAnalysisBusyId, setDatasetAnalysisBusyId] = useState<string | null>(null);
  const [datasetAnalysisError, setDatasetAnalysisError] = useState<string | null>(null);
  const [datasetActivateError, setDatasetActivateError] = useState<string | null>(null);
  const [datasetMappings, setDatasetMappings] = useState<Record<number, MappingDraft>>({});
  const [groupByColumns, setGroupByColumns] = useState<Record<string, number | null>>({});
  const [correlationColumns, setCorrelationColumns] = useState<Record<string, number[]>>({});

  useEffect(() => {
    let mounted = true;
    void desktop.listSteelDatasets().then((items) => {
      if (mounted) setSavedDatasets(items);
    }).catch(() => {
      if (mounted) setDatasetSaveError(t("analysisDatasetCatalogLoadError"));
    }).finally(() => {
      if (mounted) setCatalogLoading(false);
    });
    return () => {
      mounted = false;
    };
  }, [t]);

  const chooseDataset = async () => {
    setDatasetError(null);
    try {
      const selected = await desktop.openFileDialog({
        directory: false,
        multiple: false,
        title: t("analysisChooseDataset"),
        filters: [{ name: t("analysisDatasetTitle"), extensions: ["csv", "xlsx"] }],
      });
      if (typeof selected !== "string") return;
      setDatasetBusy(true);
      setDatasetSourcePath(selected);
      setDatasetSaved(false);
      setDatasetSaveError(null);
      setDatasetAnalysis(null);
      setDatasetAnalysisError(null);
      const preview = await desktop.previewSteelDataset({ sourcePath: selected });
      const mappings: Record<number, MappingDraft> = {};
      preview.columns.forEach((_, ordinal) => {
        mappings[ordinal] = { canonicalField: "", unit: "" };
      });
      setDatasetMappings(mappings);
      setDataset(preview);
    } catch (cause) {
      setDatasetError(cause instanceof Error ? cause.message : t("analysisDatasetError"));
      setDataset(null);
    } finally {
      setDatasetBusy(false);
    }
  };

  const saveDataset = async () => {
    if (!dataset || !datasetSourcePath || datasetSaveBusy) return;
    setDatasetSaveBusy(true);
    setDatasetSaveError(null);
    try {
      const mappings = Object.entries(datasetMappings).flatMap(([ordinal, mapping]) => {
        const canonicalField = mapping.canonicalField.trim();
        const unit = mapping.unit.trim();
        if (!canonicalField && !unit) return [];
        return [{
          ordinal: Number(ordinal),
          canonicalField: canonicalField || null,
          unit: unit || null,
        } satisfies DatasetColumnMapping];
      });
      const saved = await desktop.saveSteelDataset({
        sourcePath: datasetSourcePath,
        sheet: dataset.selectedSheet,
        mappings,
      });
      setSavedDatasets((current) => [saved, ...current.filter((item) => item.id !== saved.id)]);
      setDatasetSaved(true);
    } catch (cause) {
      setDatasetSaveError(cause instanceof Error ? cause.message : t("analysisDatasetSaveError"));
    } finally {
      setDatasetSaveBusy(false);
    }
  };

  const updateMapping = (ordinal: number, field: keyof MappingDraft, value: string) => {
    setDatasetMappings((current) => ({
      ...current,
      [ordinal]: { ...(current[ordinal] ?? { canonicalField: "", unit: "" }), [field]: value },
    }));
  };

  const activateDataset = async (datasetId: string) => {
    if (datasetActivateBusyId) return;
    setDatasetActivateBusyId(datasetId);
    setDatasetActivateError(null);
    try {
      const activated = await desktop.activateSteelDataset(datasetId);
      setSavedDatasets((current) => current.map((item) => item.id === activated.id ? activated : item));
    } catch (cause) {
      setDatasetActivateError(cause instanceof Error ? cause.message : t("analysisDatasetActivateError"));
    } finally {
      setDatasetActivateBusyId(null);
    }
  };

  const runDatasetAnalysis = async (datasetId: string) => {
    if (datasetAnalysisBusyId) return;
    setDatasetAnalysisBusyId(datasetId);
    setDatasetAnalysisError(null);
    try {
      const request: {
        datasetId: string;
        groupByColumn?: number;
        correlationColumns?: number[];
      } = { datasetId };
      const groupByColumn = groupByColumns[datasetId];
      const selectedCorrelations = correlationColumns[datasetId] ?? [];
      if (groupByColumn !== undefined && groupByColumn !== null) request.groupByColumn = groupByColumn;
      if (selectedCorrelations.length > 0) request.correlationColumns = selectedCorrelations;
      setDatasetAnalysis(await desktop.analyzeSteelDataset(request));
    } catch (cause) {
      setDatasetAnalysis(null);
      setDatasetAnalysisError(cause instanceof Error ? cause.message : t("analysisDatasetAnalysisError"));
    } finally {
      setDatasetAnalysisBusyId(null);
    }
  };

  const updateGroupByColumn = (datasetId: string, ordinal: number | null) => {
    setGroupByColumns((current) => ({ ...current, [datasetId]: ordinal }));
    setDatasetAnalysis(null);
  };

  const toggleCorrelationColumn = (datasetId: string, ordinal: number) => {
    setCorrelationColumns((current) => {
      const selected = current[datasetId] ?? [];
      const next = selected.includes(ordinal)
        ? selected.filter((item) => item !== ordinal)
        : [...selected, ordinal].sort((left, right) => left - right);
      return { ...current, [datasetId]: next };
    });
    setDatasetAnalysis(null);
  };

  return (
    <section className="bloomery-analysis-dataset" aria-labelledby="dataset-preview-heading">
      <div className="bloomery-section-heading">
        <div>
          <p className="bloomery-eyebrow">DATA-01</p>
          <h2 id="dataset-preview-heading">{t("analysisDatasetTitle")}</h2>
        </div>
        <button type="button" className="bloomery-icon-button" onClick={() => void chooseDataset()} disabled={datasetBusy} aria-label={t("analysisChooseDataset")} title={t("analysisChooseDataset")}>
          {datasetBusy ? <LoaderCircle size={17} className="bloomery-spin" aria-hidden="true" /> : <FolderOpen size={17} aria-hidden="true" />}
        </button>
      </div>
      <p className="bloomery-analysis-copy">{t("analysisDatasetCopy")}</p>
      {datasetError && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={16} aria-hidden="true" />{datasetError}</p>}
      {datasetSaveError && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={16} aria-hidden="true" />{datasetSaveError}</p>}
      {datasetAnalysisError && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={16} aria-hidden="true" />{datasetAnalysisError}</p>}
      {datasetActivateError && <p className="bloomery-analysis-error" role="alert"><TriangleAlert size={16} aria-hidden="true" />{datasetActivateError}</p>}
      {dataset && (
        <div className="bloomery-dataset-preview" aria-live="polite">
          <div className="bloomery-dataset-summary">
            <div><span>{t("analysisDatasetFile")}</span><strong>{dataset.sourceName}</strong></div>
            <div><span>{t("analysisDatasetRows")}</span><strong>{dataset.rowCount}</strong></div>
            <div><span>{t("analysisDatasetColumns")}</span><strong>{dataset.columnCount}</strong></div>
            <div><span>{t("analysisDatasetSheet")}</span><strong>{dataset.selectedSheet}</strong></div>
          </div>
          <div className="bloomery-dataset-table-wrap">
            <table className="bloomery-dataset-table">
              <thead><tr><th>{t("analysisDatasetColumn")}</th><th>{t("analysisDatasetType")}</th><th>{t("analysisDatasetMissing")}</th><th>{t("analysisDatasetInvalid")}</th><th>{t("analysisDatasetRange")}</th><th>{t("analysisDatasetCanonical")}</th><th>{t("analysisDatasetUnit")}</th></tr></thead>
              <tbody>{dataset.columns.map((column, ordinal) => <tr key={column.name}><td><strong>{column.name}</strong>{column.duplicate && <span className="bloomery-dataset-warning">{t("analysisDatasetDuplicate")}</span>}</td><td>{column.inferredType}</td><td>{column.missingCount}</td><td>{column.invalidCount}</td><td>{columnRange(column)}</td><td><input data-testid={`dataset-mapping-${ordinal}-canonical`} value={datasetMappings[ordinal]?.canonicalField ?? ""} onChange={(event) => updateMapping(ordinal, "canonicalField", event.target.value)} aria-label={`${column.name} ${t("analysisDatasetCanonical")}`} /></td><td><input data-testid={`dataset-mapping-${ordinal}-unit`} value={datasetMappings[ordinal]?.unit ?? ""} onChange={(event) => updateMapping(ordinal, "unit", event.target.value)} aria-label={`${column.name} ${t("analysisDatasetUnit")}`} /></td></tr>)}</tbody>
            </table>
          </div>
          {dataset.warnings.length > 0 && <p className="bloomery-dataset-warning"><TriangleAlert size={15} aria-hidden="true" />{dataset.warnings.join("; ")}</p>}
          {dataset.sampleRows.length > 0 && <details className="bloomery-dataset-sample"><summary><Table2 size={15} aria-hidden="true" />{t("analysisDatasetSample")}</summary><pre>{dataset.sampleRows.map((row) => row.join(" | ")).join("\n")}</pre></details>}
          <div className="bloomery-dataset-actions">
            <button type="button" className="bloomery-action-primary" data-testid="save-dataset" onClick={() => void saveDataset()} disabled={datasetSaveBusy || datasetSaved}>
              {datasetSaveBusy ? <LoaderCircle size={16} className="bloomery-spin" aria-hidden="true" /> : <Save size={16} aria-hidden="true" />}
              <span>{datasetSaveBusy ? t("analysisDatasetSaving") : datasetSaved ? t("analysisDatasetSaved") : t("analysisDatasetSave")}</span>
            </button>
          </div>
        </div>
      )}
      {!dataset && !datasetBusy && !datasetError && <div className="bloomery-result-empty"><Table2 size={22} aria-hidden="true" /><span>{t("analysisChooseDataset")}</span></div>}
      <div className="bloomery-dataset-catalog" aria-label={t("analysisDatasetCatalog")}>
        <div className="bloomery-dataset-catalog-heading"><strong>{t("analysisDatasetCatalog")}</strong><span>{catalogLoading ? t("loading") : t("items", { count: savedDatasets.length })}</span></div>
        {!catalogLoading && savedDatasets.length === 0 ? <p className="bloomery-dataset-catalog-empty">{t("analysisDatasetCatalogEmpty")}</p> : (
          <div className="bloomery-dataset-catalog-list">
            {savedDatasets.map((item) => (
              <div className="bloomery-dataset-catalog-row" key={item.id}>
                <div><strong>{item.sourceName}</strong><span>{item.selectedSheet} · {item.rowCount} {t("analysisDatasetRows").toLowerCase()}</span></div>
                <span className={`bloomery-dataset-status ${item.mappingState === "ready" ? "is-ready" : "is-draft"}`} data-testid={`dataset-status-${item.id}`}>
                  {item.mappingState === "ready" ? <CircleCheck size={14} aria-hidden="true" /> : null}
                  {item.mappingState === "ready" ? t("analysisDatasetStatusReady") : t("analysisDatasetStatusDraft")}
                </span>
                <DatasetAnalysisControls
                  dataset={item}
                  groupByColumn={groupByColumns[item.id] ?? null}
                  correlationColumns={correlationColumns[item.id] ?? []}
                  onGroupByColumnChange={(ordinal) => updateGroupByColumn(item.id, ordinal)}
                  onCorrelationColumnToggle={(ordinal) => toggleCorrelationColumn(item.id, ordinal)}
                />
                {item.mappingState === "ready" && <DatasetTrainingControls dataset={item} />}
                {item.mappingState === "draft" && <button type="button" className="bloomery-dataset-activate" data-testid={`activate-dataset-${item.id}`} onClick={() => void activateDataset(item.id)} disabled={datasetActivateBusyId !== null} title={t("analysisDatasetActivate")}>
                  {datasetActivateBusyId === item.id ? <LoaderCircle size={15} className="bloomery-spin" aria-hidden="true" /> : <Power size={15} aria-hidden="true" />}
                  <span>{datasetActivateBusyId === item.id ? t("analysisDatasetActivating") : t("analysisDatasetActivate")}</span>
                </button>}
                <button type="button" className="bloomery-dataset-analyze" data-testid={`analyze-dataset-${item.id}`} onClick={() => void runDatasetAnalysis(item.id)} disabled={datasetAnalysisBusyId !== null} title={t("analysisDatasetAnalyze")}>
                  {datasetAnalysisBusyId === item.id ? <LoaderCircle size={15} className="bloomery-spin" aria-hidden="true" /> : <BarChart3 size={15} aria-hidden="true" />}
                  <span>{datasetAnalysisBusyId === item.id ? t("analysisDatasetAnalyzing") : t("analysisDatasetAnalyze")}</span>
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
      {datasetAnalysis && <DatasetAnalysisResult analysis={datasetAnalysis} />}
    </section>
  );
}
