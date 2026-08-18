import { Monitor, Moon, Sun } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import { useTheme, type ThemePreference } from "../../theme/theme";

const options: Array<{
  value: ThemePreference;
  labelKey: "themeSystem" | "themeLight" | "themeDark";
  Icon: typeof Monitor;
}> = [
  { value: "system", labelKey: "themeSystem", Icon: Monitor },
  { value: "light", labelKey: "themeLight", Icon: Sun },
  { value: "dark", labelKey: "themeDark", Icon: Moon },
];

export default function ThemeSelect() {
  const { t } = useLocale();
  const { preference, setPreference } = useTheme();

  return (
    <section className="bloomery-theme-select" aria-labelledby="theme-heading">
      <div className="bloomery-theme-select-heading">
        <div>
          <h2 id="theme-heading">{t("themeTitle")}</h2>
        </div>
      </div>
      <div className="bloomery-theme-options" role="group" aria-labelledby="theme-heading">
        {options.map(({ value, labelKey, Icon }) => (
          <button
            key={value}
            type="button"
            className={`bloomery-theme-option ${preference === value ? "is-selected" : ""}`}
            aria-label={t(labelKey)}
            aria-pressed={preference === value}
            onClick={() => setPreference(value)}
          >
            <Icon size={18} aria-hidden="true" />
            <span>{t(labelKey)}</span>
          </button>
        ))}
      </div>
    </section>
  );
}
