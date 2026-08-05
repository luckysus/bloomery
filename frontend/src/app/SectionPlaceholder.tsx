import { CircleDashed } from "lucide-react";
import { useLocale } from "../i18n/locale";
import type { NavigationSection } from "./navigation";

interface SectionPlaceholderProps {
  section: NavigationSection;
}

export default function SectionPlaceholder({ section }: SectionPlaceholderProps) {
  const Icon = section.icon;
  const { t } = useLocale();

  return (
    <section className="bloomery-placeholder" aria-labelledby="section-heading">
      <div className="bloomery-placeholder-icon" aria-hidden="true">
        <Icon size={22} />
      </div>
      <p className="bloomery-eyebrow">{t("localModule")}</p>
      <h1 id="section-heading">{t(section.labelKey)}</h1>
      <p className="bloomery-placeholder-copy">{t(section.descriptionKey)}</p>
      <div className="bloomery-placeholder-status">
        <CircleDashed size={17} aria-hidden="true" />
        <span>{t("waitingLocalData")}</span>
      </div>
    </section>
  );
}
