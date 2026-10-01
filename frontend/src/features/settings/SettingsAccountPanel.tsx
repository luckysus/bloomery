import { useEffect, useState } from "react";
import { Check, CircleUserRound, HardDrive, LoaderCircle, RotateCcw, Save } from "lucide-react";
import { desktop, type StoragePaths } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

type AccountSettings = { display_name: string; workspace_name: string };
const defaults: AccountSettings = { display_name: "Suna", workspace_name: "本地工作区" };
function parseAccount(raw: string | null): AccountSettings {
  if (!raw) return defaults;
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    return { display_name: typeof value.display_name === "string" && value.display_name.trim() ? value.display_name : defaults.display_name, workspace_name: typeof value.workspace_name === "string" && value.workspace_name.trim() ? value.workspace_name : defaults.workspace_name };
  } catch { return defaults; }
}
export default function SettingsAccountPanel() {
  const { t } = useLocale();
  const [value, setValue] = useState<AccountSettings>(defaults);
  const [paths, setPaths] = useState<StoragePaths | null>(null);
  const [state, setState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { let active = true; void Promise.all([desktop.getSetting("profile.account"), typeof desktop.getStoragePaths === "function" ? desktop.getStoragePaths() : Promise.resolve(null)]).then(([raw, nextPaths]) => { if (!active) return; setValue(parseAccount(raw)); setPaths(nextPaths); }).catch((cause) => { if (active) setError(cause instanceof Error ? cause.message : "无法读取账户设置"); }); return () => { active = false; }; }, []);
  const save = async () => { setState("saving"); setError(null); try { const next = { version: 1, display_name: value.display_name.trim() || defaults.display_name, workspace_name: value.workspace_name.trim() || defaults.workspace_name }; await desktop.setSetting("profile.account", JSON.stringify(next)); setValue(next); setState("saved"); } catch (cause) { setState("error"); setError(cause instanceof Error ? cause.message : "账户设置保存失败"); } };
  const initials = value.display_name.trim().slice(0, 2).toUpperCase() || "SU";
  return <section className="suna-settings-form-panel" aria-labelledby="settings-account-heading"><header className="suna-settings-form-heading"><div><span className="suna-settings-kicker">LOCAL ACCOUNT</span><h2 id="settings-account-heading">{t("settingsCategoryAccount")}</h2><p>{t("settingsAccountCopy")}</p></div><CircleUserRound size={22} aria-hidden="true" /></header>{error && <p className="suna-settings-inline-error" role="alert">{error}</p>}<div className="suna-account-profile-card"><span className="suna-account-avatar suna-account-avatar-large">{initials}</span><div><strong>{value.display_name || defaults.display_name}</strong><span>本地单人账户 · 数据保存在本机</span></div><span className="suna-settings-status-badge"><Check size={13} />已启用</span></div><div className="suna-settings-form-grid"><label className="suna-settings-field"><span>{t("settingsAccountName")}</span><input value={value.display_name} maxLength={80} onChange={(event) => setValue({ ...value, display_name: event.target.value })} /></label><label className="suna-settings-field"><span>工作区名称</span><input value={value.workspace_name} maxLength={80} onChange={(event) => setValue({ ...value, workspace_name: event.target.value })} /></label></div><div className="suna-settings-info-grid"><div><HardDrive size={16} /><span>账户模式</span><strong>本地单人</strong></div><div><HardDrive size={16} /><span>应用数据</span><code title={paths?.app_data}>{paths?.app_data ?? "桌面端启动后显示"}</code></div></div><footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={() => setValue(defaults)}><RotateCcw size={15} />恢复本页默认</button><button type="button" className="suna-primary-button" onClick={() => void save()} disabled={state === "saving"}>{state === "saving" ? <LoaderCircle size={15} className="suna-spin" /> : <Save size={15} />}{state === "saving" ? "保存中" : t("settingsSave")}</button>{state === "saved" && <span className="suna-settings-inline-saved"><Check size={14} />{t("settingsSaved")}</span>}</footer></section>;
}
