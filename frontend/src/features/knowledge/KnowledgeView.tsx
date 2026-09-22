import { useEffect, useMemo, useState, type FormEvent } from "react";
import {
  BookOpen,
  Check,
  FileText,
  FolderOpen,
  PanelLeftClose,
  PanelLeftOpen,
  Pencil,
  Plus,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { useLocale } from "../../i18n/locale";
import type {
  BackgroundTask,
  DocumentVersionRecord,
  IndexHealthReport,
  KnowledgeBaseDeleteImpact,
  KnowledgeBaseRecord,
  KnowledgeHealth,
  SourceDocumentRecord,
} from "../../bridge/desktop";

export interface KnowledgeViewProps {
  bases: KnowledgeBaseRecord[];
  selectedBaseId: string | null;
  documents: SourceDocumentRecord[];
  selectedDocumentId: string | null;
  versions: DocumentVersionRecord[];
  tasks: BackgroundTask[];
  health: KnowledgeHealth;
  indexHealth: IndexHealthReport | null;
  newName: string;
  filePath: string;
  renameId: string | null;
  renameName: string;
  deleteImpact: KnowledgeBaseDeleteImpact | null;
  loading: boolean;
  busy: boolean;
  taskBusyId: string | null;
  error: string | null;
  notice: string | null;
  onClose?: () => void;
  onRefresh: () => void;
  onCreateBase: (event: FormEvent<HTMLFormElement>) => void;
  onNewNameChange: (value: string) => void;
  onSelectBase: (id: string) => void;
  onSelectDocument: (id: string) => void;
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
  onCancelTask: (task: BackgroundTask) => void;
  onRetryTask: (task: BackgroundTask) => void;
  onRebuildIndex: () => void;
}

type KnowledgeViewMode = "create" | "detail";
type KnowledgeStep = 1 | 2 | 3;

const steps: Array<{ id: KnowledgeStep; label: string }> = [
  { id: 1, label: "上传" },
  { id: 2, label: "创建设置" },
  { id: 3, label: "数据处理" },
];

function documentStateLabel(
  document: SourceDocumentRecord,
  translate: (key: "documentActive" | "documentProcessing") => string,
) {
  return document.active_version_id ? translate("documentActive") : translate("documentProcessing");
}

export default function KnowledgeView({
  bases,
  selectedBaseId,
  documents,
  selectedDocumentId,
  versions,
  tasks,
  health,
  indexHealth,
  newName,
  filePath,
  renameId,
  renameName,
  deleteImpact,
  loading,
  busy,
  taskBusyId,
  error,
  notice,
  onClose = () => undefined,
  onRefresh: _onRefresh,
  onCreateBase,
  onNewNameChange,
  onSelectBase,
  onSelectDocument,
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
  onCancelTask,
  onRetryTask,
  onRebuildIndex,
}: KnowledgeViewProps) {
  const { t } = useLocale();
  const [viewMode, setViewMode] = useState<KnowledgeViewMode>("create");
  const [step, setStep] = useState<KnowledgeStep>(1);
  const [knowledgeSidebarCollapsed, setKnowledgeSidebarCollapsed] = useState(() => (
    typeof window !== "undefined" && window.localStorage.getItem("knowledgeSidebarCollapsed") === "true"
  ));

  useEffect(() => {
    window.localStorage.setItem("knowledgeSidebarCollapsed", String(knowledgeSidebarCollapsed));
  }, [knowledgeSidebarCollapsed]);

  useEffect(() => {
    if (documents.length > 0) setViewMode("detail");
  }, [documents.length]);

  const selectedBase = bases.find((base) => base.id === selectedBaseId) ?? null;
  const indexStateLabel = (state: IndexHealthReport["state"]) => {
    switch (state) {
      case "healthy": return t("indexHealthy");
      case "degraded_flat": return t("indexDegraded");
      case "rebuild_required": return t("indexRebuildRequired");
      case "rebuilding": return t("indexRebuilding");
      case "failed": return t("indexFailed");
    }
  };

  const openDetail = (id: string) => {
    onSelectBase(id);
    setViewMode("detail");
  };

  const openCreate = () => {
    setViewMode("create");
    setStep(1);
  };

  return (
    <div
      data-testid="knowledge-web-workspace"
      aria-labelledby="knowledge-heading"
      className="fixed inset-0 z-50 flex flex-col bg-[#f7f3ed] text-slate-950"
    >
      <div className="flex h-16 shrink-0 items-center gap-3 border-b border-[#e5d8cc] bg-[#fffaf3]/95 px-6 shadow-[0_1px_0_rgba(72,52,38,0.04)] max-md:h-14 max-md:gap-2 max-md:px-3">
        <BookOpen className="h-6 w-6 shrink-0 text-[#b85f43] max-md:h-5 max-md:w-5" />
        <h2 id="knowledge-heading" className="whitespace-nowrap text-2xl font-bold tracking-tight text-[#201812] max-md:text-lg">
          知识库
        </h2>
        <div className="ml-4 flex items-center gap-2 rounded-xl border border-[#eadccf] bg-[#f9efe7] p-1 shadow-inner shadow-white/50 max-md:ml-1 max-md:gap-1 max-md:p-0.5">
          <button
            type="button"
            onClick={openCreate}
            className={`whitespace-nowrap rounded-lg px-4 py-2 text-base font-semibold transition-all max-md:px-2 max-md:py-1.5 max-md:text-xs ${
              viewMode === "create" ? "bg-[#c96f52] text-white shadow-sm" : "text-[#9c593f] hover:bg-[#f4dfd2]"
            }`}
          >
            新建知识库
          </button>
          <button
            type="button"
            disabled={bases.length < 2}
            title={bases.length < 2 ? "至少需要两个知识库" : "本地端暂未提供合并命令"}
            className="whitespace-nowrap rounded-lg px-4 py-2 text-base font-semibold text-[#9c593f] transition-all hover:bg-[#f4dfd2] disabled:cursor-not-allowed disabled:text-[#cdbdb0] max-md:px-2 max-md:py-1.5 max-md:text-xs"
          >
            合并知识库
          </button>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="ml-auto shrink-0 rounded-xl p-2 text-[#6f6258] transition-colors hover:bg-[#f1e6dc] hover:text-[#2f261f]"
          aria-label="关闭知识库"
          title="关闭"
        >
          <X className="h-5 w-5" />
        </button>
      </div>

      {error && (
        <div role="alert" className="mx-4 mt-3 flex shrink-0 items-center gap-2 rounded-xl border border-red-200 bg-red-50 px-4 py-2.5 text-sm text-red-700 max-md:mx-2">
          <span>{error}</span>
        </div>
      )}
      {notice && (
        <div role="status" className="mx-4 mt-3 flex shrink-0 items-center gap-2 rounded-xl border border-emerald-200 bg-emerald-50 px-4 py-2.5 text-sm text-emerald-700 max-md:mx-2">
          <Check className="h-4 w-4" aria-hidden="true" />
          <span>{notice}</span>
        </div>
      )}

      <div className="flex-1 overflow-hidden p-4 max-md:overflow-y-auto max-md:p-2">
        <div
          className={`grid h-full transition-[grid-template-columns] duration-300 ease-out max-md:!grid-cols-1 max-md:!grid-rows-[auto_auto] max-md:h-auto ${
            knowledgeSidebarCollapsed ? "grid-cols-[64px_minmax(0,1fr)]" : "grid-cols-[316px_minmax(0,1fr)]"
          } gap-4 max-md:gap-2`}
        >
          <aside className="relative min-h-0 overflow-visible rounded-2xl border border-[#e4d6c8] bg-[#fffaf3] shadow-sm max-md:max-h-[45dvh] max-md:min-h-[30dvh] max-md:overflow-hidden">
            <button
              type="button"
              onClick={() => setKnowledgeSidebarCollapsed((current) => !current)}
              className="group absolute right-3 top-2 z-20 flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-[#6f6258] transition-all duration-150 hover:bg-[#fffaf3] hover:text-[#2b2118] max-md:hidden"
              title={knowledgeSidebarCollapsed ? "打开边栏" : "关闭侧栏"}
              aria-label={knowledgeSidebarCollapsed ? "打开边栏" : "关闭侧栏"}
            >
              {knowledgeSidebarCollapsed ? <PanelLeftOpen size={20} aria-hidden="true" /> : <PanelLeftClose size={20} aria-hidden="true" />}
              {knowledgeSidebarCollapsed && (
                <span className="pointer-events-none absolute left-11 top-1/2 z-50 -translate-y-1/2 whitespace-nowrap rounded-md bg-slate-900 px-2 py-1 text-xs font-medium text-white opacity-0 shadow-lg transition-opacity duration-100 group-hover:opacity-100">
                  打开边栏
                </span>
              )}
            </button>
            <div className="absolute inset-0 overflow-hidden rounded-2xl max-md:static max-md:h-full">
              <div className="flex h-full w-[316px] flex-col max-md:w-full">
                <div className="flex h-14 shrink-0 items-center gap-2 border-b border-[#eee1d5] px-3 pr-12 max-md:h-11 max-md:pr-3">
                  <h3 className={`whitespace-nowrap text-base font-semibold text-[#241b15] transition-opacity duration-200 ease-out max-md:!opacity-100 ${knowledgeSidebarCollapsed ? "opacity-0" : "opacity-100"}`}>
                    已有知识库
                  </h3>
                  <span className={`whitespace-nowrap rounded-full bg-[#f1e6dc] px-2 py-0.5 text-xs font-semibold text-[#8b6b58] transition-opacity duration-200 ease-out max-md:!opacity-100 ${knowledgeSidebarCollapsed ? "opacity-0" : "opacity-100"}`}>
                    {bases.length} 个
                  </span>
                </div>
                <div
                  aria-busy={loading}
                  className={`h-[calc(100%-3.5rem)] overflow-auto p-3 transition-opacity duration-200 ease-out max-md:h-[calc(100%-2.75rem)] max-md:!pointer-events-auto max-md:!opacity-100 max-md:p-2 ${knowledgeSidebarCollapsed ? "pointer-events-none opacity-0" : "opacity-100"}`}
                >
                  {loading && bases.length === 0 ? (
                    <div role="status" className="space-y-2.5">
                      {Array.from({ length: 3 }).map((_, index) => (
                        <div key={index} className="h-16 animate-pulse rounded-xl border border-[#eadccf] bg-[#f1e6dc] motion-reduce:animate-none" />
                      ))}
                    </div>
                  ) : bases.length === 0 ? (
                    <div className="rounded-xl border border-dashed border-[#dfcfc0] bg-[#fffdf8] px-4 py-8 text-center text-sm text-[#9a8b7d]">
                      暂无知识库
                    </div>
                  ) : (
                    <div className="space-y-2.5">
                      {bases.map((base) => {
                        const active = viewMode === "detail" && selectedBaseId === base.id;
                        const count = selectedBaseId === base.id ? documents.length : null;
                        return (
                          <button
                            key={base.id}
                            type="button"
                            onClick={() => openDetail(base.id)}
                            aria-label={base.name}
                            className={`w-full rounded-xl border px-4 py-3 text-left transition-all ${
                              active
                                ? "border-[#c96f52] bg-[#fbede3] shadow-sm"
                                : "border-[#eadccf] bg-[#fffdf8] hover:border-[#d8bda9] hover:bg-[#fff7ef] hover:shadow-sm"
                            }`}
                          >
                            <div className="truncate text-base font-semibold text-[#241b15]">{base.name}</div>
                            <div className="mt-1 text-xs font-medium text-[#8b7b6e]">
                              {count === null ? "本地知识库" : `${count} 个文件`}
                            </div>
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </aside>

          {viewMode === "detail" && selectedBase ? (
            <section className="min-w-0 min-h-0 flex flex-col rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] p-4 shadow-sm max-md:min-h-[74dvh] max-md:p-2">
              <div className="mb-3 flex shrink-0 items-center justify-between">
                <div className="min-w-0">
                  <h3 className="truncate text-lg font-semibold text-[#241b15]">{selectedBase.name}</h3>
                  <div className="mt-1 text-xs font-medium text-[#8b7b6e]">{documents.length} 个文档</div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <button
                    type="button"
                    onClick={() => onStartRename(selectedBase)}
                    className="rounded-lg p-2 text-[#8a7665] transition-colors hover:bg-[#f1e6dc] hover:text-[#2f261f]"
                    title="重命名"
                    aria-label={`重命名 ${selectedBase.name}`}
                  >
                    <Pencil className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    onClick={() => void onRequestDelete(selectedBase)}
                    className="rounded-lg p-2 text-[#8a7665] transition-colors hover:bg-red-50 hover:text-red-600"
                    title="删除知识库"
                    aria-label={`删除 ${selectedBase.name}`}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>
              {renameId === selectedBase.id && (
                <div className="mb-3 flex items-center gap-2 rounded-xl border border-[#eadccf] bg-white p-3">
                  <input
                    aria-label={t("renameKnowledgeBase")}
                    value={renameName}
                    onChange={(event) => onRenameNameChange(event.target.value)}
                    autoFocus
                    className="h-10 min-w-0 flex-1 rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none focus:border-[#cc785c]"
                  />
                  <button type="button" onClick={onSaveRename} disabled={busy} className="rounded-lg bg-[#cc785c] px-3 py-2 text-sm font-semibold text-white">
                    保存
                  </button>
                  <button type="button" onClick={onCancelRename} className="rounded-lg bg-slate-100 px-3 py-2 text-sm font-semibold text-slate-700">
                    取消
                  </button>
                </div>
              )}
              <LocalKnowledgeBaseDetail
                documents={documents}
                selectedDocumentId={selectedDocumentId}
                versions={versions}
                onSelectDocument={onSelectDocument}
              />
            </section>
          ) : (
            <section className="min-w-0 min-h-0 flex flex-col">
              <div className="mb-4 flex shrink-0 items-center justify-between rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] px-6 py-4 shadow-sm max-md:px-3">
                {steps.map((item) => {
                  const done = item.id < step;
                  const active = item.id === step;
                  return (
                    <div key={item.id} className="flex items-center gap-2">
                      <div className={`flex h-8 w-8 items-center justify-center rounded-full text-base font-semibold shadow-sm ${active ? "bg-[#c96f52] text-white" : done ? "bg-[#f4dfd2] text-[#a65a41]" : "bg-[#f3eadf] text-[#8b7b6e]"}`}>
                        {done ? <Check className="h-4 w-4" /> : item.id}
                      </div>
                      <span className={`text-base font-semibold max-md:hidden ${active ? "text-[#b85f43]" : done ? "text-[#5d5046]" : "text-[#8b7b6e]"}`}>
                        {item.label}
                      </span>
                    </div>
                  );
                })}
              </div>

              <div className="mb-4 shrink-0 rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] px-5 py-4 shadow-sm">
                <label className="block text-base font-semibold text-[#241b15]" htmlFor="knowledge-name">
                  知识库名称
                </label>
                <input
                  id="knowledge-name"
                  value={newName}
                  onChange={(event) => onNewNameChange(event.target.value)}
                  placeholder="请输入知识库名称"
                  className="mt-2 h-11 w-full max-w-2xl rounded-xl border border-[#e2d2c4] bg-[#fffdf8] px-4 text-base text-[#241b15] outline-none transition-colors placeholder:text-[#a79a8d] focus:border-[#c96f52]"
                />
              </div>

              <div className="min-h-0 flex-1 overflow-auto">
                <div className="mx-auto max-w-[1670px] pb-24">
                  <form onSubmit={onCreateBase} className="mb-4 rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] p-5 shadow-sm">
                    <div className="mb-4 flex items-center gap-2">
                      <Upload className="h-5 w-5 text-[#b85f43]" />
                      <h3 className="text-lg font-semibold text-[#241b15]">上传本地文档</h3>
                    </div>
                    <div className="rounded-2xl border-2 border-dashed border-[#dfcfc0] bg-[#fffdf8] px-5 py-8 text-center transition-colors hover:border-[#c96f52] hover:bg-[#fff7ef]">
                      <Upload className="mx-auto h-8 w-8 text-[#c96f52]" />
                      <p className="mt-3 text-base font-semibold text-[#241b15]">选择本地文件，开始建立知识库</p>
                      <p className="mt-1 text-sm text-[#8b7b6e]">数据保留在 Bloomery 本地，由 Rust 运行时处理。</p>
                      <div className="mx-auto mt-5 flex max-w-3xl gap-2 max-md:flex-col">
                        <input
                          aria-label="文件路径"
                          value={filePath}
                          onChange={(event) => onFilePathChange(event.target.value)}
                          placeholder="输入完整的 Windows 文件路径"
                          className="h-11 min-w-0 flex-1 rounded-xl border border-[#e2d2c4] bg-white px-4 text-base text-[#241b15] outline-none placeholder:text-[#a79a8d] focus:border-[#c96f52]"
                        />
                        <button
                          type="button"
                          onClick={onChooseFile}
                          disabled={busy}
                          className="inline-flex h-11 items-center justify-center gap-2 rounded-xl border border-[#e2d2c4] bg-white px-4 text-sm font-semibold text-[#7f7064] transition-colors hover:border-[#c96f52] hover:text-[#2f261f] disabled:cursor-not-allowed disabled:opacity-60"
                          aria-label="选择文件"
                        >
                          <FolderOpen className="h-4 w-4" />
                          选择文件
                        </button>
                      </div>
                    </div>
                    <div className="mt-4 flex flex-wrap items-center justify-end gap-3">
                      <button
                        type="button"
                        onClick={() => setStep((current) => current > 1 ? (current - 1) as KnowledgeStep : current)}
                        disabled={step === 1}
                        className="rounded-xl bg-[#eee3d8] px-4 py-2.5 text-base font-medium text-[#5d5046] transition-colors hover:bg-[#e5d5c6] disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        上一步
                      </button>
                      <button
                        type="submit"
                        disabled={busy || !newName.trim()}
                        className="inline-flex items-center gap-2 rounded-xl bg-[#c96f52] px-5 py-2.5 text-base font-semibold text-white shadow-sm transition-colors hover:bg-[#a9583e] disabled:cursor-not-allowed disabled:bg-[#e6dfd8]"
                      >
                        <Plus className="h-4 w-4" />
                        创建知识库
                      </button>
                      <button
                        type="button"
                        onClick={() => setStep((current) => Math.min(3, current + 1) as KnowledgeStep)}
                        disabled={!filePath.trim()}
                        className="rounded-xl bg-[#c96f52] px-5 py-2.5 text-base font-semibold text-white shadow-sm transition-colors hover:bg-[#a9583e] disabled:cursor-not-allowed disabled:bg-[#e6dfd8]"
                      >
                        下一步
                      </button>
                    </div>
                  </form>

                  <form onSubmit={onImportDocument} className="rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] p-5 shadow-sm">
                    <div className="mb-3 flex items-center gap-2">
                      <FileText className="h-5 w-5 text-[#b85f43]" />
                      <strong className="text-base text-[#241b15]">导入到当前知识库</strong>
                    </div>
                    <p className="mb-4 text-sm leading-6 text-[#8b7b6e]">
                      {selectedBase ? `当前目标：${selectedBase.name}` : "请先创建或选择一个知识库。"}
                    </p>
                    <button
                      type="submit"
                      disabled={busy || !filePath.trim() || !selectedBaseId}
                      className="inline-flex items-center gap-2 rounded-xl bg-[#c96f52] px-5 py-2.5 text-base font-semibold text-white shadow-sm transition-colors hover:bg-[#a9583e] disabled:cursor-not-allowed disabled:bg-[#e6dfd8]"
                    >
                      <FileText className="h-4 w-4" />
                      导入文档
                    </button>
                  </form>

                </div>
              </div>
            </section>
          )}
        </div>
      </div>

      {deleteImpact && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-slate-950/35">
          <div className="w-[460px] rounded-lg bg-white p-6 shadow-2xl max-md:mx-4 max-md:w-auto">
            <div className="mb-4 flex items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold tracking-[0.12em] text-[#9c593f]">DESTRUCTIVE ACTION</p>
                <h3 className="mt-2 text-lg font-semibold text-slate-950">删除“{deleteImpact.name}”？</h3>
              </div>
              <button type="button" onClick={onCancelDelete} className="rounded-md p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900" aria-label="关闭">
                <X className="h-5 w-5" />
              </button>
            </div>
            <p className="text-sm leading-6 text-slate-600">
              将删除 {deleteImpact.document_count} 个文档、{deleteImpact.version_count} 个版本和 {deleteImpact.chunk_count} 个索引块。
            </p>
            {deleteImpact.active_task_count > 0 && <strong className="mt-3 block text-sm text-red-600">请先取消活动任务。</strong>}
            <div className="mt-5 flex justify-end gap-3">
              <button type="button" onClick={onCancelDelete} className="rounded-lg bg-slate-100 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-200">
                取消
              </button>
              <button
                type="button"
                onClick={onConfirmDelete}
                disabled={busy || deleteImpact.active_task_count > 0}
                className="inline-flex items-center gap-2 rounded-lg bg-red-500 px-4 py-2 text-sm font-semibold text-white hover:bg-red-600 disabled:cursor-not-allowed disabled:opacity-60"
              >
                <Trash2 className="h-4 w-4" />
                确认删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function LocalKnowledgeBaseDetail({
  documents,
  selectedDocumentId,
  versions,
  onSelectDocument,
}: {
  documents: SourceDocumentRecord[];
  selectedDocumentId: string | null;
  versions: DocumentVersionRecord[];
  onSelectDocument: (id: string) => void;
}) {
  const { t } = useLocale();
  const [searchText, setSearchText] = useState("");
  const filteredDocuments = useMemo(() => {
    const keyword = searchText.trim().toLowerCase();
    return keyword
      ? documents.filter((document) => document.display_name.toLowerCase().includes(keyword))
      : documents;
  }, [documents, searchText]);
  const currentDocument = documents.find((document) => document.id === selectedDocumentId) ?? documents[0] ?? null;

  return (
    <div className="relative min-h-0 flex-1 overflow-hidden rounded-2xl border border-[#e5d8cc] bg-[#fffdf8] shadow-sm">
      <div className="grid h-full min-h-[360px] grid-cols-[280px_minmax(0,1fr)] max-md:grid-cols-1 max-md:grid-rows-[auto_minmax(0,1fr)]">
        <aside className="min-h-0 border-r border-[#eadccf] bg-[#fffaf3] max-md:max-h-[36dvh] max-md:min-h-[24dvh] max-md:overflow-hidden max-md:border-b max-md:border-r-0">
          <div className="p-3 max-md:p-2">
            <input
              value={searchText}
              onChange={(event) => setSearchText(event.target.value)}
              placeholder="搜索"
              className="h-10 w-full rounded-xl border border-[#e2d2c4] bg-[#fffdf8] px-3 text-base text-[#241b15] outline-none placeholder:text-[#a79a8d] focus:border-[#c96f52] max-md:h-9 max-md:text-sm"
            />
          </div>
          <div className="px-4 pb-2 text-xs font-semibold text-[#8b7b6e] max-md:px-3 max-md:pb-1">文档列表</div>
          <div className="h-[calc(100%-5.5rem)] overflow-auto px-3 pb-3 max-md:h-auto max-md:max-h-[calc(36dvh-4.5rem)] max-md:px-2 max-md:pb-2">
            {documents.length === 0 ? (
              <div className="px-2 py-6 text-sm text-[#9a8b7d]">暂无文档</div>
            ) : filteredDocuments.length === 0 ? (
              <div className="px-2 py-6 text-sm text-[#9a8b7d]">没有匹配的文档</div>
            ) : (
              <div className="space-y-1">
                {filteredDocuments.map((document) => {
                  const active = document.id === currentDocument?.id;
                  return (
                    <button
                      key={document.id}
                      type="button"
                      onClick={() => onSelectDocument(document.id)}
                      className={`flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-base transition-colors ${
                        active ? "bg-[#fbede3] text-[#241b15]" : "text-[#5d5046] hover:bg-[#fff7ef]"
                      }`}
                    >
                      <FileText className="h-4 w-4 shrink-0 text-[#8b6b58]" />
                      <span className="truncate">{document.display_name}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </aside>

        <section className="min-w-0 min-h-0 flex flex-col bg-[#f7f3ed]">
          <div className="relative flex h-14 shrink-0 items-center justify-between gap-4 border-b border-[#eadccf] bg-[#fffdf8] px-4">
            <div className="flex min-w-0 items-center gap-2 text-base text-[#241b15]">
              <FileText className="h-4 w-4 shrink-0 text-[#7b604f]" />
              <span className="truncate font-medium">{currentDocument?.display_name || "选择一个文档"}</span>
            </div>
            <span className="shrink-0 text-xs text-[#8b7b6e]">本地解析</span>
          </div>
          {!currentDocument ? (
            <div className="flex flex-1 items-center justify-center px-6 text-center text-sm text-[#8f8174]">
              选择文档后，在这里查看本地解析状态和版本信息。
            </div>
          ) : (
            <div className="min-h-0 flex-1 overflow-auto bg-[#f7f3ed] p-3 max-md:p-2">
              <div className="min-h-full rounded-xl border border-[#eadccf] bg-[#fffdf8] px-5 py-5 text-base leading-7 text-[#241b15] shadow-sm">
                <div className="flex items-start justify-between gap-4 border-b border-[#eadccf] pb-4">
                  <div className="min-w-0">
                    <h4 className="truncate text-lg font-semibold text-[#241b15]">{currentDocument.display_name}</h4>
                    <p className="mt-1 text-sm text-[#8f8174]">
                      {currentDocument.source_kind.toUpperCase()} / {documentStateLabel(currentDocument, (key) => t(key))}
                    </p>
                  </div>
                  <FileText className="h-6 w-6 shrink-0 text-[#c96f52]" />
                </div>
                <div className="mt-5">
                  <h5 className="text-base font-semibold text-[#241b15]">本地解析结果</h5>
                  <p className="mt-2 text-sm leading-6 text-[#8f8174]">
                    Bloomery 当前通过本地 Rust 运行时管理文档解析和索引。原始 PDF 与 Markdown 预览接口尚未接入桌面 bridge，文档内容不会回调 Web 服务。
                  </p>
                </div>
                <div className="mt-5 grid gap-3 sm:grid-cols-2">
                  <div className="rounded-xl border border-[#eadccf] bg-white px-4 py-3">
                    <span className="text-xs font-semibold text-[#8b7b6e]">文档状态</span>
                    <strong className="mt-1 block text-sm text-[#241b15]">{documentStateLabel(currentDocument, (key) => t(key))}</strong>
                  </div>
                  <div className="rounded-xl border border-[#eadccf] bg-white px-4 py-3">
                    <span className="text-xs font-semibold text-[#8b7b6e]">当前版本</span>
                    <strong className="mt-1 block text-sm text-[#241b15]">{versions.length ? `${versions.length} 个版本` : "暂无版本"}</strong>
                  </div>
                </div>
                {versions.length > 0 && (
                  <div className="mt-5">
                    <h5 className="text-base font-semibold text-[#241b15]">版本记录</h5>
                    <div className="mt-2 space-y-2">
                      {versions.map((version) => (
                        <div key={version.id} className="flex items-center justify-between gap-3 rounded-xl border border-[#eadccf] bg-white px-4 py-3">
                          <div className="min-w-0">
                            <strong className="block truncate text-sm text-[#241b15]">{version.embedding_model_id}</strong>
                            <span className="mt-1 block text-xs text-[#8b7b6e]">
                              {version.parser} {version.parser_version} / {version.chunk_policy_version}
                            </span>
                          </div>
                          <span className="shrink-0 text-xs text-[#8b7b6e]">
                            {version.expected_chunk_count} {t("indexedChunks").toLowerCase()}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
