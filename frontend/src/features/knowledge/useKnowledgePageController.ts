import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocale } from "../../i18n/locale";
import {
  desktop,
  type BackgroundTask,
  type KnowledgeBaseRecord,
  type KnowledgeHealth,
  type PostgresIngestionJob,
  type ProviderProfileResponse,
  type SourceDocumentRecord,
} from "../../bridge/desktop";
import {
  type KnowledgeUploadFile,
  type LiteratureFolder,
  type LiteratureJob,
} from "./KnowledgeBaseWizard";
import type { LiteratureFileInfo, LiteratureFilePreview } from "./knowledgeTypes";
import {
  emptyHealth,
  errorMessage,
  parseRetrievalSetup,
  type RetrievalSetup,
} from "./knowledgeModel";

type KnowledgeViewMode = "create" | "detail";
type KnowledgeStep = 1 | 2 | 3;

const supportedExtensions = [".pdf", ".png", ".jpg", ".jpeg", ".docx", ".pptx", ".xlsx"];

function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function taskToJob(task: BackgroundTask, folder: string): LiteratureJob {
  const terminal = task.state === "completed" || task.state === "failed";
  return {
    job_id: task.id,
    folder,
    status: task.state === "completed" ? "completed" : task.state === "failed" ? "failed" : "running",
    progress: terminal ? "" : "本地处理中",
    error: task.error_code ?? undefined,
    filenames: task.file_name ? [task.file_name] : undefined,
    created_at: task.created_at,
    elapsed_seconds: task.started_at ? Math.max(0, (Date.now() - Date.parse(task.started_at)) / 1000) : 0,
    progress_percent: task.progress,
  };
}

function postgresJobToJob(job: PostgresIngestionJob, folder: string): LiteratureJob {
  const status = job.state === "completed" ? "completed" : job.state === "failed" || job.state === "quarantined" ? "failed" : "running";
  return {
    job_id: `pg:${job.id}`,
    folder,
    status,
    progress: status === "failed" ? job.error_message || "处理失败" : status === "completed" ? "处理完成" : "服务器处理中",
    error: job.error_message ?? undefined,
    created_at: job.created_at,
    elapsed_seconds: 0,
  };
}

export default function useKnowledgePageController() {
  const { t } = useLocale();
  const [bases, setBases] = useState<KnowledgeBaseRecord[]>([]);
  const [baseCounts, setBaseCounts] = useState<Record<string, number>>({});
  const [selectedBaseId, setSelectedBaseId] = useState<string | null>(null);
  const [documents, setDocuments] = useState<SourceDocumentRecord[]>([]);
  const [literatureFiles, setLiteratureFiles] = useState<LiteratureFileInfo[]>([]);
  const [selectedLiteratureFile, setSelectedLiteratureFile] = useState("");
  const [literatureFilePreview, setLiteratureFilePreview] = useState<LiteratureFilePreview | null>(null);
  const [literatureFilesLoading, setLiteratureFilesLoading] = useState(false);
  const [literaturePreviewLoading, setLiteraturePreviewLoading] = useState(false);
  const [tasks, setTasks] = useState<BackgroundTask[]>([]);
  const [postgresJobs, setPostgresJobs] = useState<PostgresIngestionJob[]>([]);
  const [health, setHealth] = useState<KnowledgeHealth>(emptyHealth);
  const [retrieval, setRetrieval] = useState<RetrievalSetup>({
    embeddingProfileId: null,
    mineruProfileId: null,
  });
  const [providerProfiles, setProviderProfiles] = useState<ProviderProfileResponse[]>([]);
  const [viewMode, setViewMode] = useState<KnowledgeViewMode>("create");
  const [step, setStep] = useState<KnowledgeStep>(1);
  const [knowledgeName, setKnowledgeName] = useState("");
  const [knowledgeFolder, setKnowledgeFolder] = useState("");
  const [uploadedFiles, setUploadedFiles] = useState<KnowledgeUploadFile[]>([]);
  const [uploadBusy, setUploadBusy] = useState(false);
  const [litLoading, setLitLoading] = useState<string | null>(null);
  const [expandedJobId, setExpandedJobId] = useState<string | null>(null);
  const [expandedJobLogs] = useState<Record<string, string[]>>({});
  const [parseMode, setParseMode] = useState<"precise" | "fast">("precise");
  const [extractImages, setExtractImages] = useState(true);
  const [extractOcr, setExtractOcr] = useState(true);
  const [extractTables, setExtractTables] = useState(true);
  const [enableFormula, setEnableFormula] = useState(true);
  const [pageRanges, setPageRanges] = useState("");
  const [filterText, setFilterText] = useState("");
  const [segmentMode, setSegmentMode] = useState<"auto" | "custom" | "hierarchy">("auto");
  const [maxChunkSize, setMaxChunkSize] = useState(500);
  const [minChunkSize, setMinChunkSize] = useState(50);
  const [chunkOverlap, setChunkOverlap] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const selectedBase = useMemo(
    () => bases.find((base) => base.id === selectedBaseId) ?? null,
    [bases, selectedBaseId],
  );
  const selectedFolder = knowledgeFolder || selectedBase?.name || knowledgeName.trim();
  const folders = useMemo<LiteratureFolder[]>(
    () => bases.map((base) => ({ name: base.name, pdf_count: baseCounts[base.id] ?? 0 })),
    [baseCounts, bases],
  );
  const embeddingProfile = providerProfiles.find((profile) => profile.id === retrieval.embeddingProfileId);

  const loadDocuments = useCallback(async (baseId: string | null, preferredName = "") => {
    if (!baseId) {
      setDocuments([]);
      setLiteratureFiles([]);
      setSelectedLiteratureFile("");
      return;
    }
    setLiteratureFilesLoading(true);
    try {
      const nextDocuments = await desktop.listKnowledgeDocuments(baseId);
      setDocuments(nextDocuments);
      if (typeof desktop.listPostgresIngestionJobs === "function") {
        setPostgresJobs(await desktop.listPostgresIngestionJobs(baseId).catch(() => []));
      }
      const nextId = nextDocuments.find((document) => document.display_name === preferredName)?.id
        ?? nextDocuments[0]?.id
        ?? null;
      setLiteratureFiles(nextDocuments.map((document) => ({
        name: document.display_name,
        size: 0,
        document_id: document.id,
      })));
      setSelectedLiteratureFile(
        nextDocuments.find((document) => document.id === nextId)?.display_name
        ?? nextDocuments[0]?.display_name
        ?? "",
      );
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setLiteratureFilesLoading(false);
    }
  }, [t]);

  const loadOverview = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextBases, nextHealth, nextTasks, setting, profiles] = await Promise.all([
        desktop.listKnowledgeBases(),
        desktop.getKnowledgeHealth(),
        desktop.listBackgroundTasks(),
        desktop.getSetting("onboarding.retrieval"),
        desktop.listProviderProfiles(),
      ]);
      const counts = await Promise.all(nextBases.map(async (base) => (
        [base.id, (await desktop.listKnowledgeDocuments(base.id)).length] as const
      )));
      setBases(nextBases);
      setBaseCounts(Object.fromEntries(counts));
      setHealth(nextHealth);
      setTasks(nextTasks);
      setRetrieval(parseRetrievalSetup(setting));
      setProviderProfiles(profiles);
      setSelectedBaseId((current) => {
        const nextId = current && nextBases.some((base) => base.id === current)
          ? current
          : nextBases[0]?.id ?? null;
        const nextBase = nextBases.find((base) => base.id === nextId);
        if (nextBase) {
          setKnowledgeName((name) => name || nextBase.name);
          setKnowledgeFolder((folder) => folder || nextBase.name);
        }
        return nextId;
      });
      if (nextBases.length > 0) setViewMode("detail");
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void loadOverview();
  }, [loadOverview]);

  useEffect(() => {
    void loadDocuments(selectedBaseId);
  }, [loadDocuments, selectedBaseId]);

  useEffect(() => {
    if (!selectedBaseId && bases.length > 0) {
      setSelectedBaseId(bases[0].id);
      setKnowledgeName((current) => current || bases[0].name);
      setKnowledgeFolder((current) => current || bases[0].name);
    }
  }, [bases, selectedBaseId]);

  useEffect(() => {
    if (documents.length > 0 && viewMode === "create" && step === 1 && uploadedFiles.length === 0) {
      setViewMode("detail");
    }
  }, [documents.length, step, uploadedFiles.length, viewMode]);

  useEffect(() => {
    const current = literatureFiles.find((file) => file.name === selectedLiteratureFile);
    if (!current?.document_id) {
      setLiteratureFilePreview(null);
      return;
    }
    let cancelled = false;
    setLiteraturePreviewLoading(true);
    void desktop.getKnowledgeDocumentPreview(current.document_id)
      .then((preview) => {
        if (!cancelled) {
          setLiteratureFilePreview({
            folder: selectedFolder,
            filename: current.name,
            processed: preview.processed,
            content: preview.content ?? undefined,
            blocks: preview.blocks,
            raw_data_url: preview.raw_data_url ?? undefined,
            raw_sheets: preview.raw_sheets ?? [],
          });
        }
      })
      .catch(() => {
        if (!cancelled) setLiteratureFilePreview(null);
      })
      .finally(() => {
        if (!cancelled) setLiteraturePreviewLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [literatureFiles, selectedFolder, selectedLiteratureFile]);

  const resetWizard = useCallback(() => {
    setViewMode("create");
    setStep(1);
    setKnowledgeName("");
    setKnowledgeFolder("");
    setUploadedFiles([]);
    setSelectedLiteratureFile("");
    setLiteratureFilePreview(null);
    setLitLoading(null);
    setExpandedJobId(null);
  }, []);

  const openCreateView = useCallback(() => {
    resetWizard();
    setSelectedBaseId(null);
  }, [resetWizard]);

  const openAppendView = useCallback((folder: string) => {
    const base = bases.find((item) => item.name === folder);
    if (!base) return;
    setSelectedBaseId(base.id);
    setViewMode("create");
    setStep(1);
    setKnowledgeName(base.name);
    setKnowledgeFolder(base.name);
    setUploadedFiles([]);
    setSelectedLiteratureFile("");
    setLiteratureFilePreview(null);
  }, [bases]);

  const openDetailView = useCallback((folder: string) => {
    const base = bases.find((item) => item.name === folder);
    if (!base) return;
    setSelectedBaseId(base.id);
    setViewMode("detail");
    setKnowledgeName(base.name);
    setKnowledgeFolder(base.name);
    setUploadedFiles([]);
  }, [bases]);

  const chooseFiles = useCallback(async () => {
    const selected = await desktop.openFileDialog({
      directory: false,
      multiple: true,
      title: t("browseFile"),
      filters: [{ name: t("importLocalDocument"), extensions: ["pdf", "png", "jpg", "jpeg", "docx", "pptx", "xlsx"] }],
    });
    const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
    if (paths.length === 0) return;
    if (!knowledgeName.trim() && !knowledgeFolder.trim()) {
      setError("请先填写知识库名称");
      return;
    }
    setKnowledgeFolder((current) => current || knowledgeName.trim());
    setKnowledgeName((current) => current.trim() || knowledgeFolder.trim());
    setUploadedFiles(paths.map((path, index) => ({
      id: `${Date.now()}-${index}-${path}`,
      name: fileName(path),
      storageName: path,
      size: 0,
      progress: 100,
      status: "done" as const,
    })));
  }, [knowledgeFolder, knowledgeName, t]);

  const uploadFiles = useCallback((files: FileList | File[]) => {
    const nextFiles = Array.from(files)
      .filter((file) => supportedExtensions.some((extension) => file.name.toLowerCase().endsWith(extension)))
      .map((file, index) => ({
        id: `${Date.now()}-${index}-${file.name}`,
        name: file.name,
        storageName: (file as File & { path?: string }).path || file.name,
        size: Number.isFinite(file.size) ? file.size : 0,
        progress: 100,
        status: "done" as const,
      }));
    if (nextFiles.length > 0) setUploadedFiles(nextFiles);
  }, []);

  const confirmProcessing = useCallback(async () => {
    const name = (knowledgeFolder || knowledgeName).trim();
    if (!name || uploadedFiles.length === 0) return;
    const embeddingProfileId = retrieval.embeddingProfileId;
    if (!embeddingProfileId) {
      setError(t("setupRetrievalFirst"));
      return;
    }
    setLitLoading(name);
    setUploadBusy(true);
    setError(null);
    try {
      const postgresHealth = typeof desktop.getKnowledgeDatabaseHealth === "function"
        ? await desktop.getKnowledgeDatabaseHealth().catch(() => null)
        : null;
      if (!postgresHealth?.connected || !postgresHealth.vector_extension) {
        throw new Error("请先连接 PostgreSQL 并启用 pgvector");
      }
      const postgresBases = await desktop.listPostgresKnowledgeBases();
      const postgresBaseId = selectedBaseId
        || postgresBases.find((base) => base.name === name)?.id
        || (await desktop.createPostgresKnowledgeBase(name)).id;
      for (const file of uploadedFiles.filter((item) => item.status === "done")) {
        await desktop.importPostgresDocument({
          knowledge_base_id: postgresBaseId,
          source_path: file.storageName || file.name,
        });
      }
      setStep(3);
      setTasks(await desktop.listBackgroundTasks());
      setHealth(await desktop.getKnowledgeHealth());
      setSelectedBaseId(postgresBaseId);
      setPostgresJobs(await desktop.listPostgresIngestionJobs(postgresBaseId));
      await loadOverview();
    } catch (cause) {
      setError(errorMessage(cause, t("knowledgeError")));
    } finally {
      setUploadBusy(false);
      setLitLoading(null);
    }
  }, [knowledgeFolder, knowledgeName, retrieval, selectedBaseId, t, uploadedFiles]);

  const renameFile = useCallback(async (filename: string, newFilename: string) => {
    const document = documents.find((item) => item.display_name === filename);
    if (!document) return;
    await desktop.renameKnowledgeDocument(document.id, newFilename);
    await loadDocuments(selectedBaseId, newFilename);
    await loadOverview();
  }, [documents, loadDocuments, loadOverview, selectedBaseId]);

  const deleteFile = useCallback(async (filename: string) => {
    const document = documents.find((item) => item.display_name === filename);
    if (!document) return;
    await desktop.deleteKnowledgeDocument(document.id);
    await loadDocuments(selectedBaseId);
    await loadOverview();
  }, [documents, loadDocuments, loadOverview, selectedBaseId]);

  const deleteFolder = useCallback(async (folder: string) => {
    const base = bases.find((item) => item.name === folder);
    if (!base) return;
    const impact = await desktop.previewDeleteKnowledgeBase(base.id);
    if (impact.active_task_count > 0) {
      setError("请先取消活动任务");
      return;
    }
    await desktop.deleteKnowledgeBaseConfirmed(base.id);
    resetWizard();
    await loadOverview();
  }, [bases, loadOverview, resetWizard]);

  const mergeFolder = useCallback(async (
    source: string,
    target: string,
    options: { mode?: "new" | "existing"; destination?: string } = {},
  ) => {
    const sourceBase = bases.find((item) => item.name === source);
    const targetBase = bases.find((item) => item.name === target);
    if (!sourceBase || !targetBase) return;
    const merged = await desktop.mergeKnowledgeBases({
      source_id: sourceBase.id,
      target_id: targetBase.id,
      mode: options.mode || "existing",
      destination_name: options.destination || null,
    });
    await loadOverview();
    openDetailView(merged.name);
  }, [bases, loadOverview, openDetailView]);

  const jobs = useMemo(
    () => [
      ...tasks
        .filter((task) => task.kind === "mineru_parse" || task.kind === "rag_index_rebuild")
        .map((task) => taskToJob(task, selectedFolder)),
      ...postgresJobs.map((job) => postgresJobToJob(job, selectedFolder)),
    ],
    [postgresJobs, selectedFolder, tasks],
  );

  const cancelJob = useCallback(async (jobId: string) => {
    await desktop.cancelBackgroundTask(jobId);
    setTasks(await desktop.listBackgroundTasks());
  }, []);

  const retryJob = useCallback(async (jobId: string) => {
    if (jobId.startsWith("pg:")) {
      if (typeof desktop.retryPostgresIngestionJob === "function") {
        await desktop.retryPostgresIngestionJob(jobId.slice(3));
        if (selectedBaseId && typeof desktop.listPostgresIngestionJobs === "function") {
          setPostgresJobs(await desktop.listPostgresIngestionJobs(selectedBaseId));
        }
      }
      return;
    }
    await desktop.retryBackgroundTask(jobId);
    setTasks(await desktop.listBackgroundTasks());
  }, [selectedBaseId]);

  return {
    viewMode,
    openCreateView,
    openAppendView,
    openDetailView,
    step,
    setStep,
    knowledgeName,
    setKnowledgeName: (value: string) => {
      setKnowledgeName(value);
      if (!knowledgeFolder) setKnowledgeFolder(value);
    },
    knowledgeFolder,
    setKnowledgeFolder,
    uploadedFiles,
    uploadBusy,
    folders,
    foldersLoading: loading,
    foldersLoaded: !loading,
    onRetryFolders: () => void loadOverview(),
    literatureFiles,
    literatureFilesLoading,
    selectedLiteratureFile,
    setSelectedLiteratureFile,
    literatureFilePreview,
    literaturePreviewLoading,
    jobs,
    canContinueUpload: Boolean(selectedFolder && uploadedFiles.some((file) => file.status === "done")),
    litLoading,
    expandedJobId,
    expandedJobLogs,
    parseMode,
    setParseMode,
    extractImages,
    setExtractImages,
    extractOcr,
    setExtractOcr,
    extractTables,
    setExtractTables,
    enableFormula,
    setEnableFormula,
    pageRanges,
    setPageRanges,
    filterText,
    setFilterText,
    segmentMode,
    setSegmentMode,
    maxChunkSize,
    setMaxChunkSize,
    minChunkSize,
    setMinChunkSize,
    chunkOverlap,
    setChunkOverlap,
    onClose: () => undefined,
    onUploadFiles: uploadFiles,
    onChooseFiles: chooseFiles,
    onRenameFile: renameFile,
    onDeleteFile: deleteFile,
    onDeleteFolder: deleteFolder,
    onMergeFolder: mergeFolder,
    onConfirmProcessing: confirmProcessing,
    onDeleteJob: async (jobId: string) => {
      if (jobId.startsWith("pg:")) return;
      await desktop.cancelBackgroundTask(jobId);
    },
    onRetryJob: retryJob,
    setExpandedJobId,
    processingLabel: "本地处理中",
    mineruProcessingConfig: embeddingProfile ? { provider_mode: "local" } : undefined,
    cancelJob,
    retryJob,
    error,
    health,
  };
}
