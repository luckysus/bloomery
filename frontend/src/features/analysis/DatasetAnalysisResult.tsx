import { BarChart3, TriangleAlert } from "lucide-react";
import type { DatasetAnalysis } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

function formatMetric(value: number | null) {
  if (value === null || !Number.isFinite(value)) return "-";
  return Number(value.toFixed(4)).toString();
}

function formatRate(value: number) {
  return `${(value * 100).toFixed(1)}%`;
}

export default function DatasetAnalysisResult({ analysis }: { analysis: DatasetAnalysis }) {
  const { t } = useLocale();
  const columnNames = new Map(analysis.columns.map((column) => [column.ordinal, column.name]));
  return (
    <section className="bloomery-dataset-analysis" data-testid="dataset-analysis-result" aria-labelledby="dataset-analysis-heading">
      <div className="bloomery-section-heading">
        <div>
          <p className="bloomery-eyebrow">PROFILE-01</p>
          <h3 id="dataset-analysis-heading">{t("analysisDatasetAnalysisTitle")}</h3>
        </div>
        <BarChart3 size={19} aria-hidden="true" />
      </div>
      <div className="bloomery-dataset-analysis-summary">
        <div><span>{t("analysisDatasetAnalyzedRows")}</span><strong>{analysis.analyzedRowCount}</strong></div>
        <div><span>{t("analysisDatasetExcludedRows")}</span><strong>{analysis.excludedRowCount}</strong></div>
      </div>
      {analysis.warnings.length > 0 && (
        <p className="bloomery-dataset-warning"><TriangleAlert size={15} aria-hidden="true" />{analysis.warnings.join("; ")}</p>
      )}
      <div className="bloomery-dataset-table-wrap">
        <table className="bloomery-dataset-table bloomery-dataset-analysis-table">
          <thead>
            <tr>
              <th>{t("analysisDatasetColumn")}</th>
              <th>{t("analysisDatasetSamples")}</th>
              <th>{t("analysisDatasetMean")}</th>
              <th>{t("analysisDatasetStdDev")}</th>
              <th>{t("analysisDatasetMedian")}</th>
              <th>{t("analysisDatasetMissingRate")}</th>
              <th>{t("analysisDatasetOutliers")}</th>
              <th>{t("analysisDatasetOutlierRows")}</th>
            </tr>
          </thead>
          <tbody>
            {analysis.columns.map((column) => (
              <tr key={column.ordinal}>
                <td>
                  <strong>{column.name}</strong>
                  {column.canonicalField && <small>{column.canonicalField}{column.unit ? ` / ${column.unit}` : ""}</small>}
                </td>
                <td>{column.sampleCount}</td>
                <td>{formatMetric(column.mean)}</td>
                <td>{formatMetric(column.standardDeviation)}</td>
                <td>{formatMetric(column.median)}</td>
                <td>{formatRate(column.missingRate)}</td>
                <td>{column.outlierCount}</td>
                <td>{column.outlierRows.length > 0 ? column.outlierRows.join(", ") : "-"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {analysis.columns.some((column) => column.topValues.length > 0) && (
        <div className="bloomery-dataset-top-values">
          <span>{t("analysisDatasetTopValues")}</span>
          {analysis.columns.filter((column) => column.topValues.length > 0).map((column) => (
            <p key={column.ordinal}><strong>{column.name}</strong> {column.topValues.map((item) => `${item.value} (${item.count})`).join(" · ")}</p>
          ))}
        </div>
      )}
      {analysis.columns.some((column) => column.distribution.length > 0) && (
        <div className="bloomery-dataset-distributions">
          <div className="bloomery-dataset-subheading"><strong>{t("analysisDatasetDistribution")}</strong></div>
          {analysis.columns.filter((column) => column.distribution.length > 0).map((column) => {
            const peak = Math.max(...column.distribution.map((bin) => bin.count), 1);
            return (
              <div className="bloomery-dataset-distribution" data-testid={`dataset-distribution-${column.ordinal}`} key={column.ordinal}>
                <div className="bloomery-dataset-distribution-heading"><strong>{column.name}</strong><span>{column.unit ?? ""}</span></div>
                <div className="bloomery-dataset-distribution-bars" role="img" aria-label={`${column.name} ${t("analysisDatasetDistribution")}`}>
                  {column.distribution.map((bin) => (
                    <div className="bloomery-dataset-distribution-bin" key={`${bin.lowerBound}-${bin.upperBound}`} title={`${bin.lowerBound} - ${bin.upperBound}: ${bin.count}`}>
                      <span style={{ height: `${bin.count === 0 ? 0 : Math.max(8, (bin.count / peak) * 100)}%` }} />
                      <small>{bin.count}</small>
                    </div>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      )}
      {analysis.groups.length > 0 && (
        <div className="bloomery-dataset-group-summary">
          <div className="bloomery-dataset-subheading"><strong>{t("analysisDatasetGroupSummary")}</strong></div>
          <div className="bloomery-dataset-table-wrap">
            <table className="bloomery-dataset-table">
              <thead><tr><th>{t("analysisDatasetGroupKey")}</th><th>{t("analysisDatasetRows")}</th><th>{t("analysisDatasetGroupMetrics")}</th></tr></thead>
              <tbody>
                {analysis.groups.map((group) => (
                  <tr key={group.key}>
                    <td><strong>{group.key}</strong></td>
                    <td>{group.rowCount}</td>
                    <td>{group.columns.map((column) => `${columnNames.get(column.ordinal) ?? column.ordinal}: ${formatMetric(column.mean)} (${formatMetric(column.min)} - ${formatMetric(column.max)})`).join("; ") || "-"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
      {analysis.correlations.length > 0 && (
        <div className="bloomery-dataset-correlations">
          <div className="bloomery-dataset-subheading"><strong>{t("analysisDatasetCorrelationResults")}</strong></div>
          <div className="bloomery-dataset-table-wrap">
            <table className="bloomery-dataset-table">
              <thead><tr><th>{t("analysisDatasetCorrelationPair")}</th><th>{t("analysisDatasetSamples")}</th><th>{t("analysisDatasetPearson")}</th></tr></thead>
              <tbody>
                {analysis.correlations.map((correlation) => (
                  <tr key={`${correlation.leftOrdinal}-${correlation.rightOrdinal}`}>
                    <td>{columnNames.get(correlation.leftOrdinal) ?? correlation.leftOrdinal} / {columnNames.get(correlation.rightOrdinal) ?? correlation.rightOrdinal}</td>
                    <td>{correlation.sampleCount}</td>
                    <td>{formatMetric(correlation.pearson)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </section>
  );
}
