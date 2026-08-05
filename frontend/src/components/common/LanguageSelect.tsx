import { Languages } from "lucide-react";
import { useLocale, type LanguagePreference } from "../../i18n/locale";

export default function LanguageSelect() {
  const { preference, setPreference, t } = useLocale();

  return (
    <label className="bloomery-language-control">
      <Languages size={15} aria-hidden="true" />
      <span className="sr-only">{t("languageLabel")}</span>
      <select
        aria-label={t("languageLabel")}
        value={preference}
        onChange={(event) => setPreference(event.target.value as LanguagePreference)}
      >
        <option value="system">{t("languageSystem")}</option>
        <option value="zh-CN">{t("languageChinese")}</option>
        <option value="en-US">{t("languageEnglish")}</option>
      </select>
    </label>
  );
}
