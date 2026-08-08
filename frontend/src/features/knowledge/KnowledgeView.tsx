import type { FormEvent } from "react";
import {
  AlertCircle,
  Check,
  Database,
  FileText,
  FolderOpen,
  FolderUp,
  LoaderCircle,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { useLocale } from "../../i18n/locale";
import type {
  BackgroundTask,
  KnowledgeBaseDeleteImpact,
  KnowledgeBaseRecord,
  KnowledgeHealth,
  SourceDocumentRecord,
} from "../../bridge/desktop";

export interface KnowledgeViewProps {
  bases: KnowledgeBaseRecord[];
  selectedBaseId: string | null;
  documents: SourceDocumentRecord[];
  tasks: BackgroundTask[];
  health: KnowledgeHealth;
  newName: string;
  filePath: string;
  renameId: string | null;
  renameName: string;
  deleteImpact: KnowledgeBaseDeleteImpact | null;
  loading: boolean;
  busy: boolean;
  error: string | null;
  notice: string | null;
  onRefresh: () => void;
  onCreateBase: (event: FormEvent<HTMLFormElement>) => void;
  onNewNameChange: (value: string) => void;
  onSelectBase: (id: string) => void;
  onStartRename: (base: KnowledgeBaseRecord) => void;
  onRenameNameChange: (value: string) => void;
  onSaveRename: () => void;
  onCancelRename: () => void;
  onRequestDelete: (base: KnowledgeBaseRecord) => void;
  onCancelDelete: () => void;
  onConfirmDelete: () => void;
  onImportDocument: (event: FormEvent<HTMLFormElement>) => void;
  onFilePathChange: (value: string) => void;
  onChooseFile: () => void;
}

function taskStateLabel(task: BackgroundTask, translate: (key: "taskQueued" | "taskProcessing" | "taskCompleted" | "taskFailed" | "taskCancelled" | "taskInterrupted" | "taskPaused") => string) {
  switch (task.state) {
    case "queued": return translate("taskQueued");
    case "running":
    case "waiting_external": return translate("taskProcessing");
    case "completed": return translate("taskCompleted");
    case "failed": return translate("taskFailed");
    case "cancelled": return translate("taskCancelled");
    case "interrupted": return translate("taskInterrupted");
    default: return translate("taskPaused");
  }
}

function taskKindLabel(kind: string, translate: (key: "taskLiteratureParse" | "taskIndexRebuild" | "taskBackground") => string) {
  return kind === "mineru_parse" ? translate("taskLiteratureParse") : kind === "rag_index_rebuild" ? translate("taskIndexRebuild") : translate("taskBackground");
}

function documentStateLabel(document: SourceDocumentRecord, translate: (key: "documentActive" | "documentProcessing") => string) {
  return document.active_version_id ? translate("documentActive") : translate("documentProcessing");
}

export default function KnowledgeView({
  bases,
  selectedBaseId,
  documents,
  tasks,
  health,
  newName,
  filePath,
  renameId,
  renameName,
  deleteImpact,
  loading,
  busy,
  error,
  notice,
  onRefresh,
  onCreateBase,
  onNewNameChange,
  onSelectBase,
  onStartRename,
  onRenameNameChange,
  onSaveRename,
  onCancelRename,
  onRequestDelete,
  onCancelDelete,
  onConfirmDelete,
  onImportDocument,
  onFilePathChange,
  onChooseFile,
}: KnowledgeViewProps) {
  const { t } = useLocale();
  const selectedBase = bases.find((base) => base.id === selectedBaseId) ?? null;

  return (
    <section className="bloomery-knowledge" aria-labelledby="knowledge-heading">
      <header className="bloomery-knowledge-header">
        <div><p className="bloomery-eyebrow">LOCAL KNOWLEDGE / STEEL DOMAIN</p><h1 id="knowledge-heading">{t("knowledgeTitle")}</h1><p className="bloomery-lede">{t("knowledgeLede")}</p></div>
        <button type="button" className="bloomery-icon-button" onClick={onRefresh} disabled={loading} aria-label={t("refreshKnowledge")} title={t("refreshKnowledge")}><RefreshCw size={17} aria-hidden="true" /></button>
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
          <form className="bloomery-knowledge-create" onSubmit={onCreateBase}>
            <label htmlFor="knowledge-base-name">{t("baseName")}</label>
            <div><input id="knowledge-base-name" value={newName} onChange={(event) => onNewNameChange(event.target.value)} placeholder={t("baseNamePlaceholder")} /><button type="submit" className="bloomery-icon-button" disabled={busy || !newName.trim()} aria-label={t("createKnowledgeBase")} title={t("createKnowledgeBase")}><Plus size={17} aria-hidden="true" /></button></div>
          </form>
          {loading ? <div className="bloomery-knowledge-loading"><LoaderCircle size={17} className="bloomery-spin" />{t("loading")}</div> : bases.length === 0 ? <p className="bloomery-knowledge-empty">{t("noKnowledgeBases")}</p> : (
            <div className="bloomery-knowledge-base-list">
              {bases.map((base) => <div key={base.id} className={`bloomery-knowledge-base ${selectedBaseId === base.id ? "is-active" : ""}`}>
                {renameId === base.id ? <div className="bloomery-knowledge-rename"><input aria-label={t("renameKnowledgeBase")} value={renameName} onChange={(event) => onRenameNameChange(event.target.value)} autoFocus /><button type="button" className="bloomery-icon-button" onClick={() => void onSaveRename()} disabled={busy} aria-label={t("saveName")}><Check size={15} /></button><button type="button" className="bloomery-icon-button" onClick={onCancelRename} aria-label={t("cancelRename")}><X size={15} /></button></div> : <><button type="button" className="bloomery-knowledge-base-select" onClick={() => onSelectBase(base.id)}>{base.name}</button><button type="button" className="bloomery-icon-button" onClick={() => onStartRename(base)} aria-label={`${t("rename")} ${base.name}`} title={t("rename")}><Pencil size={14} /></button><button type="button" className="bloomery-icon-button" onClick={() => void onRequestDelete(base)} aria-label={`${t("delete")} ${base.name}`} title={t("delete")}><Trash2 size={14} /></button></>}
              </div>)}
            </div>
          )}
        </aside>

        <div className="bloomery-knowledge-content">
          {selectedBase ? <>
            <div className="bloomery-knowledge-content-header"><div><p className="bloomery-eyebrow">SELECTED KNOWLEDGE BASE</p><h2>{selectedBase.name}</h2></div><span className="bloomery-knowledge-count">{t("documents", { count: documents.length })}</span></div>
            <form className="bloomery-knowledge-import" onSubmit={onImportDocument}>
              <div className="bloomery-knowledge-import-title"><FolderUp size={18} aria-hidden="true" /><strong>{t("importLocalDocument")}</strong></div>
              <label htmlFor="knowledge-file-path">{t("filePath")}</label>
              <div className="bloomery-knowledge-import-row"><input id="knowledge-file-path" value={filePath} onChange={(event) => onFilePathChange(event.target.value)} placeholder={t("filePathPlaceholder")} required /><button type="button" className="bloomery-icon-button" onClick={onChooseFile} disabled={busy} aria-label={t("browseFile")} title={t("browseFile")}><FolderOpen size={17} aria-hidden="true" /></button><button type="submit" className="bloomery-action-primary" disabled={busy || !filePath.trim()}><FileText size={17} aria-hidden="true" />{t("importDocumentAction")}</button></div>
              <p>{t("importDescription")}</p>
            </form>
            <section className="bloomery-knowledge-list-section" aria-labelledby="documents-heading">
              <div className="bloomery-knowledge-section-heading"><h3 id="documents-heading">{t("document")}</h3><span>{t("items", { count: documents.length })}</span></div>
              {documents.length === 0 ? <div className="bloomery-knowledge-empty-content"><FileText size={21} /><span>{t("noDocuments")}</span></div> : <div className="bloomery-knowledge-document-list">{documents.map((document) => <div className="bloomery-knowledge-document" key={document.id}><FileText size={18} /><div><strong>{document.display_name}</strong><span>{document.source_kind.toUpperCase()} / {documentStateLabel(document, (key) => t(key))}</span></div></div>)}</div>}
            </section>
          </> : <div className="bloomery-knowledge-empty-content is-large"><Database size={28} /><strong>{t("createKnowledgeBaseFirst")}</strong><span>{t("knowledgeBaseDescription")}</span></div>}

          <section className="bloomery-knowledge-list-section" aria-labelledby="tasks-heading">
            <div className="bloomery-knowledge-section-heading"><h3 id="tasks-heading">{t("backgroundTasks")}</h3><span>{t("items", { count: tasks.length })}</span></div>
            {tasks.length === 0 ? <div className="bloomery-knowledge-empty-content"><LoaderCircle size={19} /><span>{t("noBackgroundTasks")}</span></div> : <div className="bloomery-knowledge-task-list">{tasks.slice(0, 5).map((task) => <div className="bloomery-knowledge-task" key={task.id}><div><strong>{taskKindLabel(task.kind, (key) => t(key))}</strong><span>{taskStateLabel(task, (key) => t(key))} / {t("taskAttempt", { count: task.attempt })}</span></div><div className="bloomery-knowledge-progress"><span style={{ width: `${task.progress}%` }} /><b>{task.progress}%</b></div></div>)}</div>}
          </section>
        </div>
      </div>

      {deleteImpact && <div className="bloomery-knowledge-confirm" role="dialog" aria-modal="true" aria-labelledby="delete-heading"><div className="bloomery-knowledge-confirm-panel"><p className="bloomery-eyebrow">{t("destructiveAction")}</p><h2 id="delete-heading">{t("deleteKnowledgeBaseTitle", { name: deleteImpact.name })}</h2><p>{t("deleteKnowledgeBaseImpact", { documents: deleteImpact.document_count, versions: deleteImpact.version_count, chunks: deleteImpact.chunk_count })}</p>{deleteImpact.active_task_count > 0 && <strong className="bloomery-knowledge-danger">{t("activeTaskWarning")}</strong>}<div><button type="button" className="bloomery-action-secondary" onClick={onCancelDelete}>{t("cancel")}</button><button type="button" className="bloomery-action-primary" onClick={onConfirmDelete} disabled={busy || deleteImpact.active_task_count > 0}><Trash2 size={16} aria-hidden="true" />{t("confirmDelete")}</button></div></div></div>}
    </section>
  );
}
