import { CircleDashed } from "lucide-react";
import type { NavigationSection } from "./navigation";

interface SectionPlaceholderProps {
  section: NavigationSection;
}

export default function SectionPlaceholder({ section }: SectionPlaceholderProps) {
  const Icon = section.icon;

  return (
    <section className="bloomery-placeholder" aria-labelledby="section-heading">
      <div className="bloomery-placeholder-icon" aria-hidden="true">
        <Icon size={22} />
      </div>
      <p className="bloomery-eyebrow">LOCAL MODULE</p>
      <h1 id="section-heading">{section.label}</h1>
      <p className="bloomery-placeholder-copy">{section.description}</p>
      <div className="bloomery-placeholder-status">
        <CircleDashed size={17} aria-hidden="true" />
        <span>等待本地数据</span>
      </div>
    </section>
  );
}
