import { useEffect, useRef, useState } from "react";
import {
  BookOpen,
  Check,
  FileText,
  Loader2,
  Pencil,
  RefreshCw,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import KnowledgeBaseDetail from "./KnowledgeBaseDetail";
import type { LiteratureFileInfo, LiteratureFilePreview } from "../services/literature";
import type { MinerUProcessingConfigInfo } from "../types/rag";

// 与钢铁智能体 / 多模态智能检索侧栏保持一致的折叠图标
const SidebarPanelIcon = ({ size = 20, className = "" }: { size?: number; className?: string }) => (
  <svg
    aria-hidden="true"
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.8"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
  >
    <rect x="4.5" y="5.5" width="15" height="13" rx="3" />
    <path d="M10.5 5.5v13" />
  </svg>
);

type KnowledgeStep = 1 | 2 | 3;
type ParseMode = "precise" | "fast";
type SegmentMode = "auto" | "custom" | "hierarchy";
type MergeMode = "new" | "existing";
type KnowledgeViewMode = "create" | "detail";

export type LiteratureFolder = {
  name: string;
  pdf_count: number;
};

export type LiteratureJob = {
  job_id: string;
  folder: string;
  status: string;
  progress: string;
  error?: string;
  paper_count?: number;
  pdf_count?: number;
  filenames?: string[];
  created_at?: string;
  duration?: string;
  elapsed_seconds?: number;
  duration_seconds?: number;
  current_page?: number;
  total_pages?: number;
  progress_percent?: number;
  eta_seconds?: number | null;
  estimate_method?: string;
};

export type KnowledgeUploadFile = {
  id: string;
  name: string;
  storageName?: string;
  size: number;
  progress: number;
  status: "uploading" | "done" | "failed";
  error?: string;
};

export const KNOWLEDGE_MAX_PDF_BYTES = 100 * 1024 * 1024;

const SUPPORTED_UPLOAD_EXTENSIONS: string[] = [".pdf", ".png", ".jpg", ".jpeg", ".docx", ".pptx", ".xlsx"];

type KnowledgeBaseWizardProps = {
  viewMode: KnowledgeViewMode;
  openCreateView: () => void;
  openAppendView: (folder: string) => void;
  openDetailView: (folder: string) => void;
  step: KnowledgeStep;
  setStep: (step: KnowledgeStep) => void;
  knowledgeName: string;
  setKnowledgeName: (value: string) => void;
  knowledgeFolder: string;
  setKnowledgeFolder: (value: string) => void;
  uploadedFiles: KnowledgeUploadFile[];
  uploadBusy: boolean;
  folders: LiteratureFolder[];
  foldersLoading: boolean;
  foldersLoaded: boolean;
  onRetryFolders: () => void;
  literatureFiles: LiteratureFileInfo[];
  literatureFilesLoading: boolean;
  selectedLiteratureFile: string;
  setSelectedLiteratureFile: (filename: string) => void;
  literatureFilePreview: LiteratureFilePreview | null;
  literaturePreviewLoading: boolean;
  jobs: LiteratureJob[];
  canContinueUpload: boolean;
  litLoading: string | null;
  expandedJobId: string | null;
  expandedJobLogs: Record<string, string[]>;
  parseMode: ParseMode;
  setParseMode: (value: ParseMode) => void;
  extractImages: boolean;
  setExtractImages: (value: boolean) => void;
  extractOcr: boolean;
  setExtractOcr: (value: boolean) => void;
  extractTables: boolean;
  setExtractTables: (value: boolean) => void;
  enableFormula: boolean;
  setEnableFormula: (value: boolean) => void;
  pageRanges: string;
  setPageRanges: (value: string) => void;
  filterText: string;
  setFilterText: (value: string) => void;
  segmentMode: SegmentMode;
  setSegmentMode: (value: SegmentMode) => void;
  maxChunkSize: number;
  setMaxChunkSize: (value: number) => void;
  minChunkSize: number;
  setMinChunkSize: (value: number) => void;
  chunkOverlap: number;
  setChunkOverlap: (value: number) => void;
  onClose: () => void;
  onUploadFiles: (files: FileList | File[]) => void | Promise<void>;
  onRenameFile: (filename: string, newFilename: string) => void | Promise<void>;
  onDeleteFile: (filename: string) => void | Promise<void>;
  onDeleteFolder: (folder: string) => void | Promise<void>;
  onMergeFolder: (source: string, target: string, options?: { mode?: MergeMode; destination?: string }) => void | Promise<void>;
  onConfirmProcessing: () => void | Promise<void>;
  onDeleteJob: (jobId: string) => void | Promise<void>;
  setExpandedJobId: (jobId: string | null) => void;
  mineruProcessingConfig?: MinerUProcessingConfigInfo;
};

const steps: Array<{ id: KnowledgeStep; label: string }> = [
  { id: 1, label: "上传" },
  { id: 2, label: "创建设置" },
  { id: 3, label: "数据处理" },
];

export default function KnowledgeBaseWizard(props: KnowledgeBaseWizardProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [dragging, setDragging] = useState(false);
  const [editingFile, setEditingFile] = useState<KnowledgeUploadFile | null>(null);
  const [editingName, setEditingName] = useState("");
  const [mergingFolder, setMergingFolder] = useState<LiteratureFolder | null>(null);
  const [mergeMode, setMergeMode] = useState<MergeMode>("new");
  const [mergeTarget, setMergeTarget] = useState("");
  const [mergeDestination, setMergeDestination] = useState("");
  const [knowledgeSidebarCollapsed, setKnowledgeSidebarCollapsed] = useState(() => {
    return window.localStorage.getItem("knowledgeSidebarCollapsed") === "true";
  });
  const selectedFolderName = props.knowledgeFolder || props.knowledgeName.trim();
  const canEditKnowledgeName = props.step === 1 && props.uploadedFiles.length === 0 && !props.uploadBusy && !props.knowledgeFolder;

  useEffect(() => {
    window.localStorage.setItem("knowledgeSidebarCollapsed", String(knowledgeSidebarCollapsed));
  }, [knowledgeSidebarCollapsed]);

  const handleFiles = (files: FileList | null) => {
    if (!files || files.length === 0) return;
    void props.onUploadFiles(files);
  };

  const goNext = () => {
    if (props.step === 1) {
      if (!props.canContinueUpload) return;
      props.setStep(2);
      return;
    }
    if (props.step === 2) {
      void props.onConfirmProcessing();
    }
  };

  const goPrevious = () => {
    if (props.step > 1) props.setStep((props.step - 1) as KnowledgeStep);
  };

  const startAppendToCurrentFolder = () => {
    if (!selectedFolderName) return;
    props.openAppendView(selectedFolderName);
  };

  const finishProcessingView = () => {
    if (selectedFolderName) {
      props.openDetailView(selectedFolderName);
      return;
    }
    props.openCreateView();
  };

  const openFileEditor = (file: KnowledgeUploadFile) => {
    setEditingFile(file);
    setEditingName(stripDocumentExtension(file.name));
  };

  const closeFileEditor = () => {
    setEditingFile(null);
    setEditingName("");
  };

  const confirmFileEditor = () => {
    if (!editingFile) return;
    const nextName = editingName.trim();
    if (!nextName) return;
    void props.onRenameFile(editingFile.storageName || editingFile.name, ensureDocumentExtension(nextName, editingFile.name));
    closeFileEditor();
  };

  const openMergeDialog = () => {
    const firstSource = props.folders[0] || null;
    const firstTarget = props.folders.find(item => item.name !== firstSource?.name)?.name || "";
    setMergingFolder(firstSource);
    setMergeMode("new");
    setMergeTarget(firstTarget);
    setMergeDestination(firstSource ? `${firstSource.name} 合并库` : "");
  };

  const closeMergeDialog = () => {
    setMergingFolder(null);
    setMergeMode("new");
    setMergeTarget("");
    setMergeDestination("");
  };

  const confirmMergeDialog = () => {
    if (!mergingFolder || !mergeTarget) return;
    const destination = mergeDestination.trim();
    if (mergeMode === "new" && !destination) return;
    void props.onMergeFolder(mergingFolder.name, mergeTarget, {
      mode: mergeMode,
      destination: mergeMode === "new" ? destination : "",
    });
    closeMergeDialog();
  };

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-[#f7f3ed] text-slate-950">
      <div className="flex h-16 shrink-0 items-center gap-3 border-b border-[#e5d8cc] bg-[#fffaf3]/95 px-6 shadow-[0_1px_0_rgba(72,52,38,0.04)] max-md:h-14 max-md:gap-2 max-md:px-3">
        <BookOpen className="h-6 w-6 shrink-0 text-[#b85f43] max-md:h-5 max-md:w-5" />
        <h2 className="whitespace-nowrap text-2xl font-bold tracking-tight text-[#201812] max-md:text-lg">知识库</h2>
        <div className="ml-4 flex items-center gap-2 rounded-xl border border-[#eadccf] bg-[#f9efe7] p-1 shadow-inner shadow-white/50 max-md:ml-1 max-md:gap-1 max-md:p-0.5">
          <button
            type="button"
            onClick={props.openCreateView}
            className={`whitespace-nowrap rounded-lg px-4 py-2 text-base font-semibold transition-all max-md:px-2 max-md:py-1.5 max-md:text-xs ${
              props.viewMode === "create" ? "bg-[#c96f52] text-white shadow-sm" : "text-[#9c593f] hover:bg-[#f4dfd2]"
            }`}
          >
            新建知识库
          </button>
          <button
            type="button"
            onClick={openMergeDialog}
            disabled={!props.foldersLoaded || props.folders.length < 2}
            className="whitespace-nowrap rounded-lg px-4 py-2 text-base font-semibold text-[#9c593f] transition-all hover:bg-[#f4dfd2] disabled:cursor-not-allowed disabled:text-[#cdbdb0] max-md:px-2 max-md:py-1.5 max-md:text-xs"
          >
            合并知识库
          </button>
        </div>
        <button
          onClick={props.onClose}
          className="ml-auto shrink-0 rounded-xl p-2 text-[#6f6258] transition-colors hover:bg-[#f1e6dc] hover:text-[#2f261f]"
          aria-label="关闭知识库"
          title="关闭"
        >
          <X className="h-5 w-5" />
        </button>
      </div>

      <div className="flex-1 overflow-hidden p-4 max-md:overflow-y-auto max-md:p-2">
        <div className={`grid h-full transition-[grid-template-columns] duration-300 ease-out max-md:!grid-cols-1 max-md:!grid-rows-[auto_auto] max-md:h-auto ${
          knowledgeSidebarCollapsed ? "grid-cols-[64px_minmax(0,1fr)]" : "grid-cols-[316px_minmax(0,1fr)]"
        } gap-4 max-md:gap-2`}>
          <aside className="relative min-h-0 overflow-visible rounded-2xl border border-[#e4d6c8] bg-[#fffaf3] shadow-sm max-md:max-h-[45dvh] max-md:min-h-[30dvh] max-md:overflow-hidden">
            <button
              type="button"
              onClick={() => setKnowledgeSidebarCollapsed(prev => !prev)}
              className="group absolute right-3 top-2 z-20 flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-[#6f6258] transition-all duration-150 hover:bg-[#fffaf3] hover:text-[#2b2118] max-md:hidden"
              title={knowledgeSidebarCollapsed ? "打开边栏" : "关闭侧栏"}
            >
              <SidebarPanelIcon size={20} />
              {knowledgeSidebarCollapsed && (
                <span className="pointer-events-none absolute left-11 top-1/2 z-50 -translate-y-1/2 whitespace-nowrap rounded-md bg-slate-900 px-2 py-1 text-xs font-medium text-white opacity-0 shadow-lg transition-opacity duration-100 group-hover:opacity-100">
                  打开边栏
                </span>
              )}
            </button>
            <div className="absolute inset-0 overflow-hidden rounded-2xl max-md:static max-md:h-full">
            <div className="flex h-full w-[316px] flex-col max-md:w-full">
              <div className="flex h-14 shrink-0 items-center gap-2 border-b border-[#eee1d5] px-3 pr-12 max-md:h-11 max-md:pr-3">
                <h3 className={`whitespace-nowrap text-base font-semibold text-[#241b15] transition-opacity duration-200 ease-out max-md:!opacity-100 ${knowledgeSidebarCollapsed ? "opacity-0" : "opacity-100"}`}>已有知识库</h3>
                {!props.foldersLoaded ? (
                  <span aria-hidden="true" className={`h-5 w-10 animate-pulse rounded-full bg-[#f1e6dc] transition-opacity duration-200 ease-out motion-reduce:animate-none max-md:!opacity-100 ${knowledgeSidebarCollapsed ? "opacity-0" : "opacity-100"}`} />
                ) : (
                  <span className={`whitespace-nowrap rounded-full bg-[#f1e6dc] px-2 py-0.5 text-xs font-semibold text-[#8b6b58] transition-opacity duration-200 ease-out max-md:!opacity-100 ${knowledgeSidebarCollapsed ? "opacity-0" : "opacity-100"}`}>{props.folders.length} 个</span>
                )}
              </div>
              {!props.foldersLoaded && props.foldersLoading && props.folders.length === 0 && (
                <div role="status" className="sr-only">正在加载知识库</div>
              )}
              <div aria-busy={props.foldersLoading} className={`h-[calc(100%-3.5rem)] overflow-auto p-3 transition-opacity duration-200 ease-out max-md:h-[calc(100%-2.75rem)] max-md:!pointer-events-auto max-md:!opacity-100 max-md:p-2 ${knowledgeSidebarCollapsed ? "pointer-events-none opacity-0" : "opacity-100"}`}>
                {!props.foldersLoaded && props.foldersLoading && props.folders.length === 0 ? (
                  <div aria-hidden="true" className="space-y-2.5">
                    {Array.from({ length: 3 }).map((_, index) => (
                      <div key={index} className="h-16 animate-pulse rounded-xl border border-[#eadccf] bg-[#f1e6dc] motion-reduce:animate-none" />
                    ))}
                  </div>
                ) : !props.foldersLoaded && !props.foldersLoading && props.folders.length === 0 ? (
                  <div role="status" className="rounded-xl border border-dashed border-[#dfcfc0] bg-[#fffdf8] px-4 py-8 text-center text-sm text-[#9a8b7d]">
                    <div>知识库加载失败</div>
                    <button
                      type="button"
                      onClick={props.onRetryFolders}
                      className="mx-auto mt-3 inline-flex items-center gap-1.5 rounded-lg px-3 py-2 font-semibold text-[#9c593f] transition-colors hover:bg-[#f4dfd2]"
                    >
                      <RefreshCw aria-hidden="true" className="h-4 w-4" />
                      重新加载
                    </button>
                  </div>
                ) : props.foldersLoaded && props.folders.length === 0 ? (
                  <div className="rounded-xl border border-dashed border-[#dfcfc0] bg-[#fffdf8] px-4 py-8 text-center text-sm text-[#9a8b7d]">
                    暂无知识库
                  </div>
                ) : (
                  <div className="space-y-2.5">
                    {props.folders.map(folder => {
                      const active = selectedFolderName === folder.name;
                      return (
                        <div key={folder.name} className="relative">
                          <button
                            type="button"
                            onClick={() => props.openDetailView(folder.name)}
                            className={`w-full rounded-xl border px-4 py-3 text-left transition-all ${
                              active ? "border-[#c96f52] bg-[#fbede3] shadow-sm" : "border-[#eadccf] bg-[#fffdf8] hover:border-[#d8bda9] hover:bg-[#fff7ef] hover:shadow-sm"
                            }`}
                          >
                            <div className="truncate text-base font-semibold text-[#241b15]">{folder.name}</div>
                            <div className="mt-1 text-xs font-medium text-[#8b7b6e]">{folder.pdf_count} 个文件</div>
                          </button>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
            </div>
          </aside>

          {props.viewMode === "detail" ? (
            <section className="min-w-0 min-h-0 flex flex-col rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] p-4 shadow-sm max-md:min-h-[74dvh] max-md:p-2">
              <div className="mb-3 flex shrink-0 items-center justify-between">
                <div>
                  <h3 className="text-lg font-semibold text-[#241b15]">{selectedFolderName}</h3>
                  <div className="mt-1 text-xs font-medium text-[#8b7b6e]">{props.literatureFiles.length} 个文档</div>
                </div>
                <button
                  type="button"
                  onClick={() => void props.onDeleteFolder(selectedFolderName)}
                  className="rounded-lg p-2 text-[#8a7665] transition-colors hover:bg-red-50 hover:text-red-600"
                  title="删除知识库"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
              <KnowledgeBaseDetail
                folderName={selectedFolderName}
                files={props.literatureFiles}
                loading={props.literatureFilesLoading}
                selectedFile={props.selectedLiteratureFile}
                setSelectedFile={props.setSelectedLiteratureFile}
                preview={props.literatureFilePreview}
                previewLoading={props.literaturePreviewLoading}
                onRenameFile={props.onRenameFile}
                onDeleteFile={props.onDeleteFile}
              />
            </section>
          ) : (
          <section className="min-w-0 min-h-0 flex flex-col">
            <div className="mb-4 flex shrink-0 items-center justify-between rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] px-6 py-4 shadow-sm">
              {steps.map(item => {
                const done = item.id < props.step;
                const active = item.id === props.step;
                return (
                  <div key={item.id} className="flex items-center gap-2">
                    <div
                      className={`flex h-8 w-8 items-center justify-center rounded-full text-base font-semibold shadow-sm ${
                        active
                          ? "bg-[#c96f52] text-white"
                          : done
                            ? "bg-[#f4dfd2] text-[#a65a41]"
                            : "bg-[#f3eadf] text-[#8b7b6e]"
                      }`}
                    >
                      {done ? <Check className="w-4 h-4" /> : item.id}
                    </div>
                    <span className={`text-base font-semibold ${active ? "text-[#b85f43]" : done ? "text-[#5d5046]" : "text-[#8b7b6e]"}`}>
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
                value={props.knowledgeName}
                disabled={!canEditKnowledgeName}
                onChange={event => {
                  props.setKnowledgeName(event.target.value);
                  props.setKnowledgeFolder("");
                }}
                placeholder="请输入知识库名称"
                className="mt-2 h-11 w-full max-w-2xl rounded-xl border border-[#e2d2c4] bg-[#fffdf8] px-4 text-base text-[#241b15] outline-none transition-colors placeholder:text-[#a79a8d] focus:border-[#c96f52] disabled:bg-[#f4eee6] disabled:text-[#8f8174]"
              />
            </div>

            <div className="min-h-0 flex-1 overflow-hidden">
              {props.step === 1 && (
                <div className="h-full overflow-auto">
                  <div
                    role="button"
                    tabIndex={0}
                    onClick={() => inputRef.current?.click()}
                    onKeyDown={event => {
                      if (event.key === "Enter" || event.key === " ") inputRef.current?.click();
                    }}
                    onDragOver={event => {
                      event.preventDefault();
                      setDragging(true);
                    }}
                    onDragLeave={() => setDragging(false)}
                    onDrop={event => {
                      event.preventDefault();
                      setDragging(false);
                      handleFiles(event.dataTransfer.files);
                    }}
                    className={`flex min-h-[260px] cursor-pointer flex-col items-center justify-center rounded-2xl border border-dashed bg-[#fffaf3] px-8 text-center shadow-sm transition-all hover:border-[#c96f52] hover:bg-[#fff5ed] hover:shadow-md ${
                      dragging ? "border-[#c96f52] bg-[#fff0e6] shadow-md" : "border-[#dcc9b9]"
                    }`}
                  >
                    <input
                      ref={inputRef}
                      type="file"
                      accept=".pdf,.png,.jpg,.jpeg,.docx,.pptx,.xlsx,application/pdf"
                      multiple
                      className="hidden"
                      onChange={event => handleFiles(event.target.files)}
                    />
                    <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-[#f4dfd2] text-[#b85f43]">
                      <Upload className="h-6 w-6" />
                    </div>
                    <div className="text-base font-semibold text-[#241b15]">点击上传或拖拽文档到这里</div>
                    <div className="mt-2 text-xs leading-6 text-[#8f8174]">
                      {isCloudMineruMode(props.mineruProcessingConfig?.provider_mode)
                        ? "支持 PDF、PNG、JPG、JPEG、DOCX、PPTX、XLSX，单文件 ≤ 200MB，≤ 200 页，单批次 ≤ 200 个"
                        : "支持 PDF、PNG、JPG、JPEG、DOCX、PPTX、XLSX，最多可上传 300 个文件，单个文件不超过 100MB"}
                    </div>
                  </div>
                  {props.uploadedFiles.length > 0 && (
                    <div className="mt-4 space-y-2">
                      {props.uploadedFiles.map(file => (
                        <UploadFileRow
                          key={file.id}
                          file={file}
                          onEdit={() => openFileEditor(file)}
                          onDelete={() => void props.onDeleteFile(file.storageName || file.name)}
                        />
                      ))}
                    </div>
                  )}
                </div>
              )}

              {props.step === 2 && (
                <div className="h-full overflow-auto">
                  <div className="max-w-[1640px] mx-auto space-y-5 pb-24">
                    <div className="rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] px-5 py-5 shadow-sm">
                      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
                        <div>
                          <div className="text-base font-semibold text-[#241b15]">解析内容</div>
                          <div className="mt-1 text-sm leading-6 text-[#7f7064]">
                            处理完成后会自动写入当前知识库，检索模式和智能体都可以引用。
                          </div>
                        </div>
                        {isCloudMineruMode(props.mineruProcessingConfig?.provider_mode) && (
                          <div className="rounded-lg bg-[#f4dfd2] px-3 py-2 text-sm font-medium text-[#9c593f]">
                            线上 MinerU：单文件 ≤ 200MB，≤ 200 页，单批次 ≤ 200 个
                          </div>
                        )}
                      </div>

                      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                        <ToggleCard
                          checked={props.enableFormula}
                          onChange={props.setEnableFormula}
                          title="公式识别"
                          description="识别行内公式和公式结构"
                        />
                        <ToggleCard
                          checked={props.extractTables}
                          onChange={props.setExtractTables}
                          title="表格识别"
                          description="提取 PDF 中的表格内容"
                        />
                        <ToggleCard
                          checked={props.extractImages}
                          onChange={props.setExtractImages}
                          title="图片元素"
                          description="保留图片与图注，便于后续引用"
                        />
                      </div>

                      <div className="mt-5 grid gap-3 lg:grid-cols-[220px_minmax(0,1fr)]">
                        <label className="flex items-center gap-2 text-base font-semibold text-slate-800">
                          <input
                            type="checkbox"
                            checked={Boolean(props.pageRanges.trim())}
                            onChange={event => {
                              if (!event.target.checked) props.setPageRanges("");
                            }}
                            className="h-4 w-4 accent-[#cc785c]"
                          />
                          指定页码范围
                        </label>
                        <input
                          value={props.pageRanges}
                          onChange={event => props.setPageRanges(event.target.value)}
                          placeholder="默认处理全文，例如 2,4-6 或 2--2"
                          className="h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-base text-slate-900 outline-none placeholder:text-slate-400 focus:border-[#cc785c]"
                        />
                      </div>
                    </div>
              </div>
            </div>
          )}

          {props.step === 3 && (
            <div className="h-full overflow-auto">
              <div className="mx-auto max-w-[1670px] pb-24">
                <h3 className="mb-5 text-lg font-semibold text-[#241b15]">服务器处理中</h3>
                <div className="space-y-3">
                  {props.jobs.filter(job => !selectedFolderName || job.folder === selectedFolderName).map(job => (
                    <JobRow
                      key={job.job_id}
                      job={job}
                      expanded={props.expandedJobId === job.job_id}
                      logs={props.expandedJobLogs[job.job_id] || []}
                      onToggleLogs={() => props.setExpandedJobId(props.expandedJobId === job.job_id ? null : job.job_id)}
                      onDelete={() => void props.onDeleteJob(job.job_id)}
                    />
                  ))}
                  {props.jobs.filter(job => !selectedFolderName || job.folder === selectedFolderName).length === 0 && (
                    <div className="rounded-2xl border border-[#e5d8cc] bg-[#fffaf3] px-5 py-8 text-center text-sm text-[#9a8b7d]">
                      暂无处理任务
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
            </div>
          </section>
          )}
        </div>
      </div>

      <div className="flex h-16 shrink-0 items-center justify-end gap-3 border-t border-[#e5d8cc] bg-[#fffaf3]/95 px-6 shadow-[0_-1px_0_rgba(72,52,38,0.04)]">
        {props.viewMode === "detail" ? (
          <button
            onClick={startAppendToCurrentFolder}
            disabled={!selectedFolderName || props.uploadBusy || props.litLoading !== null}
            className="rounded-xl bg-[#c96f52] px-5 py-2.5 text-base font-semibold text-white shadow-sm transition-colors hover:bg-[#a9583e] disabled:cursor-not-allowed disabled:bg-[#e6dfd8]"
          >
            添加
          </button>
        ) : (
          <>
            {props.step === 3 && (
              <span className="mr-2 text-sm text-slate-500">任务已在服务器后台处理，关闭当前页面不会中断；处理完成后可进行引用</span>
            )}
            {props.step > 1 && (
              <button
                onClick={goPrevious}
                className="rounded-xl bg-[#eee3d8] px-4 py-2.5 text-base font-medium text-[#5d5046] transition-colors hover:bg-[#e5d5c6]"
              >
                上一步
              </button>
            )}
            {props.step < 3 ? (
              <button
                onClick={goNext}
                disabled={(props.step === 1 && !props.canContinueUpload) || props.uploadBusy || props.litLoading !== null}
                className="rounded-xl bg-[#c96f52] px-5 py-2.5 text-base font-semibold text-white shadow-sm transition-colors hover:bg-[#a9583e] disabled:cursor-not-allowed disabled:bg-[#e6dfd8]"
              >
                {props.step === 2 && props.litLoading ? "启动中..." : props.step === 2 ? "确认" : "下一步"}
              </button>
            ) : (
              <button
                onClick={finishProcessingView}
                className="rounded-xl bg-[#c96f52] px-5 py-2.5 text-base font-semibold text-white shadow-sm transition-colors hover:bg-[#a9583e]"
              >
                确认
              </button>
            )}
          </>
        )}
      </div>

      {editingFile && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-slate-950/35">
          <div className="w-[480px] rounded-lg bg-white p-6 shadow-2xl">
            <div className="mb-4 flex items-center justify-between">
              <h3 className="text-lg font-semibold text-slate-950">编辑名称</h3>
              <button
                type="button"
                onClick={closeFileEditor}
                className="rounded-md p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900"
                aria-label="关闭"
              >
                <X className="h-5 w-5" />
              </button>
            </div>
            <div className="relative">
              <textarea
                value={editingName}
                maxLength={100}
                onChange={event => setEditingName(event.target.value)}
                className="h-24 w-full resize-none rounded-lg border border-slate-200 bg-white px-3 py-3 pr-14 text-sm text-slate-800 outline-none focus:border-[#cc785c]"
              />
              <span className="absolute bottom-3 right-3 text-xs text-slate-400">{editingName.length}/100</span>
            </div>
            <div className="mt-4 flex justify-end gap-3">
              <button
                type="button"
                onClick={closeFileEditor}
                className="rounded-lg bg-slate-100 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-200"
              >
                取消
              </button>
              <button
                type="button"
                onClick={confirmFileEditor}
                className="rounded-lg bg-[#cc785c] px-4 py-2 text-sm font-semibold text-white hover:bg-[#a9583e]"
              >
                确认
              </button>
            </div>
          </div>
        </div>
      )}

      {mergingFolder && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-slate-950/35">
          <div className="w-[460px] rounded-lg bg-white p-6 shadow-2xl">
            <div className="mb-4 flex items-center justify-between">
              <h3 className="text-lg font-semibold text-slate-950">合并知识库</h3>
              <button
                type="button"
                onClick={closeMergeDialog}
                className="rounded-md p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900"
                aria-label="关闭"
              >
                <X className="h-5 w-5" />
              </button>
            </div>
            <div className="text-sm leading-6 text-slate-600">
              选择两个知识库进行合并。原知识库都会保留。
            </div>
            <label className="mt-4 block text-sm font-semibold text-slate-900">
              源知识库
              <select
                value={mergingFolder.name}
                onChange={event => {
                  const nextSource = props.folders.find(folder => folder.name === event.target.value) || null;
                  setMergingFolder(nextSource);
                  if (nextSource && mergeTarget === nextSource.name) {
                    setMergeTarget(props.folders.find(folder => folder.name !== nextSource.name)?.name || "");
                  }
                  if (nextSource && !mergeDestination.trim()) {
                    setMergeDestination(`${nextSource.name} 合并库`);
                  }
                }}
                className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none focus:border-[#cc785c]"
              >
                {props.folders.map(folder => (
                  <option key={folder.name} value={folder.name}>{folder.name}</option>
                ))}
              </select>
            </label>
            <div className="mt-4 grid grid-cols-2 gap-2">
              <button
                type="button"
                onClick={() => setMergeMode("new")}
                className={`rounded-lg border px-3 py-2 text-left text-sm font-semibold ${
                  mergeMode === "new" ? "border-[#cc785c] bg-[#fbf2ed] text-[#a9583e]" : "border-slate-200 bg-white text-slate-700 hover:bg-slate-50"
                }`}
              >
                合并为新知识库
              </button>
              <button
                type="button"
                onClick={() => setMergeMode("existing")}
                className={`rounded-lg border px-3 py-2 text-left text-sm font-semibold ${
                  mergeMode === "existing" ? "border-[#cc785c] bg-[#fbf2ed] text-[#a9583e]" : "border-slate-200 bg-white text-slate-700 hover:bg-slate-50"
                }`}
              >
                复制到已有知识库
              </button>
            </div>
            <label className="mt-4 block text-sm font-semibold text-slate-900">
              {mergeMode === "new" ? "参与合并的另一个知识库" : "复制到"}
              <select
                value={mergeTarget}
                onChange={event => setMergeTarget(event.target.value)}
                className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none focus:border-[#cc785c]"
              >
                {props.folders.filter(folder => folder.name !== mergingFolder.name).map(folder => (
                  <option key={folder.name} value={folder.name}>{folder.name}</option>
                ))}
              </select>
            </label>
            {mergeMode === "new" && (
              <label className="mt-4 block text-sm font-semibold text-slate-900">
                新知识库名称
                <input
                  value={mergeDestination}
                  onChange={event => setMergeDestination(event.target.value)}
                  placeholder="请输入新知识库名称"
                  className="mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-[#cc785c]"
                />
              </label>
            )}
            <div className="mt-3 rounded-lg bg-slate-50 px-3 py-2 text-xs leading-5 text-slate-500">
              {mergeMode === "new"
                ? "会创建一个新知识库，复制两个知识库的 PDF 和已处理输出。"
                : "会把当前知识库复制到选中的已有知识库中，不会删除当前知识库。"}
            </div>
            <div className="mt-5 flex justify-end gap-3">
              <button
                type="button"
                onClick={closeMergeDialog}
                className="rounded-lg bg-slate-100 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-200"
              >
                取消
              </button>
              <button
                type="button"
                onClick={confirmMergeDialog}
                disabled={!mergeTarget || (mergeMode === "new" && !mergeDestination.trim())}
                className="rounded-lg bg-[#cc785c] px-4 py-2 text-sm font-semibold text-white hover:bg-[#a9583e] disabled:bg-[#e6dfd8]"
              >
                合并
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function UploadFileRow({
  file,
  onEdit,
  onDelete,
}: {
  file: KnowledgeUploadFile;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const uploading = file.status === "uploading";
  const failed = file.status === "failed";
  return (
    <div className="group relative overflow-hidden rounded-xl border border-[#eadccf] bg-[#fffdf8] shadow-sm transition-all hover:border-[#c96f52] hover:shadow-md">
      {uploading && (
        <div
          className="absolute inset-y-0 left-0 bg-[#f4ded4]/80 transition-[width]"
          style={{ width: `${Math.max(4, Math.min(100, file.progress))}%` }}
        />
      )}
      <div className="relative flex min-h-[54px] items-center justify-between gap-4 px-4 py-2.5">
        <div className="flex min-w-0 items-center gap-3">
          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-[#c96f52] text-[10px] font-bold text-white shadow-sm">
            {(file.name.split(".").pop() || "FILE").toUpperCase().slice(0, 4)}
          </div>
          <div className="min-w-0">
            <div className="truncate text-sm font-medium text-[#241b15]">{file.name}</div>
            <div className={`mt-0.5 text-xs ${failed ? "text-red-500" : "text-[#8f8174]"}`}>
              {failed ? (
                file.error || "上传失败"
              ) : uploading ? (
                formatBytes(file.size)
              ) : (
                <>
                  <span className="group-hover:hidden">{formatBytes(file.size)}</span>
                  <span className="hidden group-hover:inline">上传成功</span>
                </>
              )}
            </div>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2 text-sm text-[#5d5046]">
          {uploading && <span>{file.progress}%</span>}
          {!uploading && (
            <div className="flex items-center gap-2 opacity-0 transition-opacity group-hover:opacity-100">
              <button
                type="button"
                onClick={onEdit}
                className="rounded-md p-1.5 text-[#8a7665] hover:bg-[#f1e6dc] hover:text-[#b85f43]"
                title="编辑"
              >
                <Pencil className="h-4 w-4" />
              </button>
              <button
                type="button"
                onClick={onDelete}
                className="rounded-md p-1.5 text-[#8a7665] hover:bg-red-50 hover:text-red-500"
                title="删除"
              >
                <Trash2 className="h-4 w-4" />
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function ToggleCard({
  checked,
  onChange,
  title,
  description,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  title: string;
  description: string;
}) {
  return (
    <button
      type="button"
      onClick={() => onChange(!checked)}
      className={`rounded-xl border px-4 py-4 text-left transition-all ${
        checked ? "border-[#c96f52] bg-[#fbede3] shadow-sm" : "border-[#eadccf] bg-[#fffdf8] hover:border-[#d8bda9] hover:shadow-sm"
      }`}
    >
      <div className="flex items-center gap-2">
        <span className={`flex h-4 w-4 items-center justify-center rounded border ${
          checked ? "border-[#c96f52] bg-[#c96f52] text-white" : "border-[#d8c6b6] bg-[#fffdf8]"
        }`}>
          {checked && <Check className="h-3 w-3" />}
        </span>
        <span className="text-base font-semibold text-slate-950">{title}</span>
      </div>
      <div className="mt-1 pl-6 text-sm leading-6 text-slate-500">{description}</div>
    </button>
  );
}

function JobRow({
  job,
  expanded,
  logs,
  onToggleLogs,
  onDelete,
}: {
  job: LiteratureJob;
  expanded: boolean;
  logs: string[];
  onToggleLogs: () => void;
  onDelete: () => void;
}) {
  const running = !["completed", "failed"].includes(job.status);
  const progressText = formatJobProgress(job);
  // 批次标识：提交时间(到分钟) + 本批文件名/文件数，区分同名知识库的多次处理
  const fileCount = job.filenames?.length || job.pdf_count || job.paper_count || 0;
  const batchLine = [
    (job.created_at || "").slice(0, 16),
    job.filenames?.length
      ? job.filenames.length === 1
        ? job.filenames[0]
        : `${job.filenames[0]} 等 ${job.filenames.length} 个文件`
      : fileCount
        ? `${fileCount} 个文件`
        : "",
  ].filter(Boolean).join(" · ");
  return (
    <div className="overflow-hidden rounded-lg border border-slate-200 bg-white">
      <div className="relative">
        {running && <div className="absolute inset-y-0 left-0 w-2 bg-indigo-100" />}
        <div className="flex items-center justify-between gap-4 px-5 py-4">
          <div className="flex min-w-0 items-center gap-3">
            <FileText className="w-6 h-6 shrink-0 text-red-500" />
            <div className="min-w-0">
              <div className="truncate text-base font-medium text-slate-950">{job.folder}</div>
              {batchLine && <div className="truncate text-xs text-slate-400">{batchLine}</div>}
              <div className="text-sm text-slate-500">{job.status === "completed" ? "处理完成" : job.status === "failed" ? "处理失败" : job.progress || "服务器处理中"}</div>
            </div>
          </div>
          <div className="flex items-center gap-3 text-sm text-slate-700">
            {progressText && <span>{progressText}</span>}
            {running && <Loader2 className="w-4 h-4 animate-spin text-blue-600" />}
            <button onClick={onToggleLogs} className="rounded p-1.5 text-slate-400 hover:bg-slate-100 hover:text-blue-600" title="查看日志">
              <RefreshCw className="w-4 h-4" />
            </button>
            {["completed", "failed"].includes(job.status) && (
              <button onClick={onDelete} className="rounded p-1.5 text-slate-400 hover:bg-slate-100 hover:text-red-500" title="删除任务">
                <Trash2 className="w-4 h-4" />
              </button>
            )}
          </div>
        </div>
      </div>
      {expanded && (
        <div className="max-h-56 overflow-auto border-t border-slate-200 bg-slate-950 p-4 font-mono text-sm text-green-400">
          {logs.length === 0 ? <span className="text-slate-500">暂无日志输出...</span> : logs.map((line, idx) => <div key={idx} className="whitespace-pre-wrap break-all">{line}</div>)}
        </div>
      )}
    </div>
  );
}

function formatJobProgress(job: LiteratureJob) {
  if (job.status === "completed") {
    return `耗时 ${job.duration || formatSeconds(job.duration_seconds ?? job.elapsed_seconds)}`;
  }
  if (job.status === "failed") return job.error ? "处理失败" : "";
  const elapsed = formatSeconds(job.elapsed_seconds);
  const currentPage = Number(job.current_page || 0);
  const totalPages = Number(job.total_pages || 0);
  const percent = Number(job.progress_percent || 0);
  if (totalPages > 0) {
    if (currentPage > 0 && job.eta_seconds !== undefined && job.eta_seconds !== null) {
      return `${percent}% · ${currentPage}/${totalPages} 页 · 预计剩余约 ${formatSeconds(job.eta_seconds)}`;
    }
    return `${currentPage}/${totalPages} 页 · 估算中 · 已耗时 ${elapsed}`;
  }
  return `处理中 · 已耗时 ${elapsed}`;
}

function formatSeconds(value?: number | null) {
  if (!Number.isFinite(value || 0) || !value || value <= 0) return "0s";
  const total = Math.max(0, Math.round(value));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes > 0) return `${minutes}min ${seconds}s`;
  return `${seconds}s`;
}

function formatBytes(size: number) {
  if (!Number.isFinite(size) || size <= 0) return "0 Byte";
  if (size < 1024) return `${size} Byte`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / 1024 / 1024).toFixed(2)} MB`;
}

function stripDocumentExtension(name: string) {
  const lower = name.toLowerCase();
  const ext = SUPPORTED_UPLOAD_EXTENSIONS.find(item => lower.endsWith(item));
  return ext ? name.slice(0, name.length - ext.length) : name;
}

function ensureDocumentExtension(name: string, originalName: string) {
  const lower = name.toLowerCase();
  if (SUPPORTED_UPLOAD_EXTENSIONS.some(item => lower.endsWith(item))) return name;
  const originalLower = originalName.toLowerCase();
  const originalExt = SUPPORTED_UPLOAD_EXTENSIONS.find(item => originalLower.endsWith(item)) || ".pdf";
  return `${name}${originalExt}`;
}

function isCloudMineruMode(mode?: string) {
  return mode === "cloud_only";
}
