import {
  AlertCircle,
  ArrowUpRight,
  BookOpen,
  BrainCircuit,
  CircleAlert,
  CircleCheck,
  Clock3,
  Database,
  FileUp,
  LoaderCircle,
  MessageSquare,
  MessageSquarePlus,
  RefreshCw,
  Server,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useLocale, type MessageKey } from "../i18n/locale";
import { desktop, type BackgroundTask, type Conversation } from "../bridge/desktop";
import { useWorkbenchOverview } from "../features/workbench/useWorkbenchOverview";

interface WorkbenchHomeProps {
  initializationState: "loading" | "ready" | "failed";
  onOpenSection: (section: "chat" | "knowledge" | "analysis") => void;
}

function isActiveTask(task: BackgroundTask) {
  return task.state === "queued" || task.state === "running" || task.state === "waiting_external" || task.state === "paused";
}

function taskLabel(task: BackgroundTask, t: (key: MessageKey) => string) {
  if (task.kind === "mineru_parse") return t("taskLiteratureParse");
  if (task.kind === "rag_index_rebuild") return t("taskIndexRebuild");
  return t("taskBackground");
}

function taskStateLabel(task: BackgroundTask, t: (key: MessageKey) => string) {
  if (task.state === "queued") return t("taskQueued");
  if (task.state === "running" || task.state === "waiting_external") return t("taskProcessing");
  if (task.state === "failed") return t("taskFailed");
  if (task.state === "cancelled") return t("taskCancelled");
  if (task.state === "interrupted") return t("taskInterrupted");
  return t("taskPaused");
}

function recentConversations(items: Conversation[]) {
  return items
    .filter((conversation) => !conversation.archived)
    .sort((left, right) => right.updated_at.localeCompare(left.updated_at))
    .slice(0, 5);
}

export default function WorkbenchHome({ initializationState, onOpenSection }: WorkbenchHomeProps) {
  const ready = initializationState === "ready";
  const { t } = useLocale();
  const overview = useWorkbenchOverview(ready);
  const conversations = recentConversations(overview.conversations);
  const activeTasks = overview.backgroundTasks.filter(isActiveTask).slice(0, 5);
  const health = overview.health;
  const knowledgeFailed = overview.failedSources.includes("knowledgeBases") || overview.failedSources.includes("knowledgeHealth");
  const tasksFailed = overview.failedSources.includes("backgroundTasks") || overview.failedSources.includes("knowledgeHealth");
  const knowledgeStatus = overview.loading
    ? t("loading")
    : knowledgeFailed
      ? t("runtimeCheck")
      : health && health.knowledge_base_count > 0
        ? t("workbenchKnowledgeCount", { count: health.knowledge_base_count })
        : t("notCreated");
  const activeTaskCount = health?.active_task_count ?? activeTasks.length;
  const taskStatus = overview.loading
    ? t("loading")
    : tasksFailed
      ? t("runtimeCheck")
      : activeTaskCount > 0
        ? t("workbenchActiveTaskCount", { count: activeTaskCount })
        : t("noActiveTasks");

  const [providerStatus, setProviderStatus] = useState<"loading" | "missing" | "secret" | "ready">("loading");
  const [providerLabel, setProviderLabel] = useState("");
  useEffect(() => {
    if (!ready) return;
    let mounted = true;
    desktop
      .listProviderProfiles()
      .then((profiles) => {
        if (!mounted) return;
        const chat = profiles.find(
          (profile) => profile.enabled && (profile.kind === "open_ai_compatible" || profile.kind === "ollama"),
        );
        if (!chat) {
          setProviderStatus("missing");
          setProviderLabel(t("providerNotConfigured"));
        } else if (!chat.secret_configured) {
          setProviderStatus("secret");
          setProviderLabel(t("providerSecretMissing"));
        } else {
          setProviderStatus("ready");
          setProviderLabel(chat.display_name);
        }
      })
      .catch(() => {
        if (mounted) {
          setProviderStatus("missing");
          setProviderLabel(t("providerNotConfigured"));
        }
      });
    return () => {
      mounted = false;
    };
  }, [ready, t]);

  const statusRows = [
    { labelKey: "localRuntime", value: t("started"), icon: Server, tone: "good", testId: undefined },
    { labelKey: "modelProvider", value: providerStatus === "loading" ? t("loading") : providerLabel, icon: BrainCircuit, tone: providerStatus === "ready" ? "good" : "pending", testId: "workbench-provider-status" },
    { labelKey: "knowledgeBase", value: knowledgeStatus, icon: BookOpen, tone: knowledgeFailed ? "pending" : health?.knowledge_base_count ? "good" : "neutral", testId: "workbench-knowledge-status" },
    { labelKey: "backgroundTasks", value: taskStatus, icon: Clock3, tone: tasksFailed ? "pending" : activeTaskCount ? "good" : "neutral", testId: "workbench-task-status" },
  ] as const;

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

      {overview.failedSources.length > 0 && (
        <div className="bloomery-workbench-alert" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("workbenchLoadError")}</span>
        </div>
      )}

      <section className="bloomery-status-section" aria-labelledby="status-heading" aria-busy={overview.loading}>
        <div className="bloomery-section-heading">
          <div>
            <p className="bloomery-eyebrow">RUNTIME STATUS</p>
            <h2 id="status-heading">{t("runtimeStatus")}</h2>
          </div>
          <div className="bloomery-section-heading-actions">
            <button type="button" className="bloomery-icon-button" onClick={overview.refresh} disabled={overview.loading || !ready} aria-label={t("refreshWorkbench")} title={t("refreshWorkbench")}>
              <RefreshCw size={17} aria-hidden="true" className={overview.loading ? "bloomery-spin" : undefined} />
            </button>
            <span className={`bloomery-state-badge ${ready && !overview.failedSources.length ? "is-good" : "is-pending"}`}>
              <span className="bloomery-state-dot" aria-hidden="true" />
              {ready ? (overview.loading ? t("loading") : overview.failedSources.length ? t("runtimeCheck") : t("runtimeReady")) : initializationState === "failed" ? t("runtimeCheck") : t("runtimeInitializing")}
            </span>
          </div>
        </div>
        <div className="bloomery-status-list">
          {statusRows.map(({ labelKey, value, icon: Icon, tone, testId }) => {
            const StatusIcon = tone === "good" ? CircleCheck : CircleAlert;
            return (
              <div className="bloomery-status-row" key={labelKey}>
                <span className={`bloomery-status-icon is-${tone}`} aria-hidden="true"><Icon size={16} /></span>
                <span className="bloomery-status-label">{t(labelKey as MessageKey)}</span>
                <span className="bloomery-status-value" data-testid={testId}>{value}</span>
                <StatusIcon className={`bloomery-status-check ${tone === "good" ? "is-good" : "is-pending"}`} size={16} aria-hidden="true" />
              </div>
            );
          })}
        </div>
      </section>

      <section className="bloomery-empty-section" aria-labelledby="recent-heading" aria-busy={overview.loading}>
        <div className="bloomery-section-heading">
          <div>
            <p className="bloomery-eyebrow">RECENT WORK</p>
            <h2 id="recent-heading">{t("recentWork")}</h2>
          </div>
          <span className="bloomery-muted-label" data-testid="workbench-record-count">{t("records", { count: conversations.length })}</span>
        </div>
        {overview.loading ? (
          <div className="bloomery-workbench-loading" role="status"><LoaderCircle size={18} className="bloomery-spin" />{t("loading")}</div>
        ) : conversations.length === 0 && activeTasks.length === 0 ? (
          <div className="bloomery-empty-state">
            <div className="bloomery-empty-icon" aria-hidden="true"><BookOpen size={22} /></div>
            <div><strong>{t("noRecords")}</strong><p>{t("workbenchEmptyCopy")}</p></div>
          </div>
        ) : (
          <div className="bloomery-recent-list">
            {conversations.length > 0 && (
              <div className="bloomery-recent-group">
                <p className="bloomery-recent-group-title">{t("workbenchRecentConversations")}</p>
                {conversations.map((conversation) => (
                  <div className="bloomery-recent-row" key={conversation.id}>
                    <MessageSquare size={17} aria-hidden="true" />
                    <strong>{conversation.title || t("newConversation")}</strong>
                  </div>
                ))}
              </div>
            )}
            {activeTasks.length > 0 && (
              <div className="bloomery-recent-group">
                <p className="bloomery-recent-group-title">{t("workbenchRecentTasks")}</p>
                {activeTasks.map((task) => (
                  <div className="bloomery-recent-row" key={task.id}>
                    <Clock3 size={17} aria-hidden="true" />
                    <strong>{taskLabel(task, (key) => t(key))}</strong>
                    <span>{taskStateLabel(task, (key) => t(key))} · {t("workbenchTaskProgress", { progress: task.progress })}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
