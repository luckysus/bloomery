import { useEffect, useState } from "react";
import {
  AlertCircle,
  Check,
  LoaderCircle,
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
import DiagnosticsHealthGrid from "./DiagnosticsHealthGrid";
import DiagnosticsTaskList from "./DiagnosticsTaskList";
import { formatBytes } from "./diagnosticsModel";

interface DiagnosticsSnapshot {
  storage: StorageHealth | null;
  index: IndexHealthReport | null;
  tasks: BackgroundTask[];
  steelPackage: {
    status: "ready" | "error" | "unknown";
    error: string | null;
  };
}

const emptySnapshot: DiagnosticsSnapshot = {
  storage: null,
  index: null,
  tasks: [],
  steelPackage: { status: "unknown", error: null },
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

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return fallback;
}

export default function DiagnosticsPage() {
  const { t } = useLocale();
  const [snapshot, setSnapshot] = useState(emptySnapshot);
  const [loading, setLoading] = useState(true);
  const [indexError, setIndexError] = useState(false);
  const [busyTask, setBusyTask] = useState<string | null>(null);
  const [busyBackup, setBusyBackup] = useState(false);
  const [busySteelPackage, setBusySteelPackage] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    setNotice(null);
    setIndexError(false);
    try {
      const [storage, retrievalValue, completedValue, profiles, tasks, domainPackages] = await Promise.all([
        desktop.getStorageHealth(),
        desktop.getSetting("onboarding.retrieval"),
        desktop.getSetting("onboarding.completed"),
        desktop.listProviderProfiles(),
        desktop.listBackgroundTasks(),
        desktop.listDomainPackages().catch(() => []),
      ]);
      const retrieval = parseObject(retrievalValue);
      const completed = parseObject(completedValue);
      const installedSteelPackage = domainPackages.find((item) => item.id === "steel");
      const steelPackageStatus = installedSteelPackage?.active
        ? "ready"
        : completed.steel_package_status === "ready"
        ? "ready"
        : completed.steel_package_status === "error" ? "error" : "unknown";
      const steelPackageError = stringValue(completed.steel_package_error);
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
      setSnapshot({
        storage,
        index,
        tasks,
        steelPackage: { status: steelPackageStatus, error: steelPackageError },
      });
      return true;
    } catch (cause) {
      setError(errorMessage(cause, t("diagnosticsLoadError")));
      return false;
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const retrySteelPackage = async () => {
    setBusySteelPackage(true);
    setError(null);
    setNotice(null);
    try {
      await desktop.installBundledSteelPackage();
      const completed = parseObject(await desktop.getSetting("onboarding.completed"));
      await desktop.setSetting("onboarding.completed", JSON.stringify({
        ...completed,
        version: typeof completed.version === "number" ? completed.version : 1,
        completed: true,
        steel_package_status: "ready",
        steel_package_error: null,
      }));
      const refreshed = await load();
      if (!refreshed) return;
      setSnapshot((current) => ({
        ...current,
        steelPackage: { status: "ready", error: null },
      }));
      setNotice(t("diagnosticsSteelPackageRepaired"));
    } catch (cause) {
      setError(errorMessage(cause, t("diagnosticsSteelPackageRepairError")));
    } finally {
      setBusySteelPackage(false);
    }
  };

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

  const storage = snapshot.storage;

  return (
    <section className="bloomery-diagnostics bloomery-page-surface" aria-labelledby="diagnostics-heading" aria-busy={loading}>
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
          <DiagnosticsHealthGrid
            storage={storage}
            index={snapshot.index}
            indexError={indexError}
            steelPackage={snapshot.steelPackage}
            busySteelPackage={busySteelPackage}
            onRetrySteelPackage={() => void retrySteelPackage()}
          />

          <DiagnosticsTaskList
            tasks={snapshot.tasks}
            busyTask={busyTask}
            onRetry={(task) => void retryTask(task)}
          />

          <aside className="bloomery-diagnostics-privacy"><Check size={17} aria-hidden="true" /><div><strong>{t("diagnosticsPrivacyTitle")}</strong><span>{t("diagnosticsPrivacyCopy")}</span></div></aside>
        </>
      )}
    </section>
  );
}
