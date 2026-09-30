import { useLocale, type MessageKey } from "../../i18n/locale";
import type { LucideIcon } from "lucide-react";

export interface SettingsTabOption<T extends string> {
  id: T;
  labelKey: MessageKey;
  icon?: LucideIcon;
  description?: string;
}

export default function SettingsTabList<T extends string>({
  tabs,
  activeTab,
  onSelect,
}: {
  tabs: readonly SettingsTabOption<T>[];
  activeTab: T;
  onSelect: (tab: T) => void;
}) {
  const { t } = useLocale();
  return (
    <div className="suna-settings-tabs" role="tablist" aria-label={t("settingsTitle")}>
      {tabs.map((tab) => {
        const Icon = tab.icon;
        return <button
          key={tab.id}
          type="button"
          role="tab"
          id={`settings-tab-${tab.id}`}
          aria-selected={activeTab === tab.id}
          aria-controls={`settings-panel-${tab.id}`}
          className={`suna-settings-tab ${activeTab === tab.id ? "is-active" : ""}`}
          onClick={() => onSelect(tab.id)}
        >
          {Icon && <Icon size={16} aria-hidden="true" />}
          <span>{t(tab.labelKey)}</span>
          {tab.description && <small aria-hidden="true">{tab.description}</small>}
        </button>;
      })}
    </div>
  );
}
