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

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "本地操作失败，请检查配置后重试。";
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

function taskStateLabel(task: BackgroundTask) {
  switch (task.state) {
    case "queued":
      return "排队中";
    case "running":
    case "waiting_external":
      return "处理中";
    case "completed":
      return "已完成";
    case "failed":
      return "失败";
    case "cancelled":
      return "已取消";
    case "interrupted":
      return "已中断";
    default:
      return "已暂停";
  }
}

function taskKindLabel(kind: string) {
  return kind === "mineru_parse" ? "文献解析" : kind === "rag_index_rebuild" ? "索引重建" : "后台任务";
}

function documentStateLabel(document: SourceDocumentRecord) {
  return document.active_version_id ? "已激活" : "处理中";
}

export default function KnowledgePage() {
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
      setError(errorMessage(cause));
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
      if (mounted) setError(errorMessage(cause));
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
      setError(errorMessage(cause));
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
      setError(errorMessage(cause));
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
      setError(errorMessage(cause));
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
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const importDocument = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedBaseId || !filePath.trim()) return;
    if (!retrieval.embeddingProfileId || !retrieval.mineruProfileId) {
      setError("导入文档前需要先配置 SiliconFlow Embedding 和 MinerU。请到设置中完成配置。");
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
      setNotice("导入任务已创建");
      await loadOverview();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="bloomery-knowledge" aria-labelledby="knowledge-heading">
      <header className="bloomery-knowledge-header">
        <div>
          <p className="bloomery-eyebrow">LOCAL KNOWLEDGE / STEEL DOMAIN</p>
          <h1 id="knowledge-heading">知识库</h1>
          <p className="bloomery-lede">管理标准、论文和工艺资料，让每一次检索都能回到原始文档。</p>
        </div>
        <button type="button" className="bloomery-icon-button" onClick={() => void loadOverview()} disabled={loading} aria-label="刷新知识库" title="刷新知识库">
          <RefreshCw size={17} aria-hidden="true" />
        </button>
      </header>

      {error && <div className="bloomery-knowledge-alert" role="alert"><AlertCircle size={17} aria-hidden="true" /><span>{error}</span></div>}
      {notice && <div className="bloomery-knowledge-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}

      <div className="bloomery-knowledge-metrics" aria-label="知识库统计">
        <div><span>知识库</span><strong>{health.knowledge_base_count}</strong></div>
        <div><span>文档</span><strong>{health.document_count}</strong></div>
        <div><span>已激活</span><strong>{health.active_document_count}</strong></div>
        <div><span>索引块</span><strong>{health.indexed_chunk_count} / {health.chunk_count}</strong></div>
        <div><span>活动任务</span><strong>{health.active_task_count}</strong></div>
      </div>

      <div className="bloomery-knowledge-layout">
        <aside className="bloomery-knowledge-sidebar" aria-label="知识库列表">
          <div className="bloomery-knowledge-section-heading"><span>知识库目录</span><Database size={16} aria-hidden="true" /></div>
          <form className="bloomery-knowledge-create" onSubmit={createBase}>
            <label htmlFor="knowledge-base-name">知识库名称</label>
            <div>
              <input id="knowledge-base-name" value={newName} onChange={(event) => setNewName(event.target.value)} placeholder="例如：钢铁标准" />
              <button type="submit" className="bloomery-icon-button" disabled={busy || !newName.trim()} aria-label="创建知识库" title="创建知识库"><Plus size={17} aria-hidden="true" /></button>
            </div>
          </form>
          {loading ? <div className="bloomery-knowledge-loading"><LoaderCircle size={17} className="bloomery-spin" />正在读取</div> : bases.length === 0 ? (
            <p className="bloomery-knowledge-empty">还没有知识库</p>
          ) : (
            <div className="bloomery-knowledge-base-list">
              {bases.map((base) => (
                <div key={base.id} className={`bloomery-knowledge-base ${selectedBaseId === base.id ? "is-active" : ""}`}>
                  {renameId === base.id ? (
                    <div className="bloomery-knowledge-rename">
                      <input aria-label="重命名知识库" value={renameName} onChange={(event) => setRenameName(event.target.value)} autoFocus />
                      <button type="button" className="bloomery-icon-button" onClick={() => void saveRename()} disabled={busy} aria-label="保存名称"><Check size={15} /></button>
                      <button type="button" className="bloomery-icon-button" onClick={() => setRenameId(null)} aria-label="取消重命名"><X size={15} /></button>
                    </div>
                  ) : (
                    <>
                      <button type="button" className="bloomery-knowledge-base-select" onClick={() => setSelectedBaseId(base.id)}>{base.name}</button>
                      <button type="button" className="bloomery-icon-button" onClick={() => startRename(base)} aria-label={`重命名 ${base.name}`} title="重命名"><Pencil size={14} /></button>
                      <button type="button" className="bloomery-icon-button" onClick={() => void requestDelete(base)} aria-label={`删除 ${base.name}`} title="删除"><Trash2 size={14} /></button>
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
                <span className="bloomery-knowledge-count">{documents.length} 份文档</span>
              </div>
              <form className="bloomery-knowledge-import" onSubmit={importDocument}>
                <div className="bloomery-knowledge-import-title"><FolderUp size={18} aria-hidden="true" /><strong>导入本地文档</strong></div>
                <label htmlFor="knowledge-file-path">文件路径</label>
                <div className="bloomery-knowledge-import-row">
                  <input id="knowledge-file-path" value={filePath} onChange={(event) => setFilePath(event.target.value)} placeholder="输入 Windows 文件完整路径" required />
                  <button type="submit" className="bloomery-action-primary" disabled={busy || !filePath.trim()}><FileText size={17} aria-hidden="true" />导入文档</button>
                </div>
                <p>使用当前配置的 MinerU 解析文档，并用 BGE-M3 建立本地索引。</p>
              </form>

              <section className="bloomery-knowledge-list-section" aria-labelledby="documents-heading">
                <div className="bloomery-knowledge-section-heading"><h3 id="documents-heading">文档</h3><span>{documents.length} 项</span></div>
                {documents.length === 0 ? <div className="bloomery-knowledge-empty-content"><FileText size={21} /><span>这个知识库还没有文档</span></div> : (
                  <div className="bloomery-knowledge-document-list">
                    {documents.map((document) => <div className="bloomery-knowledge-document" key={document.id}><FileText size={18} /><div><strong>{document.display_name}</strong><span>{document.source_kind.toUpperCase()} · {documentStateLabel(document)}</span></div></div>)}
                  </div>
                )}
              </section>
            </>
          ) : (
            <div className="bloomery-knowledge-empty-content is-large"><Database size={28} /><strong>先创建一个知识库</strong><span>知识库会保存文档、版本和可追溯的检索证据。</span></div>
          )}

          <section className="bloomery-knowledge-list-section" aria-labelledby="tasks-heading">
            <div className="bloomery-knowledge-section-heading"><h3 id="tasks-heading">后台任务</h3><span>{tasks.length} 项</span></div>
            {tasks.length === 0 ? <div className="bloomery-knowledge-empty-content"><LoaderCircle size={19} /><span>暂无后台任务</span></div> : (
              <div className="bloomery-knowledge-task-list">
                {tasks.slice(0, 5).map((task) => <div className="bloomery-knowledge-task" key={task.id}><div><strong>{taskKindLabel(task.kind)}</strong><span>{taskStateLabel(task)} · 第 {task.attempt} 次</span></div><div className="bloomery-knowledge-progress"><span style={{ width: `${task.progress}%` }} /><b>{task.progress}%</b></div></div>)}
              </div>
            )}
          </section>
        </div>
      </div>

      {deleteImpact && <div className="bloomery-knowledge-confirm" role="dialog" aria-modal="true" aria-labelledby="delete-heading"><div className="bloomery-knowledge-confirm-panel"><p className="bloomery-eyebrow">DESTRUCTIVE ACTION</p><h2 id="delete-heading">删除“{deleteImpact.name}”？</h2><p>这会删除 {deleteImpact.document_count} 份文档、{deleteImpact.version_count} 个版本和 {deleteImpact.chunk_count} 个索引块。</p>{deleteImpact.active_task_count > 0 && <strong className="bloomery-knowledge-danger">请先取消活动任务。</strong>}<div><button type="button" className="bloomery-action-secondary" onClick={() => setDeleteImpact(null)}>取消</button><button type="button" className="bloomery-action-primary" onClick={() => void confirmDelete()} disabled={busy || deleteImpact.active_task_count > 0}><Trash2 size={16} aria-hidden="true" />确认删除</button></div></div></div>}
    </section>
  );
}
