import { useEffect, useRef, useState } from "react";
import { Check, CircleUserRound, ExternalLink, HardDrive, LoaderCircle, RotateCcw, Save, Trash2 } from "lucide-react";
import { desktop, isDesktopRuntime, type StoragePaths } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import { getSettingValue, setSettingValue } from "./settingsModel";
import { settingsErrorMessage } from "./settingsError";
import { Input } from "../../components/ui/input";

type AccountSettings = { display_name: string; workspace_name: string };
const defaults: AccountSettings = { display_name: "Suna", workspace_name: "本地工作区" };
function parseAccount(raw: string | null): AccountSettings {
  if (!raw) return defaults;
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    return { display_name: typeof value.display_name === "string" && value.display_name.trim() ? value.display_name : defaults.display_name, workspace_name: typeof value.workspace_name === "string" && value.workspace_name.trim() ? value.workspace_name : defaults.workspace_name };
  } catch { return defaults; }
}
export default function SettingsAccountPanel({ onAccountNameSaved }: { onAccountNameSaved?: (name: string) => void }) {
  const { t } = useLocale();
  const [value, setValue] = useState<AccountSettings>(defaults);
  const [paths, setPaths] = useState<StoragePaths | null>(null);
  const [state, setState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const [loadNonce, setLoadNonce] = useState(0);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const pending = useRef(defaults);
  const timer = useRef<number | null>(null);
  const mountedRef = useRef(true);
  useEffect(() => {
    let active = true;
    mountedRef.current = true;
    const load = async () => {
      setError(null);
      setLoadError(false);
      setLoaded(false);
      try {
        const raw = await getSettingValue("profile.account");
        if (!active) return;
        const next = parseAccount(raw);
        pending.current = next;
        setValue(next);
        setLoaded(true);
        setLoadError(false);
      } catch (cause) {
        if (active) {
          setLoaded(false);
          setLoadError(true);
          setState("error");
          setError(settingsErrorMessage(cause, "无法读取账户设置，请重试"));
        }
      }
    };
    void load();
    if (isDesktopRuntime() && typeof desktop.getStoragePaths === "function") {
      void desktop.getStoragePaths().then((nextPaths) => { if (active) setPaths(nextPaths); }).catch((cause) => {
        if (active) setError(settingsErrorMessage(cause, "无法读取本地目录，请重试"));
      });
    }
    const changed = (event: Event) => {
      const detail = (event as CustomEvent<{ key?: string; value?: string }>).detail;
      if (detail?.key !== "profile.account" || !detail.value) return;
      const next = parseAccount(detail.value);
      pending.current = next;
      setValue(next);
      setLoaded(true);
      setState("saved");
      setLoadError(false);
      setError(null);
    };
    const reset = () => {
      pending.current = defaults;
      setValue(defaults);
      setLoaded(true);
      setState("saved");
      setLoadError(false);
      setError(null);
    };
    window.addEventListener("suna:setting-changed", changed);
    window.addEventListener("suna:settings-reset", reset);
    return () => {
      active = false;
      mountedRef.current = false;
      window.removeEventListener("suna:setting-changed", changed);
      window.removeEventListener("suna:settings-reset", reset);
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [loadNonce]);
  const savePending = async () => {
    setState("saving");
    setError(null);
    try {
      const next = { version: 1, display_name: pending.current.display_name.trim() || defaults.display_name, workspace_name: pending.current.workspace_name.trim() || defaults.workspace_name };
      pending.current = next;
      await setSettingValue("profile.account", JSON.stringify(next));
      if (!mountedRef.current) return;
      setValue(next);
      onAccountNameSaved?.(next.display_name);
      setState("saved");
    } catch (cause) {
      if (!mountedRef.current) return;
      setState("error");
      setError(settingsErrorMessage(cause, "账户设置保存失败，请重试"));
    }
  };
  const save = async () => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    await savePending();
  };
  const update = (next: AccountSettings) => {
    const normalized = { display_name: next.display_name.slice(0, 80), workspace_name: next.workspace_name.slice(0, 80) };
    pending.current = normalized;
    setValue(normalized);
    setState("idle");
    setError(null);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => { timer.current = null; void savePending(); }, 650);
  };
  const openDataDirectory = async () => { setError(null); if (!isDesktopRuntime()) { setError("打开应用数据目录仅在桌面客户端中可用"); return; } try { await desktop.openStoragePath("app_data"); } catch (cause) { setError(settingsErrorMessage(cause, "无法打开数据目录，请重试")); } };
  const exportProfile = () => {
    const payload = { version: 1, exported_at: new Date().toISOString(), account: value };
    const url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" }));
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "suna-account.json";
    anchor.click();
    URL.revokeObjectURL(url);
  };
  const clearLocalProfile = async () => {
    if (!window.confirm("只清除本地账户资料（显示名称和工作区名称），不会删除对话、知识库或原始文件。继续吗？")) return;
    setState("saving"); setError(null);
    try {
      if (timer.current !== null) window.clearTimeout(timer.current);
      pending.current = defaults;
      await setSettingValue("profile.account", JSON.stringify({ version: 1, ...defaults }));
      setValue(defaults);
      onAccountNameSaved?.(defaults.display_name);
      setState("saved");
    } catch (cause) { setState("error"); setError(settingsErrorMessage(cause, "清除本地账户资料失败，请重试")); }
  };
  const initials = value.display_name.trim().slice(0, 2).toUpperCase() || "SU";
  return <section className="suna-settings-form-panel" aria-labelledby="settings-account-heading" aria-busy={!loaded || state === "saving"}>
    <header className="suna-settings-form-heading"><div><span className="suna-settings-kicker">LOCAL ACCOUNT</span><h2 id="settings-account-heading">{t("settingsCategoryAccount")}</h2><p>{t("settingsAccountCopy")}</p></div><CircleUserRound size={22} aria-hidden="true" /></header>
    {error && <p className="suna-settings-inline-error" role="alert">{error}{loadError && <button type="button" className="suna-settings-inline-retry" onClick={() => setLoadNonce((nonce) => nonce + 1)}>重试</button>}</p>}
    {!loaded ? <p className="suna-settings-loading" role="status" aria-live="polite"><LoaderCircle size={16} className="suna-spin" />正在加载账户设置...</p> : <>
      <div className="suna-account-profile-card"><span className="suna-account-avatar suna-account-avatar-large">{initials}</span><div><strong>{value.display_name || defaults.display_name}</strong><span>本地单人账户 · 数据保存在本机</span></div><span className="suna-settings-status-badge"><Check size={13} />已启用</span></div>
      <div className="suna-settings-form-grid"><label className="suna-settings-field"><span>{t("settingsAccountName")}</span><Input value={value.display_name} maxLength={80} onChange={(event) => update({ ...value, display_name: event.target.value })} /></label><label className="suna-settings-field"><span>工作区名称</span><Input value={value.workspace_name} maxLength={80} onChange={(event) => update({ ...value, workspace_name: event.target.value })} /></label></div>
      <div className="suna-settings-info-grid"><div><HardDrive size={16} /><span>账户模式</span><strong>本地单人</strong></div><div><HardDrive size={16} /><span>应用数据</span><code title={paths?.app_data}>{paths?.app_data ?? "桌面端启动后显示"}</code><button type="button" className="suna-icon-button" onClick={() => void openDataDirectory()} aria-label="打开应用数据目录" title="打开应用数据目录"><ExternalLink size={14} /></button></div></div>
      <footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={() => update(defaults)}><RotateCcw size={15} />恢复本页默认</button><button type="button" className="suna-secondary-button" onClick={exportProfile}><ExternalLink size={15} />导出账户资料</button><button type="button" className="suna-secondary-button is-danger" onClick={() => void clearLocalProfile()} disabled={state === "saving"}><Trash2 size={15} />清除本地资料</button><button type="button" className="suna-primary-button" onClick={() => void save()} disabled={state === "saving"}>{state === "saving" ? <LoaderCircle size={15} className="suna-spin" /> : <Save size={15} />}{state === "saving" ? "保存中" : t("settingsSave")}</button><span className={`suna-settings-inline-saved is-${state}`} role="status">{state === "saving" && <LoaderCircle size={14} className="suna-spin" />}{state === "saved" && <Check size={14} />}{state === "error" ? "保存失败，修改后重试" : state === "saving" ? "保存中" : state === "saved" ? t("settingsSaved") : "自动保存"}</span></footer>
    </>}
  </section>;
}
