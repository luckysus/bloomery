import { CircleHelp, LoaderCircle } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import SettingsProviderCard from "./SettingsProviderCard";
import type { ProviderSlot, RetrievalPlan, SettingsEditor } from "./settingsModel";

export default function SettingsProvidersPanel({
  plan,
  loading,
  editors,
  busySlot,
  testingSlot,
  onChange,
  onSubmit,
  onTest,
  onDelete,
  onPlanChange,
}: {
  plan: RetrievalPlan;
  loading: boolean;
  editors: SettingsEditor[];
  busySlot: ProviderSlot | null;
  testingSlot: ProviderSlot | null;
  onChange: (editor: SettingsEditor) => void;
  onSubmit: (event: React.FormEvent<HTMLFormElement>, editor: SettingsEditor) => void;
  onTest: (editor: SettingsEditor) => void;
  onDelete: (editor: SettingsEditor) => void;
  onPlanChange: (plan: RetrievalPlan) => void;
}) {
  const { t } = useLocale();
  return (
    <>
      <section className="bloomery-settings-plan" aria-labelledby="settings-plan-heading">
        <div><h2 id="settings-plan-heading">{t("settingsPlanTitle")}</h2></div>
        <fieldset className="bloomery-settings-plan-options">
          <legend>{t("settingsPlanLabel")}</legend>
          <label><input type="radio" name="siliconflow-plan" checked={plan === "free"} onChange={() => onPlanChange("free")} aria-label={t("settingsPlanFree")} />{t("freePlan")}</label>
          <label><input type="radio" name="siliconflow-plan" checked={plan === "pro"} onChange={() => onPlanChange("pro")} aria-label={t("settingsPlanPro")} />{t("proPlan")}</label>
        </fieldset>
      </section>

      {loading ? <div className="bloomery-settings-loading"><LoaderCircle size={18} className="bloomery-spin" />{t("loading")}</div> : (
        <div className="bloomery-settings-grid">
          {editors.map((editor) => (
            <SettingsProviderCard
              key={editor.slot}
              editor={editor}
              busy={busySlot === editor.slot}
              testing={testingSlot === editor.slot}
              onChange={onChange}
              onSubmit={onSubmit}
              onTest={onTest}
              onDelete={onDelete}
            />
          ))}
        </div>
      )}

      <aside className="bloomery-settings-note"><CircleHelp size={17} aria-hidden="true" /><span>{t("settingsProviderNote")}</span></aside>
    </>
  );
}
