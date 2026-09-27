import { useEffect, useState } from "react";
import { desktop } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";
import ThemeSelect from "../../components/common/ThemeSelect";

type Preferences = {
  restoreSession: boolean;
  saveDrafts: boolean;
  showToolDetails: boolean;
  confirmDangerous: boolean;
  density: "comfortable" | "standard" | "compact";
  reduceMotion: boolean;
  inspector: boolean;
};

const defaults: Preferences = {
  restoreSession: true,
  saveDrafts: true,
  showToolDetails: true,
  confirmDangerous: true,
  density: "standard",
  reduceMotion: false,
  inspector: true,
};

export default function SettingsPreferencesPanel({ mode }: { mode: "general" | "appearance" }) {
  const { t } = useLocale();
  const [value, setValue] = useState(defaults);
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    void desktop.getSetting("ui.preferences").then((raw) => {
      if (!raw) return;
      try { setValue({ ...defaults, ...JSON.parse(raw) }); } catch { /* keep defaults */ }
    });
  }, []);
  const update = async (next: Partial<Preferences>) => {
    const merged = { ...value, ...next };
    setValue(merged);
    await desktop.setSetting("ui.preferences", JSON.stringify(merged));
    setSaved(true);
    window.setTimeout(() => setSaved(false), 1200);
  };
  const toggle = (key: keyof Preferences, label: string) => (
    <label className="bloomery-settings-preference"><span>{label}</span><input type="checkbox" checked={Boolean(value[key])} onChange={(event) => void update({ [key]: event.target.checked })} /></label>
  );
  return (
    <section className="bloomery-settings-preferences">
      <h2>{t(mode === "general" ? "settingsTabGeneral" : "settingsCategoryAppearance")}</h2>
      <p>{t(mode === "general" ? "settingsGeneralCopy" : "settingsAppearanceCopy")}</p>
      {mode === "general" ? <div className="bloomery-settings-preference-list">
        {toggle("restoreSession", t("settingsRestoreSession"))}
        {toggle("saveDrafts", t("settingsSaveDrafts"))}
        {toggle("showToolDetails", t("settingsShowToolDetails"))}
        {toggle("confirmDangerous", t("settingsConfirmDangerous"))}
      </div> : <>
        <ThemeSelect />
        <label className="bloomery-settings-field">{t("settingsDensity")}<select value={value.density} onChange={(event) => void update({ density: event.target.value as Preferences["density"] })}><option value="comfortable">{t("settingsDensityComfortable")}</option><option value="standard">{t("settingsDensityStandard")}</option><option value="compact">{t("settingsDensityCompact")}</option></select></label>
        <div className="bloomery-settings-preference-list">{toggle("reduceMotion", t("settingsReduceMotion"))}{toggle("inspector", t("settingsShowInspector"))}</div>
      </>}
      {saved && <span className="bloomery-settings-inline-saved" role="status">{t("settingsSaved")}</span>}
    </section>
  );
}
