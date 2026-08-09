import { Download, RefreshCw, Upload } from "lucide-react";
import { useLocale } from "../../i18n/locale";

interface Props {
  loading: boolean;
  busy: boolean;
  onRefresh: () => void;
  onExport: () => void;
  onCreateBackup: () => void;
  onRestoreBackup: () => void;
}

export default function DiagnosticsHeader({
  loading,
  busy,
  onRefresh,
  onExport,
  onCreateBackup,
  onRestoreBackup,
}: Props) {
  const { t } = useLocale();
  return (
    <header className="bloomery-diagnostics-header">
      <div>
        <p className="bloomery-eyebrow">LOCAL RUNTIME / HEALTH</p>
        <h1 id="diagnostics-heading">{t("diagnosticsTitle")}</h1>
        <p className="bloomery-lede">{t("diagnosticsLede")}</p>
      </div>
      <div className="bloomery-diagnostics-actions">
        <button type="button" className="bloomery-icon-button" onClick={onRefresh} disabled={loading} aria-label={t("diagnosticsRefresh")} title={t("diagnosticsRefresh")}>
          <RefreshCw size={18} aria-hidden="true" />
        </button>
        <button type="button" className="bloomery-action-secondary" onClick={onExport} disabled={loading || busy}>
          <Download size={16} aria-hidden="true" />{t("diagnosticsExport")}
        </button>
        <button type="button" className="bloomery-action-secondary" onClick={onCreateBackup} disabled={loading || busy}>
          <Download size={16} aria-hidden="true" />{t("diagnosticsBackupExport")}
        </button>
        <button type="button" className="bloomery-action-secondary" onClick={onRestoreBackup} disabled={loading || busy}>
          <Upload size={16} aria-hidden="true" />{t("diagnosticsBackupRestore")}
        </button>
      </div>
    </header>
  );
}
