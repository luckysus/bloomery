import { Check, LoaderCircle, Monitor, Moon, Sun } from "lucide-react";
import { useLocale, type LanguagePreference } from "../../i18n/locale";
import { useTheme, type ThemePreference } from "../../theme/theme";
import { useAppearanceSettings, type AppearancePreferences, type DensityPreference, type FontSizePreference, type SidebarWidthPreference, type StartupPagePreference } from "../../settings/appearance";

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

function ToggleRow({ checked, label, description, onChange }: { checked: boolean; label: string; description: string; onChange: (checked: boolean) => void }) {
  return <label className="suna-settings-toggle-row"><span><strong>{label}</strong><small>{description}</small></span><input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} /><i aria-hidden="true" /></label>;
}

function SegmentedControl<T extends string>({ value, options, ariaLabel, onChange }: { value: T; options: Array<{ value: T; label: string }>; ariaLabel: string; onChange: (value: T) => void }) {
  return <div className="suna-settings-segmented" role="group" aria-label={ariaLabel}>{options.map((option) => <button key={option.value} type="button" className={value === option.value ? "is-selected" : ""} aria-pressed={value === option.value} onClick={() => onChange(option.value)}>{option.label}</button>)}</div>;
}

function AppearanceSettings() {
  const { t, preference: language, setPreference: setLanguage } = useLocale();
  const { preference: theme, setPreference: setTheme } = useTheme();
  const { preferences, updatePreferences, saveState, retrySave } = useAppearanceSettings();
  const update = (next: Partial<AppearancePreferences>) => updatePreferences(next);
  const sidebarOptions: Array<{ value: SidebarWidthPreference; label: string }> = [
    { value: "narrow", label: t("settingsSidebarNarrow") },
    { value: "standard", label: t("settingsSidebarStandard") },
    { value: "wide", label: t("settingsSidebarWide") },
  ];

  return <section className="suna-appearance-panel" aria-labelledby="appearance-heading">
    <header className="suna-appearance-heading"><div><h2 id="appearance-heading">{t("settingsAppearanceHeading")}</h2><p>{t("settingsAppearanceLede")}</p></div><SaveState state={saveState} onRetry={retrySave} /></header>
    <div className="suna-appearance-section"><h3>{t("settingsAppearanceMode")}</h3><div className="suna-theme-preview-grid" role="group" aria-label={t("themeTitle")}>{themeOptions.map(({ value, labelKey, descriptionKey, Icon }) => <button type="button" key={value} className={`suna-theme-preview-card ${theme === value ? "is-selected" : ""}`} aria-label={t(labelKey)} aria-pressed={theme === value} onClick={() => setTheme(value)}><span className={`suna-theme-preview ${value}`}><span /><span /><span /></span><span className="suna-theme-preview-copy"><strong>{t(labelKey)}</strong><small>{t(descriptionKey)}</small></span>{theme === value && <span className="suna-theme-check"><Check size={13} /></span>}<Icon className="suna-theme-preview-icon" size={15} aria-hidden="true" /></button>)}</div></div>
    <div className="suna-appearance-section suna-appearance-controls"><label className="suna-settings-language-row"><span><strong>{t("languageLabel")}</strong><small>{t("settingsLanguageCopy")}</small></span><select aria-label={t("languageLabel")} value={language} onChange={(event) => setLanguage(event.target.value as LanguagePreference)}><option value="zh-CN">简体中文</option><option value="en-US">English</option><option value="system">{t("languageSystem")}</option></select></label><div className="suna-settings-control-row"><span><strong>{t("settingsFontSize")}</strong><small>{t("settingsFontSizeCopy")}</small></span><SegmentedControl value={preferences.fontSize} options={fontOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))} ariaLabel={t("settingsFontSize")} onChange={(fontSize) => update({ fontSize })} /></div><div className="suna-settings-control-row"><span><strong>{t("settingsDensity")}</strong><small>{t("settingsDensityCopy")}</small></span><SegmentedControl value={preferences.density} options={densityOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))} ariaLabel={t("settingsDensity")} onChange={(density) => update({ density })} /></div></div>
    <div className="suna-appearance-section suna-settings-toggle-list"><ToggleRow checked={preferences.enableAnimations} label={t("settingsAnimations")} description={t("settingsAnimationsCopy")} onChange={(enableAnimations) => update({ enableAnimations })} /><ToggleRow checked={preferences.showAgentPanel} label={t("settingsAgentPanel")} description={t("settingsAgentPanelCopy")} onChange={(showAgentPanel) => update({ showAgentPanel })} /></div>
    <details className="suna-settings-advanced"><summary>{t("settingsAdvancedLayout")}</summary><div className="suna-settings-control-row"><span><strong>{t("settingsSidebarWidth")}</strong><small>{t("settingsSidebarWidthCopy")}</small></span><SegmentedControl value={preferences.sidebarWidth} options={sidebarOptions} ariaLabel={t("settingsSidebarWidth")} onChange={(sidebarWidth) => update({ sidebarWidth })} /></div></details>
  </section>;
}

function GeneralSettings() {
  const { t } = useLocale();
  const { preferences, updatePreferences, saveState, retrySave } = useAppearanceSettings();
  const toggle = (key: keyof Pick<AppearancePreferences, "restoreSession" | "saveDrafts" | "autoUpdate" | "notifications" | "showToolDetails" | "confirmDangerous">, labelKey: "settingsRestoreSession" | "settingsSaveDrafts" | "settingsAutoUpdate" | "settingsNotifications" | "settingsShowToolDetails" | "settingsConfirmDangerous", copyKey: "settingsRestoreSessionCopy" | "settingsSaveDraftsCopy" | "settingsAutoUpdateCopy" | "settingsNotificationsCopy" | "settingsShowToolDetailsCopy" | "settingsConfirmDangerousCopy") => <ToggleRow checked={preferences[key]} label={t(labelKey)} description={t(copyKey)} onChange={(checked) => updatePreferences({ [key]: checked })} />;
  return <section className="suna-settings-preferences"><header className="suna-appearance-heading"><div><h2>{t("settingsTabGeneral")}</h2><p>{t("settingsGeneralCopy")}</p></div><SaveState state={saveState} onRetry={retrySave} /></header><div className="suna-settings-general-selects"><label className="suna-settings-language-row"><span><strong>{t("settingsDefaultPage")}</strong><small>{t("settingsDefaultPageCopy")}</small></span><select value={preferences.startupPage} onChange={(event) => updatePreferences({ startupPage: event.target.value as StartupPagePreference })}><option value="chat">{t("settingsDefaultPageChat")}</option><option value="knowledge">{t("settingsDefaultPageKnowledge")}</option><option value="literature">{t("settingsDefaultPageLiterature")}</option><option value="data">{t("settingsDefaultPageData")}</option></select></label></div><div className="suna-settings-toggle-list">{toggle("restoreSession", "settingsRestoreSession", "settingsRestoreSessionCopy")}{toggle("saveDrafts", "settingsSaveDrafts", "settingsSaveDraftsCopy")}{toggle("autoUpdate", "settingsAutoUpdate", "settingsAutoUpdateCopy")}{toggle("notifications", "settingsNotifications", "settingsNotificationsCopy")}{toggle("showToolDetails", "settingsShowToolDetails", "settingsShowToolDetailsCopy")}{toggle("confirmDangerous", "settingsConfirmDangerous", "settingsConfirmDangerousCopy")}</div></section>;
}

export default function SettingsPreferencesPanel({ mode }: { mode: "general" | "appearance" }) {
  return mode === "appearance" ? <AppearanceSettings /> : <GeneralSettings />;
}