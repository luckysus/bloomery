import { Database, PackageCheck, SearchCheck, RotateCcw } from "lucide-react";
import type { IndexHealthReport, StorageHealth } from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";
import { formatBytes } from "./diagnosticsModel";

interface SteelPackageHealth {
  status: "ready" | "error" | "unknown";
  error: string | null;
}

interface DiagnosticsHealthGridProps {
  storage: StorageHealth | null;
  index: IndexHealthReport | null;
  indexError: boolean;
  steelPackage: SteelPackageHealth;
  busySteelPackage: boolean;
  onRetrySteelPackage: () => void;
}

export default function DiagnosticsHealthGrid({
  storage,
  index,
  indexError,
  steelPackage,
  busySteelPackage,
  onRetrySteelPackage,
}: DiagnosticsHealthGridProps) {
  const { t } = useLocale();
  const databaseHealthy = Boolean(storage?.database_ok);
  const indexHealthy = index?.state === "healthy";
  const steelPackageHealthy = steelPackage.status === "ready";
  const steelPackageStatusKey: MessageKey = steelPackageHealthy
    ? "diagnosticsSteelPackageHealthy"
    : steelPackage.status === "error"
      ? "diagnosticsSteelPackageAttention"
      : "diagnosticsSteelPackageUnknown";

  return (
    <div className="bloomery-diagnostics-grid">
      <article className="bloomery-diagnostics-card">
        <div className="bloomery-diagnostics-card-heading">
          <span className="bloomery-diagnostics-card-icon"><Database size={17} aria-hidden="true" /></span>
          <div><p className="bloomery-eyebrow">SQLITE</p><h2>{t("diagnosticsDatabase")}</h2></div>
          <span className={`bloomery-diagnostics-status ${databaseHealthy ? "is-healthy" : "is-warning"}`}>
            {databaseHealthy ? t("diagnosticsDatabaseHealthy") : t("diagnosticsDatabaseAttention")}
          </span>
        </div>
        <dl className="bloomery-diagnostics-details">
          <div><dt>{t("diagnosticsMigration")}</dt><dd>{storage ? `${storage.current_migration_version} / ${storage.latest_migration_version}` : "--"}</dd></div>
          <div><dt>{t("diagnosticsStorageSize")}</dt><dd>{formatBytes(storage?.database_size_bytes)}</dd></div>
          <div><dt>{t("diagnosticsReclaimable")}</dt><dd>{formatBytes(storage?.reclaimable_bytes)}</dd></div>
          <div><dt>{t("diagnosticsAvailableDisk")}</dt><dd>{formatBytes(storage?.available_disk_bytes)}</dd></div>
        </dl>
      </article>

      <article className="bloomery-diagnostics-card">
        <div className="bloomery-diagnostics-card-heading">
          <span className="bloomery-diagnostics-card-icon"><SearchCheck size={17} aria-hidden="true" /></span>
          <div><p className="bloomery-eyebrow">RAG INDEX</p><h2>{t("diagnosticsIndex")}</h2></div>
          <span className={`bloomery-diagnostics-status ${indexHealthy ? "is-healthy" : "is-warning"}`}>
            {indexHealthy ? t("diagnosticsIndexHealthy") : t(indexError ? "diagnosticsIndexUnavailable" : "diagnosticsIndexAttention")}
          </span>
        </div>
        <dl className="bloomery-diagnostics-details">
          <div><dt>{t("diagnosticsServingMode")}</dt><dd>{index?.serving_mode ?? (index ? t("diagnosticsUnknown") : t("diagnosticsIndexUnconfigured"))}</dd></div>
          <div><dt>{t("diagnosticsChunkCount")}</dt><dd>{index?.chunk_count ?? "--"}</dd></div>
          <div><dt>{t("diagnosticsRebuildSpace")}</dt><dd>{formatBytes(index?.required_rebuild_bytes)}</dd></div>
          <div><dt>{t("diagnosticsStaleTemporary")}</dt><dd>{index?.stale_temporary_count ?? "--"}</dd></div>
        </dl>
      </article>

      <article className="bloomery-diagnostics-card">
        <div className="bloomery-diagnostics-card-heading">
          <span className="bloomery-diagnostics-card-icon"><PackageCheck size={17} aria-hidden="true" /></span>
          <div><p className="bloomery-eyebrow">STEEL DOMAIN</p><h2>{t("diagnosticsSteelPackage")}</h2></div>
          <span className={`bloomery-diagnostics-status ${steelPackageHealthy ? "is-healthy" : "is-warning"}`}>
            {t(steelPackageStatusKey)}
          </span>
        </div>
        <dl className="bloomery-diagnostics-details">
          <div><dt>{t("diagnosticsSteelPackageBundled")}</dt><dd>{steelPackageHealthy ? t("diagnosticsSteelPackageHealthy") : "--"}</dd></div>
          {steelPackage.error && <div><dt>{t("diagnosticsSteelPackageError")}</dt><dd>{steelPackage.error}</dd></div>}
        </dl>
        {!steelPackageHealthy && <button type="button" className="bloomery-action-secondary" onClick={onRetrySteelPackage} disabled={busySteelPackage}>
          <RotateCcw size={15} aria-hidden="true" />{busySteelPackage ? t("loading") : t("diagnosticsRetrySteelPackage")}
        </button>}
      </article>
    </div>
  );
}
