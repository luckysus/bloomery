import { Activity, AlertCircle, Check, Settings2 } from "lucide-react";
import LanguageSelect from "../../components/common/LanguageSelect";
import type { MessageKey } from "../../i18n/locale";

interface Props {
  t: (key: MessageKey, values?: Record<string, string | number>) => string;
  loading: boolean;
  error: string | null;
  notice: string | null;
  query: string;
  setQuery: (value: string) => void;
  load: () => void;
  exportSettings: () => void;
  importSettings: (file: File) => void;
  fileInputKey: number;
  resetPreferences: () => void;
  onOpenDiagnostics?: () => void;
}

export default function SettingsPageChrome({ t, loading, error, notice, query, setQuery, load, exportSettings, importSettings, fileInputKey, resetPreferences, onOpenDiagnostics }: Props) {
  return <>
    <header className="suna-settings-header"><div><span className="suna-settings-kicker">SUNA CONTROL CENTER</span><h1 id="settings-heading">{t("settingsTitle")}</h1><p>{t("settingsCenterLede")}</p></div><div className="suna-settings-header-actions"><LanguageSelect />{onOpenDiagnostics && <button type="button" className="suna-action-secondary suna-settings-diagnostics-button" onClick={onOpenDiagnostics}><Activity size={16} aria-hidden="true" />{t("settingsDiagnostics")}</button>}<button type="button" className="suna-icon-button" onClick={load} disabled={loading} aria-label={t("settingsRefresh")} title={t("settingsRefresh")}><Settings2 size={18} aria-hidden="true" /></button></div></header>
    {error && <div className="suna-settings-alert" role="alert"><AlertCircle size={17} aria-hidden="true" /><span>{error}</span></div>}
    {notice && <div className="suna-settings-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}
    <div className="suna-settings-toolbar"><input type="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("settingsSearchPlaceholder")} aria-label={t("settingsSearch")} /><button type="button" className="suna-action-secondary" onClick={exportSettings}>{t("settingsExport")}</button><label className="suna-action-secondary">{t("settingsImport")}<input key={fileInputKey} type="file" accept="application/json" hidden onChange={(event) => { const file = event.target.files?.[0]; if (file) importSettings(file); }} /></label><button type="button" className="suna-action-secondary" onClick={resetPreferences}>{t("settingsReset")}</button></div>
  </>;
}
