import { useEffect, useState } from "react";
import { Check, Keyboard, RotateCcw, Save } from "lucide-react";
import { desktop } from "../../bridge/desktop";
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
  const { t } = useLocale(); const [value, setValue] = useState<ShortcutSettings>(defaults); const [saved, setSaved] = useState(false); const [error, setError] = useState<string | null>(null);
  useEffect(() => { void desktop.getSetting("ui.shortcuts").then((raw) => setValue(normalize(raw))).catch((cause) => setError(cause instanceof Error ? cause.message : "无法读取快捷键")); }, []);
  const update = (key: ShortcutKey, next: string) => { setValue((current) => ({ ...current, [key]: next })); setSaved(false); };
  const save = async () => { setError(null); try { await desktop.setSetting("ui.shortcuts", JSON.stringify({ version: 1, ...value })); setSaved(true); } catch (cause) { setError(cause instanceof Error ? cause.message : "快捷键保存失败"); } };
  return <section className="suna-settings-form-panel" aria-labelledby="settings-shortcuts-heading"><header className="suna-settings-form-heading"><div><span className="suna-settings-kicker">KEYBOARD</span><h2 id="settings-shortcuts-heading">{t("settingsCategoryShortcuts")}</h2><p>{t("settingsShortcutsCopy")}</p></div><Keyboard size={22} aria-hidden="true" /></header>{error && <p className="suna-settings-inline-error" role="alert">{error}</p>}<div className="suna-shortcut-list">{rows.map((row) => <label className="suna-shortcut-row" key={row.key}><span><strong>{row.label}</strong><small>{row.copy}</small></span><select value={value[row.key]} aria-label={row.label} onChange={(event) => update(row.key, event.target.value)}>{choices.map((choice) => <option key={choice} value={choice}>{choice}</option>)}</select></label>)}</div><footer className="suna-settings-form-actions"><button type="button" className="suna-secondary-button" onClick={() => { setValue(defaults); setSaved(false); }}><RotateCcw size={15} />恢复默认快捷键</button><button type="button" className="suna-primary-button" onClick={() => void save()}><Save size={15} />保存快捷键</button>{saved && <span className="suna-settings-inline-saved"><Check size={14} />{t("settingsSaved")}</span>}</footer></section>;
}


