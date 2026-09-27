import { useEffect, useState } from "react";
import { desktop } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

const defaults = { maxTurns: 20, contextBudget: 32768, retries: 2, saveCheckpoints: true, allowRecovery: true };
export default function SettingsAgentPanel() {
  const { t } = useLocale();
  const [value, setValue] = useState(defaults);
  const [saved, setSaved] = useState(false);
  useEffect(() => { void desktop.getSetting("agent.preferences").then((raw) => { if (raw) { try { setValue({ ...defaults, ...JSON.parse(raw) }); } catch { /* defaults */ } } }); }, []);
  const update = async (next: Partial<typeof defaults>) => { const merged = { ...value, ...next }; setValue(merged); await desktop.setSetting("agent.preferences", JSON.stringify(merged)); setSaved(true); window.setTimeout(() => setSaved(false), 1200); };
  return <section className="bloomery-settings-preferences"><h2>{t("settingsCategoryAgent")}</h2><p>{t("settingsAgentCopy")}</p><div className="bloomery-settings-agent-grid"><label className="bloomery-settings-field">{t("settingsMaxTurns")}<input type="number" min="1" max="100" value={value.maxTurns} onChange={(event) => void update({ maxTurns: Number(event.target.value) })} /></label><label className="bloomery-settings-field">{t("settingsContextBudget")}<input type="number" min="1024" max="262144" step="1024" value={value.contextBudget} onChange={(event) => void update({ contextBudget: Number(event.target.value) })} /></label><label className="bloomery-settings-field">{t("settingsRetries")}<input type="number" min="0" max="10" value={value.retries} onChange={(event) => void update({ retries: Number(event.target.value) })} /></label></div><label className="bloomery-settings-preference"><span>{t("settingsSaveCheckpoints")}</span><input type="checkbox" checked={value.saveCheckpoints} onChange={(event) => void update({ saveCheckpoints: event.target.checked })} /></label><label className="bloomery-settings-preference"><span>{t("settingsAllowRecovery")}</span><input type="checkbox" checked={value.allowRecovery} onChange={(event) => void update({ allowRecovery: event.target.checked })} /></label>{saved && <span className="bloomery-settings-inline-saved" role="status">{t("settingsSaved")}</span>}</section>;
}
