import { useLocale, type MessageKey } from "../../i18n/locale";

export interface SettingsTabOption<T extends string> {
  id: T;
  labelKey: MessageKey;
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
    <div className="bloomery-settings-tabs" role="tablist" aria-label={t("settingsTitle")}>
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          id={`settings-tab-${tab.id}`}
          aria-selected={activeTab === tab.id}
          aria-controls={`settings-panel-${tab.id}`}
          className={`bloomery-settings-tab ${activeTab === tab.id ? "is-active" : ""}`}
          onClick={() => onSelect(tab.id)}
        >
          {t(tab.labelKey)}
        </button>
      ))}
    </div>
  );
}
