import { useEffect, useRef, useState } from "react";
import { Check, Keyboard, RotateCcw, Save } from "lucide-react";
import { getSettingValue, setSettingValue } from "./settingsModel";
import { settingsErrorMessage } from "./settingsError";
import { useLocale } from "../../i18n/locale";

type ShortcutKey = "newConversation" | "focusInput" | "send" | "stopAgent" | "openCommand" | "openSettings" | "searchHistory" | "toggleSidebar" | "toggleInspector" | "recoverTask";
type ShortcutSettings = Record<ShortcutKey, string>;
const defaults: ShortcutSettings = { newConversation: "Ctrl+N", focusInput: "Ctrl+L", send: "Ctrl+Enter", stopAgent: "Esc", openCommand: "Ctrl+K", openSettings: "Ctrl+,", searchHistory: "Ctrl+Shift+F", toggleSidebar: "Ctrl+\\", toggleInspector: "Ctrl+J", recoverTask: "Ctrl+Shift+R" };
const rows: Array<{ key: ShortcutKey; label: string; copy: string }> = [
  { key: "newConversation", label: "新建对话", copy: "创建一个新的 Agent 会话" }, { key: "focusInput", label: "聚焦输入框", copy: "快速回到消息编辑区" }, { key: "send", label: "发送消息", copy: "提交当前消息或草稿" }, { key: "stopAgent", label: "停止 Agent", copy: "中断正在运行的任务" }, { key: "openCommand", label: "打开命令面板", copy: "搜索页面、对话和设置" }, { key: "openSettings", label: "打开设置", copy: "直接进入设置中心" }, { key: "searchHistory", label: "搜索对话", copy: "在历史会话中查找内容" }, { key: "toggleSidebar", label: "切换侧栏", copy: "显示或隐藏主导航" }, { key: "toggleInspector", label: "切换运行检查器", copy: "显示或隐藏 Agent 运行面板" }, { key: "recoverTask", label: "恢复最近任务", copy: "继续上次中断的 Agent 运行" },
];
const choices = ["Ctrl+N", "Ctrl+L", "Ctrl+Enter", "Enter", "Esc", "Ctrl+K", "Ctrl+,", "Ctrl+Shift+F", "Ctrl+\\", "Ctrl+J", "Ctrl+Shift+R", "未设置"];
function normalize(raw: string | null): ShortcutSettings { if (!raw) return defaults; try { const value = JSON.parse(raw) as Record<string, unknown>; return Object.fromEntries(rows.map(({ key }) => [key, typeof value[key] === "string" ? value[key] : defaults[key]])) as ShortcutSettings; } catch { return defaults; } }
export default function SettingsShortcutsPanel() {
  const { t } = useLocale(); const [value, setValue] = useState<ShortcutSettings>(defaults); const [loaded, setLoaded] = useState(false); const [loadNonce, setLoadNonce] = useState(0); const [loadError, setLoadError] = useState(false); const [saved, setSaved] = useState(false); const [saving, setSaving] = useState(false); const [error, setError] = useState<string | null>(null); const pending = useRef(defaults); const timer = useRef<number | null>(null); const request = useRef(0);
  useEffect(() => {
    let active = true;
    const load = async () => {
      setLoaded(false);
      setLoadError(false);
      setError(null);
      try {
        const raw = await getSettingValue("ui.shortcuts");
        if (!active) return;
        const next = normalize(raw); pending.current = next; setValue(next); setSaved(false); setLoadError(false);
      } catch (cause) {
        if (active) { setLoadError(true); setError(settingsErrorMessage(cause, "无法读取快捷键，请重试")); }
      } finally {
        if (active) setLoaded(true);
      }
    };
    void load();
    const changed = (event: Event) => {
      const detail = (event as CustomEvent<{ key?: string; value?: string }>).detail;
      if (detail?.key !== "ui.shortcuts" || !detail.value) return;
      const next = normalize(detail.value); pending.current = next; setValue(next); setSaved(true); setLoadError(false); setError(null);
    };
    const reset = () => { pending.current = defaults; setValue(defaults); setSaved(true); setLoadError(false); setError(null); };
    window.addEventListener("suna:setting-changed", changed);
    window.addEventListener("suna:settings-reset", reset);
    return () => { active = false; window.removeEventListener("suna:setting-changed", changed); window.removeEventListener("suna:settings-reset", reset); if (timer.current !== null) window.clearTimeout(timer.current); };
  }, [loadNonce]);
  const duplicateFor = (next: ShortcutSettings) => Object.values(next).filter((shortcut) => shortcut !== "未设置").some((shortcut, index, values) => values.indexOf(shortcut) !== index);
  const persist = async (next: ShortcutSettings) => {
    if (duplicateFor(next)) { setError("快捷键不能重复，请修改冲突的操作"); setSaved(false); return; }
    const currentRequest = ++request.current;
    setSaving(true); setError(null); setSaved(false);
    try { await setSettingValue("ui.shortcuts", JSON.stringify({ version: 1, ...next })); if (currentRequest === request.current) { setLoadError(false); setSaved(true); } }
    catch (cause) { if (currentRequest === request.current) { setLoadError(false); setError(settingsErrorMessage(cause, "快捷键保存失败，请重试")); } }
    finally { if (currentRequest === request.current) setSaving(false); }
  };
  const schedulePersist = (next: ShortcutSettings) => { if (timer.current !== null) window.clearTimeout(timer.current); timer.current = window.setTimeout(() => { timer.current = null; void persist(next); }, 650); };
  const update = (key: ShortcutKey, nextShortcut: string) => { const next = { ...pending.current, [key]: nextShortcut }; pending.current = next; setValue(next); setSaved(false); setError(null); schedulePersist(next); };
  const duplicate = duplicateFor(value);
  const save = async () => { if (timer.current !== null) { window.clearTimeout(timer.current); timer.current = null; } await persist(pending.current); };
  const reset = () => { if (timer.current !== null) { window.clearTimeout(timer.current); timer.current = null; } pending.current = defaults; setValue(defaults); setSaved(false); setError(null); schedulePersist(defaults); };
  const retryLoad = () => { setLoadNonce((nonce) => nonce + 1); };
  return <section className="suna-settings-form-panel" aria-labelledby="settings-shortcuts-heading" aria-busy={!loaded || saving}><header className="suna-settings-form-heading"><div><span className="suna-settings-kicker">KEYBOARD</span><h2 id="settings-shortcuts-heading">{t("settingsCategoryShortcuts")}</h2><p>{t("settingsShortcutsCopy")}</p></div><Keyboard size={22} aria-hidden="true" /></header>{!loaded && <p className="suna-settings-loading" role="status">正在加载快捷键...</p>}{error && !duplicate && <p className="suna-settings-inline-error" role="alert">{error}{loadError && <button type="button" className="suna-settings-inline-retry" onClick={retryLoad}>重试</button>}</p>}{duplicate && <p className="suna-settings-inline-error" role="alert">快捷键不能重复，请修改冲突的操作。</p>}<div className="suna-shortcut-list">{rows.map((row) => <label className="suna-shortcut-row" key={row.key}><span><strong>{row.label}</strong><small>{row.copy}</small></span><select value={value[row.key]} aria-label={row.label} onChange={(event) => update(row.key, event.target.value)}>{choices.map((choice) => <option key={choice} value={choice}>{choice}</option>)}</select></label>)}</div><footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={reset}><RotateCcw size={15} />恢复默认快捷键</button><button type="button" className="suna-primary-button" onClick={() => void save()} disabled={duplicate || saving}><Save size={15} />{saving ? "保存中" : "立即保存"}</button><span className={`suna-settings-inline-saved is-${saving ? "saving" : saved ? "saved" : "idle"}`} role="status">{saving && <span>保存中</span>}{saved && <><Check size={14} />{t("settingsSaved")}</>}{!saving && !saved && !duplicate && <span>自动保存</span>}</span></footer></section>;
}


