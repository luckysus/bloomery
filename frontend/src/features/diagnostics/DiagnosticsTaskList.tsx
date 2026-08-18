import { HardDrive, RotateCcw } from "lucide-react";
import type { BackgroundTask } from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";

const taskStateKeys: Record<BackgroundTask["state"], MessageKey> = {
  queued: "diagnosticsTaskQueued",
  running: "diagnosticsTaskRunning",
  waiting_external: "diagnosticsTaskWaitingExternal",
  paused: "diagnosticsTaskPaused",
  completed: "diagnosticsTaskCompleted",
  failed: "diagnosticsTaskFailed",
  cancelled: "diagnosticsTaskCancelled",
  interrupted: "diagnosticsTaskInterrupted",
};

function taskLabel(kind: string, translate: (key: MessageKey) => string) {
  if (kind === "mineru_parse") return translate("taskLiteratureParse");
  if (kind === "rag_index_rebuild") return translate("taskIndexRebuild");
  return kind || translate("taskBackground");
}

interface DiagnosticsTaskListProps {
  tasks: BackgroundTask[];
  busyTask: string | null;
  onRetry: (task: BackgroundTask) => void;
}

export default function DiagnosticsTaskList({ tasks, busyTask, onRetry }: DiagnosticsTaskListProps) {
  const { t } = useLocale();
  const failedTasks = tasks.filter((task) =>
    Boolean(task.error_code) || task.state === "failed" || task.state === "interrupted",
  );

  return (
    <section className="bloomery-diagnostics-tasks" aria-labelledby="diagnostics-tasks-heading">
      <div className="bloomery-diagnostics-section-heading"><div><h2 id="diagnostics-tasks-heading">{t("diagnosticsTaskErrors")}</h2></div><HardDrive size={18} aria-hidden="true" /></div>
      {failedTasks.length === 0 ? <p className="bloomery-diagnostics-empty">{t("diagnosticsNoTaskErrors")}</p> : (
        <div className="bloomery-diagnostics-task-list">
          {failedTasks.map((task) => (
            <div className="bloomery-diagnostics-task" key={task.id}>
              <div><strong>{taskLabel(task.kind, t)}</strong><span>{task.error_code ?? t(taskStateKeys[task.state])}</span></div>
              <div className="bloomery-diagnostics-task-meta"><span>{t(taskStateKeys[task.state])}</span><button type="button" className="bloomery-action-secondary" onClick={() => onRetry(task)} disabled={!task.can_retry || busyTask === task.id}><RotateCcw size={15} aria-hidden="true" />{busyTask === task.id ? t("loading") : t("diagnosticsRetryTask")}</button></div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
