import { CircleHelp, LoaderCircle } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import SettingsProviderCard from "./SettingsProviderCard";
import { RadioGroup, RadioGroupItem } from "../../components/ui/radio-group";
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
      <section className="suna-settings-plan" aria-labelledby="settings-plan-heading">
        <div><h2 id="settings-plan-heading">{t("settingsPlanTitle")}</h2></div>
        <fieldset className="suna-settings-plan-options">
          <legend>{t("settingsPlanLabel")}</legend>
          <RadioGroup value={plan} onValueChange={(value) => onPlanChange(value as RetrievalPlan)} aria-label={t("settingsPlanLabel")}>
            <label><RadioGroupItem value="free" aria-label={t("settingsPlanFree")} />{t("freePlan")}</label>
            <label><RadioGroupItem value="pro" aria-label={t("settingsPlanPro")} />{t("proPlan")}</label>
          </RadioGroup>
        </fieldset>
      </section>

      {loading ? <div className="suna-settings-loading"><LoaderCircle size={18} className="suna-spin" />{t("loading")}</div> : (
        <div className="suna-settings-grid">
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

      <aside className="suna-settings-note"><CircleHelp size={17} aria-hidden="true" /><span>{t("settingsProviderNote")}</span></aside>
    </>
  );
}
