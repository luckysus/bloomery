import { Check, ExternalLink, LoaderCircle, Monitor, Moon, Sun } from "lucide-react";
import { useEffect, useState } from "react";
import { desktop, isDesktopRuntime, type UpdateCheckResult } from "../../bridge/desktop";
import { useLocale, type LanguagePreference } from "../../i18n/locale";
import { useTheme, type ThemePreference } from "../../theme/theme";
import { useAppearanceSettings, type AppearancePreferences, type DensityPreference, type FontSizePreference, type StartupPagePreference } from "../../settings/appearance";
import { settingsErrorMessage } from "./settingsError";
import { Checkbox } from "../../components/ui/checkbox";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

const themeOptions: Array<{ value: ThemePreference; labelKey: "themeSystem" | "themeLight" | "themeDark"; descriptionKey: "settingsThemeLightCopy" | "settingsThemeDarkCopy" | "settingsThemeSystemCopy"; Icon: typeof Monitor }> = [
  { value: "light", labelKey: "themeLight", descriptionKey: "settingsThemeLightCopy", Icon: Sun },
  { value: "dark", labelKey: "themeDark", descriptionKey: "settingsThemeDarkCopy", Icon: Moon },
  { value: "system", labelKey: "themeSystem", descriptionKey: "settingsThemeSystemCopy", Icon: Monitor },
];

const fontOptions: Array<{ value: FontSizePreference; labelKey: "settingsFontSmall" | "settingsFontMedium" | "settingsFontLarge" }> = [
  { value: "small", labelKey: "settingsFontSmall" },
  { value: "medium", labelKey: "settingsFontMedium" },
  { value: "large", labelKey: "settingsFontLarge" },
];

const densityOptions: Array<{ value: DensityPreference; labelKey: "settingsDensityCompact" | "settingsDensityComfortable" | "settingsDensitySpacious" }> = [
  { value: "compact", labelKey: "settingsDensityCompact" },
  { value: "comfortable", labelKey: "settingsDensityComfortable" },
  { value: "spacious", labelKey: "settingsDensitySpacious" },
];

function SaveState({ state, onRetry }: { state: ReturnType<typeof useAppearanceSettings>["saveState"]; onRetry: () => void }) {
  const { t } = useLocale();
  if (state === "saving") return <span className="suna-settings-save-state is-saving" role="status"><LoaderCircle size={13} className="suna-spin" />{t("settingsSaving")}</span>;
  if (state === "saved") return <span className="suna-settings-save-state is-saved" role="status"><Check size={13} />{t("settingsSavedShort")}</span>;
  if (state === "error") return <button type="button" className="suna-settings-save-state is-error" onClick={onRetry}>{t("settingsSaveFailedRetry")}</button>;
  return null;
}

function LoadState({ error, onRetry }: { error: string | null; onRetry: () => void }) {
  if (!error) return null;
  return <span className="suna-settings-save-state is-error" role="alert">{error}<button type="button" onClick={onRetry}>重试</button></span>;
}

function ToggleRow({ checked, label, description, onChange }: { checked: boolean; label: string; description: string; onChange: (checked: boolean) => void }) {
  return <label className="suna-settings-toggle-row"><span><strong>{label}</strong><small>{description}</small></span><Checkbox aria-label={label} checked={checked} onCheckedChange={(value) => onChange(value === true)} /></label>;
}

function SegmentedControl<T extends string>({ value, options, ariaLabel, onChange }: { value: T; options: Array<{ value: T; label: string }>; ariaLabel: string; onChange: (value: T) => void }) {
  return <div className="suna-settings-segmented" role="group" aria-label={ariaLabel}>{options.map((option) => <button key={option.value} type="button" className={value === option.value ? "is-selected" : ""} aria-pressed={value === option.value} onClick={() => onChange(option.value)}>{option.label}</button>)}</div>;
}

function ThemePreview({ value }: { value: ThemePreference }) {
  return <span className={`suna-theme-preview ${value}`} aria-hidden="true">
    <span className="suna-theme-preview-sidebar"><i className="is-brand" /><i className="is-nav is-active" /><i className="is-nav" /><i className="is-nav" /><i className="is-nav" /></span>
    <span className="suna-theme-preview-main">
      <i className="is-toolbar" />
      <i className="is-title" />
      <span className="is-cards"><i /><i /><i /></span>
      <i className="is-copy" />
      <i className="is-copy is-copy-short" />
    </span>
  </span>;
}

function AppearanceSettings() {
  const { t, preference: language, setPreference: setLanguage, saveState: localeSaveState, loadError: localeLoadError, retryLoad: retryLocaleLoad, retrySave: retryLocaleSave } = useLocale();
  const { preference: theme, setPreference: setTheme, saveState: themeSaveState, loadError: themeLoadError, retryLoad: retryThemeLoad, retrySave: retryThemeSave } = useTheme();
  const { preferences, updatePreferences, saveState, retrySave, loadError, retryLoad } = useAppearanceSettings();
  const update = (next: Partial<AppearancePreferences>) => updatePreferences(next);
  const loadMessage = themeLoadError ?? loadError ?? localeLoadError;
  const retryLoadHandler = themeLoadError ? retryThemeLoad : loadError ? retryLoad : retryLocaleLoad;
  const combinedSaveState = themeSaveState === "error" || saveState === "error" || localeSaveState === "error"
    ? "error"
    : themeSaveState === "saving" || saveState === "saving" || localeSaveState === "saving"
      ? "saving"
      : themeSaveState === "saved" || saveState === "saved" || localeSaveState === "saved"
        ? "saved"
        : "idle";
  const retrySaveHandler = localeSaveState === "error"
    ? retryLocaleSave
    : themeSaveState === "error"
      ? retryThemeSave
      : retrySave;
  return <section className="suna-appearance-panel" aria-labelledby="appearance-heading">
    <header className="suna-appearance-heading"><div><h2 id="appearance-heading">{t("settingsAppearanceHeading")}</h2><p>{t("settingsAppearanceLede")}</p></div><div className="suna-settings-state-stack"><LoadState error={loadMessage} onRetry={retryLoadHandler} /><SaveState state={combinedSaveState} onRetry={retrySaveHandler} /></div></header>
    <div className="suna-appearance-section"><h3>{t("settingsAppearanceMode")}</h3><div className="suna-theme-preview-grid" role="group" aria-label={t("themeTitle")}>{themeOptions.map(({ value, labelKey, descriptionKey, Icon }) => <button type="button" key={value} className={`suna-theme-preview-card ${theme === value ? "is-selected" : ""}`} aria-label={t(labelKey)} aria-pressed={theme === value} onClick={() => setTheme(value)}><ThemePreview value={value} /><span className="suna-theme-preview-copy"><strong>{t(labelKey)}</strong><small>{t(descriptionKey)}</small></span>{theme === value && <span className="suna-theme-check"><Check size={13} /></span>}{value === "system" && <Icon className="suna-theme-preview-icon" size={15} aria-hidden="true" />}</button>)}</div></div>
    <div className="suna-appearance-section suna-appearance-controls"><label className="suna-settings-language-row"><span><strong>{t("languageLabel")}</strong></span><Select value={language} aria-label={t("languageLabel")} onValueChange={(value) => setLanguage(value as LanguagePreference)}>
  <SelectTrigger aria-label={t("languageLabel")}><SelectValue /></SelectTrigger>
  <SelectContent><SelectItem value="zh-CN">简体中文</SelectItem><SelectItem value="en-US">English</SelectItem><SelectItem value="system">{t("languageSystem")}</SelectItem>
  </SelectContent>
</Select></label><div className="suna-settings-control-row"><span><strong>{t("settingsFontSize")}</strong></span><SegmentedControl value={preferences.fontSize} options={fontOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))} ariaLabel={t("settingsFontSize")} onChange={(fontSize) => update({ fontSize })} /></div><div className="suna-settings-control-row"><span><strong>{t("settingsDensity")}</strong></span><SegmentedControl value={preferences.density} options={densityOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))} ariaLabel={t("settingsDensity")} onChange={(density) => update({ density })} /></div></div>
    <div className="suna-appearance-section suna-settings-toggle-list"><ToggleRow checked={preferences.enableAnimations} label={t("settingsAnimations")} description={t("settingsAnimationsCopy")} onChange={(enableAnimations) => update({ enableAnimations })} /><ToggleRow checked={preferences.showAgentPanel} label={t("settingsAgentPanel")} description={t("settingsAgentPanelCopy")} onChange={(showAgentPanel) => update({ showAgentPanel })} /></div>
  </section>;
}

function GeneralSettings() {
  const { t } = useLocale();
  const { preferences, updatePreferences, saveState, retrySave, loadError, retryLoad } = useAppearanceSettings();
  const [updateState, setUpdateState] = useState<"idle" | "checking" | "ready" | "available" | "error">("idle");
  const [update, setUpdate] = useState<UpdateCheckResult | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const toggle = (key: keyof Pick<AppearancePreferences, "restoreSession" | "saveDrafts" | "autoUpdate" | "notifications" | "showToolDetails" | "confirmDangerous">, labelKey: "settingsRestoreSession" | "settingsSaveDrafts" | "settingsAutoUpdate" | "settingsNotifications" | "settingsShowToolDetails" | "settingsConfirmDangerous", copyKey: "settingsRestoreSessionCopy" | "settingsSaveDraftsCopy" | "settingsAutoUpdateCopy" | "settingsNotificationsCopy" | "settingsShowToolDetailsCopy" | "settingsConfirmDangerousCopy") => <ToggleRow checked={preferences[key]} label={t(labelKey)} description={t(copyKey)} onChange={(checked) => updatePreferences({ [key]: checked })} />;
  const checkForUpdates = async () => {
    if (!isDesktopRuntime()) {
      setUpdateError("更新检查仅在桌面客户端中可用");
      setUpdateState("error");
      return;
    }
    setUpdateState("checking");
    setUpdateError(null);
    try {
      const result = await desktop.checkForUpdates();
      setUpdate(result);
      setUpdateState(result.update_available ? "available" : "ready");
    } catch (cause) {
      setUpdateError(settingsErrorMessage(cause, "更新检查失败，请重试"));
      setUpdateState("error");
    }
  };
  useEffect(() => {
    if (preferences.autoUpdate && isDesktopRuntime()) void checkForUpdates();
    // The shell also checks once on normal application startup; this keeps the
    // panel truthful when it is opened later in the same session.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return <section className="suna-settings-preferences"><header className="suna-appearance-heading"><div><h2>{t("settingsTabGeneral")}</h2><p>{t("settingsGeneralCopy")}</p></div><div className="suna-settings-state-stack"><LoadState error={loadError} onRetry={retryLoad} /><SaveState state={saveState} onRetry={retrySave} /></div></header><div className="suna-settings-general-selects"><label className="suna-settings-language-row"><span><strong>{t("settingsDefaultPage")}</strong><small>{t("settingsDefaultPageCopy")}</small></span><Select value={preferences.startupPage} onValueChange={(value) => updatePreferences({ startupPage: value as StartupPagePreference })}>
  <SelectTrigger aria-label="settingsDefaultPage"><SelectValue /></SelectTrigger>
  <SelectContent><SelectItem value="chat">{t("settingsDefaultPageChat")}</SelectItem><SelectItem value="knowledge">{t("settingsDefaultPageKnowledge")}</SelectItem><SelectItem value="literature">{t("settingsDefaultPageLiterature")}</SelectItem><SelectItem value="data">{t("settingsDefaultPageData")}</SelectItem>
  </SelectContent>
</Select></label></div><div className="suna-settings-toggle-list">{toggle("restoreSession", "settingsRestoreSession", "settingsRestoreSessionCopy")}{toggle("saveDrafts", "settingsSaveDrafts", "settingsSaveDraftsCopy")}{toggle("autoUpdate", "settingsAutoUpdate", "settingsAutoUpdateCopy")}{toggle("notifications", "settingsNotifications", "settingsNotificationsCopy")}{toggle("showToolDetails", "settingsShowToolDetails", "settingsShowToolDetailsCopy")}{toggle("confirmDangerous", "settingsConfirmDangerous", "settingsConfirmDangerousCopy")}</div><section className="suna-settings-update" aria-labelledby="settings-update-heading"><div className="suna-settings-update-heading"><div><span className="suna-settings-kicker">UPDATES</span><h3 id="settings-update-heading">软件更新</h3><p>只检查公开稳定版本，下载和安装由你明确决定。</p></div><button type="button" className="suna-secondary-button" onClick={() => void checkForUpdates()} disabled={updateState === "checking"}>{updateState === "checking" ? <LoaderCircle size={15} className="suna-spin" /> : <ExternalLink size={15} />}检查更新</button></div>{updateState === "ready" && <p className="suna-settings-update-message is-success"><Check size={14} />当前已是最新版本（{update?.current_version}）</p>}{updateState === "available" && update?.release_url && <div className="suna-settings-update-available"><p>发现新版本 {update.latest_version}。</p><a className="suna-secondary-button" href={update.release_url} target="_blank" rel="noreferrer">打开发布页<ExternalLink size={14} /></a></div>}{updateState === "error" && <p className="suna-settings-update-message is-error">{updateError || "更新检查失败"}</p>}</section></section>;
}

export default function SettingsPreferencesPanel({ mode }: { mode: "general" | "appearance" }) {
  return mode === "appearance" ? <AppearanceSettings /> : <GeneralSettings />;
}
