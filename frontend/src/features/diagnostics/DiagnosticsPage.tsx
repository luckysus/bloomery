import { useEffect, useState } from "react";
import {
  AlertCircle,
  Check,
  Database,
  HardDrive,
  LoaderCircle,
  RotateCcw,
  SearchCheck,
} from "lucide-react";
import {
  desktop,
  type BackgroundTask,
  type IndexHealthReport,
  type StorageHealth,
  type ProviderProfileResponse,
} from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";
import DiagnosticsHeader from "./DiagnosticsHeader";

interface DiagnosticsSnapshot {
  storage: StorageHealth | null;
  index: IndexHealthReport | null;
  tasks: BackgroundTask[];
}

const emptySnapshot: DiagnosticsSnapshot = {
  storage: null,
  index: null,
  tasks: [],
};

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

function parseObject(value: string | null) {
  if (!value) return {} as Record<string, unknown>;
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" ? parsed as Record<string, unknown> : {};
  } catch {
    return {} as Record<string, unknown>;
  }
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function formatBytes(value: number | null | undefined) {
  if (value === null || value === undefined) return "--";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let amount = value;
  let unit = -1;
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024;
    unit += 1;
  }
  return `${amount.toFixed(amount >= 10 ? 0 : 1)} ${units[unit]}`;
}

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function taskLabel(kind: string, translate: (key: MessageKey) => string) {
  if (kind === "mineru_parse") return translate("taskLiteratureParse");
  if (kind === "rag_index_rebuild") return translate("taskIndexRebuild");
  return kind || translate("taskBackground");
}

export default function DiagnosticsPage() {
  const { t } = useLocale();
  const [snapshot, setSnapshot] = useState(emptySnapshot);
  const [loading, setLoading] = useState(true);
  const [indexError, setIndexError] = useState(false);
  const [busyTask, setBusyTask] = useState<string | null>(null);
  const [busyBackup, setBusyBackup] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    setNotice(null);
    setIndexError(false);
    try {
      const [storage, retrievalValue, profiles, tasks] = await Promise.all([
        desktop.getStorageHealth(),
        desktop.getSetting("onboarding.retrieval"),
        desktop.listProviderProfiles(),
        desktop.listBackgroundTasks(),
      ]);
      const retrieval = parseObject(retrievalValue);
      const embeddingId = stringValue(retrieval.embedding_profile_id);
      const embedding = profiles.find((profile: ProviderProfileResponse) => profile.id === embeddingId);
      let index: IndexHealthReport | null = null;
      if (embeddingId && embedding?.model_id) {
        try {
          index = await desktop.getIndexHealth({
            provider_profile_id: embeddingId,
            model_id: embedding.model_id,
            dimension: 1024,
          });
        } catch {
          setIndexError(true);
        }
      }
      setSnapshot({ storage, index, tasks });
    } catch (cause) {
      setError(errorMessage(cause, t("diagnosticsLoadError")));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const retryTask = async (task: BackgroundTask) => {
    setBusyTask(task.id);
    setError(null);
    setNotice(null);
    try {
      const updated = await desktop.retryBackgroundTask(task.id);
      setSnapshot((current) => ({
        ...current,
        tasks: current.tasks.map((item) => item.id === updated.id ? updated : item),
      }));
      setNotice(t("diagnosticsTaskRetried"));
    } catch (cause) {
      setError(errorMessage(cause, t("diagnosticsRetryError")));
    } finally {
      setBusyTask(null);
    }
  };

  const exportDiagnostics = async () => {
    setBusyBackup(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await desktop.saveFileDialog({
        title: t("diagnosticsExport"),
        defaultPath: "bloomery-diagnostics.json",
        filters: [{ name: t("diagnosticsExportFile"), extensions: ["json"] }],
      });
      if (typeof selected !== "string" || !selected.trim()) return;
      await desktop.writeDiagnosticsExport(selected);
      setNotice(t("diagnosticsExported"));
    } catch (cause) {
      setError(errorMessage(cause, t("diagnosticsExportError")));
    } finally {
      setBusyBackup(false);
    }
  };

  const createBackup = async () => {
    setBusyBackup(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await desktop.saveFileDialog({
        title: t("diagnosticsBackupExport"),
        defaultPath: "bloomery.bloomery-backup",
        filters: [{ name: t("diagnosticsBackupFile"), extensions: ["bloomery-backup"] }],
      });
      if (typeof selected !== "string" || !selected.trim()) return;
      await desktop.createBackup(selected);
      setNotice(t("diagnosticsBackupCreated"));
    } catch (cause) {
      setError(errorMessage(cause, t("diagnosticsBackupExportError")));
    } finally {
      setBusyBackup(false);
    }
  };

  const restoreBackup = async () => {
    setBusyBackup(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await desktop.openFileDialog({
        directory: false,
        multiple: false,
        title: t("diagnosticsBackupRestore"),
        filters: [{ name: t("diagnosticsBackupFile"), extensions: ["bloomery-backup", "zip"] }],
      });
      if (typeof selected !== "string" || !selected.trim()) return;
      const preview = await desktop.previewBackup(selected);
      const previewMessage = t("diagnosticsBackupRestorePreview", {
        databaseBytes: formatBytes(preview.database_bytes),
        contentFileCount: preview.content_file_count,
        contentBytes: formatBytes(preview.content_bytes),
      });
      if (!window.confirm(`${previewMessage}\n\n${t("diagnosticsBackupRestoreConfirm")}`)) return;
      await desktop.restoreBackup(selected);
      await load();
      setNotice(t("diagnosticsBackupRestored"));
    } catch (cause) {
      setError(errorMessage(cause, t("diagnosticsBackupRestoreError")));
    } finally {
      setBusyBackup(false);
    }
  };

  const failedTasks = snapshot.tasks.filter((task) =>
    Boolean(task.error_code) || task.state === "failed" || task.state === "interrupted",
  );
  const storage = snapshot.storage;
  const databaseHealthy = Boolean(storage?.database_ok);
  const indexHealthy = snapshot.index?.state === "healthy";

  return (
    <section className="bloomery-diagnostics" aria-labelledby="diagnostics-heading" aria-busy={loading}>
      <DiagnosticsHeader
        loading={loading}
        busy={busyBackup}
        onRefresh={() => void load()}
        onExport={() => void exportDiagnostics()}
        onCreateBackup={() => void createBackup()}
        onRestoreBackup={() => void restoreBackup()}
      />

      {error && <div className="bloomery-diagnostics-alert" role="alert"><AlertCircle size={17} aria-hidden="true" /><span>{error}</span></div>}
      {notice && <div className="bloomery-diagnostics-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}

      {loading && !storage ? (
        <div className="bloomery-diagnostics-loading"><LoaderCircle size={18} className="bloomery-spin" />{t("loading")}</div>
      ) : (
        <>
          <div className="bloomery-diagnostics-grid">
            <article className="bloomery-diagnostics-card">
              <div className="bloomery-diagnostics-card-heading">
                <span className="bloomery-diagnostics-card-icon"><Database size={17} aria-hidden="true" /></span>
                <div><p className="bloomery-eyebrow">SQLITE</p><h2>{t("diagnosticsDatabase")}</h2></div>
                <span className={`bloomery-diagnostics-status ${databaseHealthy ? "is-healthy" : "is-warning"}`}>
                  {databaseHealthy ? t("diagnosticsDatabaseHealthy") : t("diagnosticsDatabaseAttention")}
                </span>
              </div>
              <dl className="bloomery-diagnostics-details">
                <div><dt>{t("diagnosticsMigration")}</dt><dd>{storage ? `${storage.current_migration_version} / ${storage.latest_migration_version}` : "--"}</dd></div>
                <div><dt>{t("diagnosticsStorageSize")}</dt><dd>{formatBytes(storage?.database_size_bytes)}</dd></div>
                <div><dt>{t("diagnosticsReclaimable")}</dt><dd>{formatBytes(storage?.reclaimable_bytes)}</dd></div>
                <div><dt>{t("diagnosticsAvailableDisk")}</dt><dd>{formatBytes(storage?.available_disk_bytes)}</dd></div>
              </dl>
            </article>

            <article className="bloomery-diagnostics-card">
              <div className="bloomery-diagnostics-card-heading">
                <span className="bloomery-diagnostics-card-icon"><SearchCheck size={17} aria-hidden="true" /></span>
                <div><p className="bloomery-eyebrow">RAG INDEX</p><h2>{t("diagnosticsIndex")}</h2></div>
                <span className={`bloomery-diagnostics-status ${indexHealthy ? "is-healthy" : "is-warning"}`}>
                  {indexHealthy ? t("diagnosticsIndexHealthy") : t(indexError ? "diagnosticsIndexUnavailable" : "diagnosticsIndexAttention")}
                </span>
              </div>
              <dl className="bloomery-diagnostics-details">
                <div><dt>{t("diagnosticsServingMode")}</dt><dd>{snapshot.index?.serving_mode ?? (snapshot.index ? t("diagnosticsUnknown") : t("diagnosticsIndexUnconfigured"))}</dd></div>
                <div><dt>{t("diagnosticsChunkCount")}</dt><dd>{snapshot.index?.chunk_count ?? "--"}</dd></div>
                <div><dt>{t("diagnosticsRebuildSpace")}</dt><dd>{formatBytes(snapshot.index?.required_rebuild_bytes)}</dd></div>
                <div><dt>{t("diagnosticsStaleTemporary")}</dt><dd>{snapshot.index?.stale_temporary_count ?? "--"}</dd></div>
              </dl>
            </article>
          </div>

          <section className="bloomery-diagnostics-tasks" aria-labelledby="diagnostics-tasks-heading">
            <div className="bloomery-diagnostics-section-heading"><div><p className="bloomery-eyebrow">RECOVERY QUEUE</p><h2 id="diagnostics-tasks-heading">{t("diagnosticsTaskErrors")}</h2></div><HardDrive size={18} aria-hidden="true" /></div>
            {failedTasks.length === 0 ? <p className="bloomery-diagnostics-empty">{t("diagnosticsNoTaskErrors")}</p> : (
              <div className="bloomery-diagnostics-task-list">
                {failedTasks.map((task) => (
                  <div className="bloomery-diagnostics-task" key={task.id}>
                    <div><strong>{taskLabel(task.kind, t)}</strong><span>{task.error_code ?? t(taskStateKeys[task.state])}</span></div>
                    <div className="bloomery-diagnostics-task-meta"><span>{t(taskStateKeys[task.state])}</span><button type="button" className="bloomery-action-secondary" onClick={() => void retryTask(task)} disabled={!task.can_retry || busyTask === task.id}><RotateCcw size={15} aria-hidden="true" />{busyTask === task.id ? t("loading") : t("diagnosticsRetryTask")}</button></div>
                  </div>
                ))}
              </div>
            )}
          </section>

          <aside className="bloomery-diagnostics-privacy"><Check size={17} aria-hidden="true" /><div><strong>{t("diagnosticsPrivacyTitle")}</strong><span>{t("diagnosticsPrivacyCopy")}</span></div></aside>
        </>
      )}
    </section>
  );
}
