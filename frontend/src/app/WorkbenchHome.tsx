import {
  ArrowUpRight,
  BookOpen,
  CircleCheck,
  Clock3,
  Database,
  FileUp,
  MessageSquarePlus,
  Server,
} from "lucide-react";
import { useLocale, type MessageKey } from "../i18n/locale";

interface WorkbenchHomeProps {
  initializationState: "loading" | "ready" | "failed";
  onOpenSection: (section: "chat" | "knowledge" | "analysis") => void;
}

const statusRows = [
  { labelKey: "localRuntime", valueKey: "started", icon: Server, tone: "good" },
  { labelKey: "knowledgeBase", valueKey: "notCreated", icon: BookOpen, tone: "neutral" },
  { labelKey: "backgroundTasks", valueKey: "noActiveTasks", icon: Clock3, tone: "neutral" },
] as const;

export default function WorkbenchHome({ initializationState, onOpenSection }: WorkbenchHomeProps) {
  const ready = initializationState === "ready";
  const { t } = useLocale();

  return (
    <div className="bloomery-workbench" data-testid="workbench-home">
      <section className="bloomery-hero" aria-labelledby="workbench-heading">
        <div>
          <p className="bloomery-eyebrow">{t("workbenchEyebrow")}</p>
          <h1 id="workbench-heading">{t("workbenchTitle")}</h1>
          <p className="bloomery-lede">{t("workbenchLede")}</p>
        </div>
        <div className="bloomery-hero-mark" aria-hidden="true">
          <span className="bloomery-hero-mark-line" />
          <span className="bloomery-hero-mark-line bloomery-hero-mark-line-short" />
          <span className="bloomery-hero-mark-dot" />
        </div>
      </section>

      <section className="bloomery-action-strip" aria-label={t("commonActions")}>
        <button type="button" className="bloomery-action-primary" onClick={() => onOpenSection("chat")}>
          <MessageSquarePlus size={18} aria-hidden="true" />
          <span>{t("newConversation")}</span>
          <ArrowUpRight size={16} aria-hidden="true" />
        </button>
        <button type="button" className="bloomery-action-secondary" onClick={() => onOpenSection("knowledge")}>
          <FileUp size={17} aria-hidden="true" />
          <span>{t("importDocument")}</span>
        </button>
        <button type="button" className="bloomery-action-secondary" onClick={() => onOpenSection("analysis")}>
          <Database size={17} aria-hidden="true" />
          <span>{t("importData")}</span>
        </button>
      </section>

      <section className="bloomery-status-section" aria-labelledby="status-heading">
        <div className="bloomery-section-heading">
          <div>
            <p className="bloomery-eyebrow">RUNTIME STATUS</p>
            <h2 id="status-heading">{t("runtimeStatus")}</h2>
          </div>
          <span className={`bloomery-state-badge ${ready ? "is-good" : "is-pending"}`}>
            <span className="bloomery-state-dot" aria-hidden="true" />
            {ready ? t("runtimeReady") : initializationState === "failed" ? t("runtimeCheck") : t("runtimeInitializing")}
          </span>
        </div>
        <div className="bloomery-status-list">
          {statusRows.map(({ labelKey, valueKey, icon: Icon, tone }) => (
            <div className="bloomery-status-row" key={labelKey}>
              <span className={`bloomery-status-icon is-${tone}`} aria-hidden="true">
                <Icon size={16} />
              </span>
              <span className="bloomery-status-label">{t(labelKey as MessageKey)}</span>
              <span className="bloomery-status-value">{t(valueKey as MessageKey)}</span>
              <CircleCheck className="bloomery-status-check" size={16} aria-hidden="true" />
            </div>
          ))}
        </div>
      </section>

      <section className="bloomery-empty-section" aria-labelledby="recent-heading">
        <div className="bloomery-section-heading">
          <div>
            <p className="bloomery-eyebrow">RECENT WORK</p>
            <h2 id="recent-heading">{t("recentWork")}</h2>
          </div>
          <span className="bloomery-muted-label">{t("records", { count: 0 })}</span>
        </div>
        <div className="bloomery-empty-state">
          <div className="bloomery-empty-icon" aria-hidden="true">
            <BookOpen size={22} />
          </div>
          <div>
            <strong>{t("noRecords")}</strong>
            <p>{t("workbenchEmptyCopy")}</p>
          </div>
        </div>
      </section>
    </div>
  );
}
