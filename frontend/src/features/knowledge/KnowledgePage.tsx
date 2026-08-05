import { useEffect, useState, type FormEvent } from "react";
import {
  AlertCircle,
  Check,
  Database,
  FileText,
  FolderUp,
  LoaderCircle,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { useLocale } from "../../i18n/locale";
import {
  desktop,
  type BackgroundTask,
  type KnowledgeBaseDeleteImpact,
  type KnowledgeBaseRecord,
  type KnowledgeHealth,
  type SourceDocumentRecord,
} from "../../bridge/desktop";

const emptyHealth: KnowledgeHealth = {
  knowledge_base_count: 0,
  document_count: 0,
  active_document_count: 0,
  version_count: 0,
  chunk_count: 0,
  indexed_chunk_count: 0,
  active_task_count: 0,
};

interface RetrievalSetup {
  embeddingProfileId: string | null;
  mineruProfileId: string | null;
}

const emptyRetrieval: RetrievalSetup = {
  embeddingProfileId: null,
  mineruProfileId: null,
};

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function parseRetrievalSetup(value: string | null): RetrievalSetup {
  if (!value) return emptyRetrieval;
  try {
    const parsed = JSON.parse(value) as Record<string, unknown>;
    return {
      embeddingProfileId: typeof parsed.embedding_profile_id === "string" ? parsed.embedding_profile_id : null,
      mineruProfileId: typeof parsed.mineru_profile_id === "string" ? parsed.mineru_profile_id : null,
    };
  } catch {
    return emptyRetrieval;
  }
}

function taskStateLabel(task: BackgroundTask, translate: (key: "taskQueued" | "taskProcessing" | "taskCompleted" | "taskFailed" | "taskCancelled" | "taskInterrupted" | "taskPaused") => string) {
  switch (task.state) {
    case "queued":
      return translate("taskQueued");
    case "running":
    case "waiting_external":
      return translate("taskProcessing");
    case "completed":
      return translate("taskCompleted");
    case "failed":
      return translate("taskFailed");
    case "cancelled":
      return translate("taskCancelled");
    case "interrupted":
      return translate("taskInterrupted");
    default:
      return translate("taskPaused");
  }
}

function taskKindLabel(kind: string, translate: (key: "taskLiteratureParse" | "taskIndexRebuild" | "taskBackground") => string) {
  return kind === "mineru_parse" ? translate("taskLiteratureParse") : kind === "rag_index_rebuild" ? translate("taskIndexRebuild") : translate("taskBackground");
}

function documentStateLabel(document: SourceDocumentRecord, translate: (key: "documentActive" | "documentProcessing") => string) {
  return document.active_version_id ? translate("documentActive") : translate("documentProcessing");
}

export default function KnowledgePage() {
  const { t } = useLocale();
  const [bases, setBases] = useState<KnowledgeBaseRecord[]>([]);
  const [selectedBaseId, setSelectedBaseId] = useState<string | null>(null);
  const [documents, setDocuments] = useState<SourceDocumentRecord[]>([]);
  const [tasks, setTasks] = useState<BackgroundTask[]>([]);
  const [health, setHealth] = useState<KnowledgeHealth>(emptyHealth);
  const [retrieval, setRetrieval] = useState<RetrievalSetup>(emptyRetrieval);
  const [newName, setNewName] = useState("");
  const [filePath, setFilePath] = useState("");
  const [renameId, setRenameId] = useState<string | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deleteImpact, setDeleteImpact] = useState<KnowledgeBaseDeleteImpact | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadOverview = async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextBases, nextHealth, nextTasks, setting] = await Promise.all([
        desktop.listKnowledgeBases(),
        desktop.getKnowledgeHealth(),
        desktop.listBackgroundTasks(),
        desktop.getSetting("onboarding.retrieval"),
      ]);
      setBases(nextBases);
      setHealth(nextHealth);
      setTasks(nextTasks);
      setRetrieval(parseRetrievalSetup(setting));
      setSelectedBaseId((current) => current && nextBases.some((base) => base.id === current) ? current : nextBases[0]?.id ?? null);
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadOverview();
  }, []);

  useEffect(() => {
    if (!selectedBaseId) {
      setDocuments([]);
      return;
    }
    let mounted = true;
    desktop.listKnowledgeDocuments(selectedBaseId).then((items) => {
      if (mounted) setDocuments(items);
    }).catch((cause) => {
      if (mounted) setError(errorMessage(cause, t("knowledgeError")));
    });
    return () => {
      mounted = false;
    };
  }, [selectedBaseId]);

  const selectedBase = bases.find((base) => base.id === selectedBaseId) ?? null;

  const createBase = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = newName.trim();
    if (!name) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const created = await desktop.createKnowledgeBase(name);
      setBases((current) => [...current, created]);
      setSelectedBaseId(created.id);
      setNewName("");
      setHealth((current) => ({ ...current, knowledge_base_count: current.knowledge_base_count + 1 }));
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const startRename = (base: KnowledgeBaseRecord) => {
    setRenameId(base.id);
    setRenameName(base.name);
    setError(null);
  };

  const saveRename = async () => {
    if (!renameId || !renameName.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const updated = await desktop.renameKnowledgeBase(renameId, renameName.trim());
      setBases((current) => current.map((base) => base.id === updated.id ? updated : base));
      setRenameId(null);
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const requestDelete = async (base: KnowledgeBaseRecord) => {
    setBusy(true);
    setError(null);
    try {
      setDeleteImpact(await desktop.previewDeleteKnowledgeBase(base.id));
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteImpact) return;
    setBusy(true);
    setError(null);
    try {
      await desktop.deleteKnowledgeBaseConfirmed(deleteImpact.knowledge_base_id);
      const remaining = bases.filter((base) => base.id !== deleteImpact.knowledge_base_id);
      setBases(remaining);
      setSelectedBaseId(remaining[0]?.id ?? null);
      setDeleteImpact(null);
      setHealth((current) => ({ ...current, knowledge_base_count: Math.max(0, current.knowledge_base_count - 1) }));
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  const importDocument = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedBaseId || !filePath.trim()) return;
    if (!retrieval.embeddingProfileId || !retrieval.mineruProfileId) {
      setError(t("setupRetrievalFirst"));
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await desktop.importLocalDocument({
        source_path: filePath.trim(),
        knowledge_base: { mode: "existing", id: selectedBaseId },
        mineru_profile_id: retrieval.mineruProfileId,
        embedding_profile_id: retrieval.embeddingProfileId,
        embedding_dimension: 1024,
      });
      setFilePath("");
      setNotice(t("importCreated"));
      await loadOverview();
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="bloomery-knowledge" aria-labelledby="knowledge-heading">
      <header className="bloomery-knowledge-header">
        <div>
          <p className="bloomery-eyebrow">LOCAL KNOWLEDGE / STEEL DOMAIN</p>
          <h1 id="knowledge-heading">{t("knowledgeTitle")}</h1>
          <p className="bloomery-lede">{t("knowledgeLede")}</p>
        </div>
        <button type="button" className="bloomery-icon-button" onClick={() => void loadOverview()} disabled={loading} aria-label={t("refreshKnowledge")} title={t("refreshKnowledge")}>
          <RefreshCw size={17} aria-hidden="true" />
        </button>
      </header>

      {error && <div className="bloomery-knowledge-alert" role="alert"><AlertCircle size={17} aria-hidden="true" /><span>{error}</span></div>}
      {notice && <div className="bloomery-knowledge-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}

      <div className="bloomery-knowledge-metrics" aria-label={t("knowledgeStats")}>
        <div><span>{t("knowledgeBase")}</span><strong>{health.knowledge_base_count}</strong></div>
        <div><span>{t("document")}</span><strong>{health.document_count}</strong></div>
        <div><span>{t("activeDocuments")}</span><strong>{health.active_document_count}</strong></div>
        <div><span>{t("indexedChunks")}</span><strong>{health.indexed_chunk_count} / {health.chunk_count}</strong></div>
        <div><span>{t("activeTasks")}</span><strong>{health.active_task_count}</strong></div>
      </div>

      <div className="bloomery-knowledge-layout">
        <aside className="bloomery-knowledge-sidebar" aria-label={t("knowledgeDirectory")}>
          <div className="bloomery-knowledge-section-heading"><span>{t("knowledgeDirectory")}</span><Database size={16} aria-hidden="true" /></div>
          <form className="bloomery-knowledge-create" onSubmit={createBase}>
            <label htmlFor="knowledge-base-name">{t("baseName")}</label>
            <div>
              <input id="knowledge-base-name" value={newName} onChange={(event) => setNewName(event.target.value)} placeholder={t("baseNamePlaceholder")} />
              <button type="submit" className="bloomery-icon-button" disabled={busy || !newName.trim()} aria-label={t("createKnowledgeBase")} title={t("createKnowledgeBase")}><Plus size={17} aria-hidden="true" /></button>
            </div>
          </form>
          {loading ? <div className="bloomery-knowledge-loading"><LoaderCircle size={17} className="bloomery-spin" />{t("loading")}</div> : bases.length === 0 ? (
            <p className="bloomery-knowledge-empty">{t("noKnowledgeBases")}</p>
          ) : (
            <div className="bloomery-knowledge-base-list">
              {bases.map((base) => (
                <div key={base.id} className={`bloomery-knowledge-base ${selectedBaseId === base.id ? "is-active" : ""}`}>
                  {renameId === base.id ? (
                    <div className="bloomery-knowledge-rename">
                      <input aria-label={t("renameKnowledgeBase")} value={renameName} onChange={(event) => setRenameName(event.target.value)} autoFocus />
                      <button type="button" className="bloomery-icon-button" onClick={() => void saveRename()} disabled={busy} aria-label={t("saveName")}><Check size={15} /></button>
                      <button type="button" className="bloomery-icon-button" onClick={() => setRenameId(null)} aria-label={t("cancelRename")}><X size={15} /></button>
                    </div>
                  ) : (
                    <>
                      <button type="button" className="bloomery-knowledge-base-select" onClick={() => setSelectedBaseId(base.id)}>{base.name}</button>
                      <button type="button" className="bloomery-icon-button" onClick={() => startRename(base)} aria-label={`${t("rename")} ${base.name}`} title={t("rename")}><Pencil size={14} /></button>
                      <button type="button" className="bloomery-icon-button" onClick={() => void requestDelete(base)} aria-label={`${t("delete")} ${base.name}`} title={t("delete")}><Trash2 size={14} /></button>
                    </>
                  )}
                </div>
              ))}
            </div>
          )}
        </aside>

        <div className="bloomery-knowledge-content">
          {selectedBase ? (
            <>
              <div className="bloomery-knowledge-content-header">
                <div><p className="bloomery-eyebrow">SELECTED KNOWLEDGE BASE</p><h2>{selectedBase.name}</h2></div>
                <span className="bloomery-knowledge-count">{t("documents", { count: documents.length })}</span>
              </div>
              <form className="bloomery-knowledge-import" onSubmit={importDocument}>
                <div className="bloomery-knowledge-import-title"><FolderUp size={18} aria-hidden="true" /><strong>{t("importLocalDocument")}</strong></div>
                <label htmlFor="knowledge-file-path">{t("filePath")}</label>
                <div className="bloomery-knowledge-import-row">
                  <input id="knowledge-file-path" value={filePath} onChange={(event) => setFilePath(event.target.value)} placeholder={t("filePathPlaceholder")} required />
                  <button type="submit" className="bloomery-action-primary" disabled={busy || !filePath.trim()}><FileText size={17} aria-hidden="true" />{t("importDocumentAction")}</button>
                </div>
                <p>{t("importDescription")}</p>
              </form>

              <section className="bloomery-knowledge-list-section" aria-labelledby="documents-heading">
                <div className="bloomery-knowledge-section-heading"><h3 id="documents-heading">{t("document")}</h3><span>{t("items", { count: documents.length })}</span></div>
                {documents.length === 0 ? <div className="bloomery-knowledge-empty-content"><FileText size={21} /><span>{t("noDocuments")}</span></div> : (
                  <div className="bloomery-knowledge-document-list">
                    {documents.map((document) => <div className="bloomery-knowledge-document" key={document.id}><FileText size={18} /><div><strong>{document.display_name}</strong><span>{document.source_kind.toUpperCase()} · {documentStateLabel(document, (key) => t(key))}</span></div></div>)}
                  </div>
                )}
              </section>
            </>
          ) : (
            <div className="bloomery-knowledge-empty-content is-large"><Database size={28} /><strong>{t("createKnowledgeBaseFirst")}</strong><span>{t("knowledgeBaseDescription")}</span></div>
          )}

          <section className="bloomery-knowledge-list-section" aria-labelledby="tasks-heading">
            <div className="bloomery-knowledge-section-heading"><h3 id="tasks-heading">{t("backgroundTasks")}</h3><span>{t("items", { count: tasks.length })}</span></div>
            {tasks.length === 0 ? <div className="bloomery-knowledge-empty-content"><LoaderCircle size={19} /><span>{t("noBackgroundTasks")}</span></div> : (
              <div className="bloomery-knowledge-task-list">
                {tasks.slice(0, 5).map((task) => <div className="bloomery-knowledge-task" key={task.id}><div><strong>{taskKindLabel(task.kind, (key) => t(key))}</strong><span>{taskStateLabel(task, (key) => t(key))} · {t("taskAttempt", { count: task.attempt })}</span></div><div className="bloomery-knowledge-progress"><span style={{ width: `${task.progress}%` }} /><b>{task.progress}%</b></div></div>)}
              </div>
            )}
          </section>
        </div>
      </div>

      {deleteImpact && <div className="bloomery-knowledge-confirm" role="dialog" aria-modal="true" aria-labelledby="delete-heading"><div className="bloomery-knowledge-confirm-panel"><p className="bloomery-eyebrow">{t("destructiveAction")}</p><h2 id="delete-heading">{t("deleteKnowledgeBaseTitle", { name: deleteImpact.name })}</h2><p>{t("deleteKnowledgeBaseImpact", { documents: deleteImpact.document_count, versions: deleteImpact.version_count, chunks: deleteImpact.chunk_count })}</p>{deleteImpact.active_task_count > 0 && <strong className="bloomery-knowledge-danger">{t("activeTaskWarning")}</strong>}<div><button type="button" className="bloomery-action-secondary" onClick={() => setDeleteImpact(null)}>{t("cancel")}</button><button type="button" className="bloomery-action-primary" onClick={() => void confirmDelete()} disabled={busy || deleteImpact.active_task_count > 0}><Trash2 size={16} aria-hidden="true" />{t("confirmDelete")}</button></div></div></div>}
    </section>
  );
}
