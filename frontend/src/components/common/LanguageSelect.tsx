import { Languages } from "lucide-react";
import { useLocale, type LanguagePreference } from "../../i18n/locale";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

export default function LanguageSelect() {
  const { preference, setPreference, t } = useLocale();

  return (
    <label className="suna-language-control">
      <Languages size={15} aria-hidden="true" />
      <span className="sr-only">{t("languageLabel")}</span>
      <Select value={preference} onValueChange={(value) => setPreference(value as LanguagePreference)}>
  <SelectTrigger aria-label={t("languageLabel")}><SelectValue /></SelectTrigger>
  <SelectContent>
        <SelectItem value="system">{t("languageSystem")}</SelectItem>
        <SelectItem value="zh-CN">{t("languageChinese")}</SelectItem>
        <SelectItem value="en-US">{t("languageEnglish")}</SelectItem>
  </SelectContent>
</Select>
    </label>
  );
}
