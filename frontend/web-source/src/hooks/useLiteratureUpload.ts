import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  KNOWLEDGE_MAX_PDF_BYTES,
  type KnowledgeUploadFile,
  type LiteratureFolder,
  type LiteratureJob,
} from "../components/KnowledgeBaseWizard";
import {
  deleteLiteratureFolder,
  deleteLiteratureJob,
  deleteLiteraturePdf,
  getLiteratureFilePreview,
  getLiteratureFiles,
  getLiteratureFolders,
  getLiteratureJobLogs,
  getLiteratureJobs,
  mergeLiteratureFolders,
  renameLiteraturePdf,
  startLiteratureProcessing,
  uploadLiteraturePdf,
  type LiteratureFileInfo,
  type LiteratureFilePreview,
} from "../services/literature";

export type KnowledgeViewMode = "create" | "detail";

export function useLiteratureUpload(authScopeKey: string) {
  const [showLiterature, setShowLiterature] = useState(false);
  const [knowledgeViewMode, setKnowledgeViewMode] = useState<KnowledgeViewMode>("create");
  const [litFolders, setLitFolders] = useState<LiteratureFolder[]>([]);
  const [litFoldersLoading, setLitFoldersLoading] = useState(false);
  const [litFoldersLoaded, setLitFoldersLoaded] = useState(false);
  const [litJobs, setLitJobs] = useState<LiteratureJob[]>([]);
  const [litLoading, setLitLoading] = useState<string | null>(null);
  const [expandedJobLogs, setExpandedJobLogs] = useState<Record<string, string[]>>({});
  const [expandedJobId, setExpandedJobId] = useState<string | null>(null);
  const [knowledgeStep, setKnowledgeStep] = useState<1 | 2 | 3>(1);
  const [knowledgeName, setKnowledgeName] = useState("");
  const [knowledgeFolder, setKnowledgeFolder] = useState("");
  const [knowledgeUploadedFiles, setKnowledgeUploadedFiles] = useState<KnowledgeUploadFile[]>([]);
  const [knowledgeUploadBusy, setKnowledgeUploadBusy] = useState(false);
  const [knowledgeParseMode, setKnowledgeParseMode] = useState<"precise" | "fast">("precise");
  const [knowledgeExtractImages, setKnowledgeExtractImages] = useState(true);
  const [knowledgeExtractOcr, setKnowledgeExtractOcr] = useState(true);
  const [knowledgeExtractTables, setKnowledgeExtractTables] = useState(true);
  const [knowledgeEnableFormula, setKnowledgeEnableFormula] = useState(true);
  const [knowledgePageRanges, setKnowledgePageRanges] = useState("");
  const [knowledgeFilterText, setKnowledgeFilterText] = useState("");
  const [knowledgeSegmentMode, setKnowledgeSegmentMode] = useState<"auto" | "custom" | "hierarchy">("auto");
  const [knowledgeMaxChunkSize, setKnowledgeMaxChunkSize] = useState(500);
  const [knowledgeMinChunkSize, setKnowledgeMinChunkSize] = useState(50);
  const [knowledgeChunkOverlap, setKnowledgeChunkOverlap] = useState(0);
  const [literatureFiles, setLiteratureFiles] = useState<LiteratureFileInfo[]>([]);
  const [literatureFilesLoading, setLiteratureFilesLoading] = useState(false);
  const [selectedLiteratureFile, setSelectedLiteratureFile] = useState("");
  const [literatureFilePreview, setLiteratureFilePreview] = useState<LiteratureFilePreview | null>(null);
  const [literaturePreviewLoading, setLiteraturePreviewLoading] = useState(false);
  const selectedLiteratureFileRef = useRef("");
  const literatureFoldersRequestSeqRef = useRef(0);
  const literatureJobsRequestSeqRef = useRef(0);
  const literatureFoldersInFlightRef = useRef(false);
  const literatureJobsInFlightRef = useRef(false);
  const literatureFilesRequestSeqRef = useRef(0);
  const literaturePreviewRequestSeqRef = useRef(0);
  const currentAuthScopeKeyRef = useRef(authScopeKey);
  currentAuthScopeKeyRef.current = authScopeKey;
  const isCurrentAuthScope = useCallback(
    (scope: string) => Boolean(scope) && scope === currentAuthScopeKeyRef.current,
    [],
  );

  const selectedKnowledgeFolder = knowledgeFolder || knowledgeName.trim();

  const fetchLitFolders = useCallback(async (force = false) => {
    const requestScope = authScopeKey;
    if (!requestScope || requestScope !== currentAuthScopeKeyRef.current) return;
    if (literatureFoldersInFlightRef.current && !force) return;
    const requestSeq = ++literatureFoldersRequestSeqRef.current;
    literatureFoldersInFlightRef.current = true;
    setLitFoldersLoading(true);
    try {
      const data = await getLiteratureFolders();
      if (requestSeq !== literatureFoldersRequestSeqRef.current || requestScope !== currentAuthScopeKeyRef.current) return;
      setLitFolders(data.folders || []);
      setLitFoldersLoaded(true);
    } catch (e) {
      console.error("Failed to fetch literature folders", e);
    } finally {
      if (requestSeq === literatureFoldersRequestSeqRef.current && requestScope === currentAuthScopeKeyRef.current) {
        literatureFoldersInFlightRef.current = false;
        setLitFoldersLoading(false);
      }
    }
  }, [authScopeKey]);

  const fetchLitJobs = useCallback(async (force = false) => {
    const requestScope = authScopeKey;
    if (!requestScope || requestScope !== currentAuthScopeKeyRef.current) return;
    if (literatureJobsInFlightRef.current && !force) return;
    const requestSeq = ++literatureJobsRequestSeqRef.current;
    literatureJobsInFlightRef.current = true;
    try {
      const data = await getLiteratureJobs();
      if (requestSeq !== literatureJobsRequestSeqRef.current || requestScope !== currentAuthScopeKeyRef.current) return;
      setLitJobs(data.jobs || []);
    } catch (e) {
      console.error("Failed to fetch literature jobs", e);
    } finally {
      if (requestSeq === literatureJobsRequestSeqRef.current && requestScope === currentAuthScopeKeyRef.current) {
        literatureJobsInFlightRef.current = false;
      }
    }
  }, [authScopeKey]);

  useEffect(() => {
    selectedLiteratureFileRef.current = selectedLiteratureFile;
  }, [selectedLiteratureFile]);

  const fetchLiteratureFiles = useCallback(async (folder: string, preferredFile = "") => {
    const requestScope = authScopeKey;
    if (!requestScope || requestScope !== currentAuthScopeKeyRef.current) return;
    const targetFolder = folder.trim();
    const requestSeq = ++literatureFilesRequestSeqRef.current;
    if (!targetFolder) {
      setLiteratureFiles([]);
      setSelectedLiteratureFile("");
      setLiteratureFilePreview(null);
      return;
    }
    setLiteratureFilesLoading(true);
    setLiteratureFiles([]);
    setSelectedLiteratureFile("");
    setLiteratureFilePreview(null);
    try {
      const data = await getLiteratureFiles(targetFolder);
      if (requestSeq !== literatureFilesRequestSeqRef.current || requestScope !== currentAuthScopeKeyRef.current) return;
      const files = data.files || [];
      setLiteratureFiles(files);
      const nextFile = preferredFile || selectedLiteratureFileRef.current || files[0]?.name || "";
      setSelectedLiteratureFile(files.some(file => file.name === nextFile) ? nextFile : files[0]?.name || "");
    } catch (e) {
      console.error("Failed to fetch literature files", e);
      if (requestSeq !== literatureFilesRequestSeqRef.current || requestScope !== currentAuthScopeKeyRef.current) return;
      setLiteratureFiles([]);
      setSelectedLiteratureFile("");
      setLiteratureFilePreview(null);
    } finally {
      if (requestSeq === literatureFilesRequestSeqRef.current && requestScope === currentAuthScopeKeyRef.current) setLiteratureFilesLoading(false);
    }
  }, [authScopeKey]);

  const openKnowledgeCreate = useCallback(() => {
    setKnowledgeViewMode("create");
    setKnowledgeStep(1);
    setKnowledgeName("");
    setKnowledgeFolder("");
    setKnowledgeUploadedFiles([]);
    setKnowledgeParseMode("precise");
    setKnowledgeExtractImages(true);
    setKnowledgeExtractOcr(true);
    setKnowledgeExtractTables(true);
    setKnowledgeEnableFormula(true);
    setKnowledgePageRanges("");
    setKnowledgeFilterText("");
    setKnowledgeSegmentMode("auto");
    setKnowledgeMaxChunkSize(500);
    setKnowledgeMinChunkSize(50);
    setKnowledgeChunkOverlap(0);
    setSelectedLiteratureFile("");
    setLiteratureFilePreview(null);
  }, []);

  const openKnowledgeAppend = useCallback((folder: string) => {
    const targetFolder = folder.trim();
    if (!targetFolder) return;
    setKnowledgeViewMode("create");
    setKnowledgeStep(1);
    setKnowledgeName(targetFolder);
    setKnowledgeFolder(targetFolder);
    setKnowledgeUploadedFiles([]);
    setKnowledgeParseMode("precise");
    setKnowledgeExtractImages(true);
    setKnowledgeExtractOcr(true);
    setKnowledgeExtractTables(true);
    setKnowledgeEnableFormula(true);
    setKnowledgePageRanges("");
    setKnowledgeFilterText("");
    setKnowledgeSegmentMode("auto");
    setKnowledgeMaxChunkSize(500);
    setKnowledgeMinChunkSize(50);
    setKnowledgeChunkOverlap(0);
    setSelectedLiteratureFile("");
    setLiteratureFilePreview(null);
  }, []);

  const openKnowledgeDetail = useCallback((folder: string) => {
    const targetFolder = folder.trim();
    if (!targetFolder) return;
    setKnowledgeViewMode("detail");
    setKnowledgeFolder(targetFolder);
    setKnowledgeName(targetFolder);
    setKnowledgeUploadedFiles([]);
    setSelectedLiteratureFile("");
    setLiteratureFilePreview(null);
  }, []);

  const resetKnowledgeWizard = useCallback(() => {
    setKnowledgeViewMode("create");
    setKnowledgeStep(1);
    setKnowledgeName("");
    setKnowledgeFolder("");
    setKnowledgeUploadedFiles([]);
    setKnowledgeParseMode("precise");
    setKnowledgeExtractImages(true);
    setKnowledgeExtractOcr(true);
    setKnowledgeExtractTables(true);
    setKnowledgeEnableFormula(true);
    setKnowledgePageRanges("");
    setKnowledgeFilterText("");
    setKnowledgeSegmentMode("auto");
    setKnowledgeMaxChunkSize(500);
    setKnowledgeMinChunkSize(50);
    setKnowledgeChunkOverlap(0);
    setExpandedJobId(null);
    setSelectedLiteratureFile("");
    setLiteratureFilePreview(null);
  }, []);

  useEffect(() => {
    literatureFoldersRequestSeqRef.current += 1;
    literatureJobsRequestSeqRef.current += 1;
    literatureFilesRequestSeqRef.current += 1;
    literaturePreviewRequestSeqRef.current += 1;
    literatureFoldersInFlightRef.current = false;
    literatureJobsInFlightRef.current = false;
    setShowLiterature(false);
    resetKnowledgeWizard();
    setLitFolders([]);
    setLitJobs([]);
    setLitFoldersLoading(Boolean(authScopeKey));
    setLitFoldersLoaded(false);
    setLitLoading(null);
    setExpandedJobLogs({});
    setKnowledgeUploadBusy(false);
    setLiteratureFiles([]);
    setLiteratureFilesLoading(false);
    selectedLiteratureFileRef.current = "";
    setLiteratureFilePreview(null);
    setLiteraturePreviewLoading(false);
    if (!authScopeKey) return;
    void Promise.all([fetchLitFolders(), fetchLitJobs()]);
  }, [authScopeKey, fetchLitFolders, fetchLitJobs, resetKnowledgeWizard]);

  const openKnowledgeWizard = useCallback(() => {
    resetKnowledgeWizard();
    setShowLiterature(true);
  }, [resetKnowledgeWizard]);

  const startLitProcessing = useCallback(async (folder: string, filenames: string[] = []) => {
    const mutationScope = authScopeKey;
    if (!isCurrentAuthScope(mutationScope)) return;
    setLitLoading(folder);
    try {
      await startLiteratureProcessing(folder, {
        parse_mode: knowledgeParseMode,
        segment_mode: knowledgeSegmentMode,
        extract_images: knowledgeExtractImages,
        extract_ocr: knowledgeExtractOcr,
        extract_tables: knowledgeExtractTables,
        enable_formula: knowledgeEnableFormula,
        page_ranges: knowledgePageRanges.trim(),
        filter_text: knowledgeFilterText,
        max_chunk_size: knowledgeMaxChunkSize,
        min_chunk_size: knowledgeMinChunkSize,
        chunk_overlap: knowledgeChunkOverlap,
        filenames,
      });
      if (!isCurrentAuthScope(mutationScope)) return;
      await fetchLitJobs(true);
      if (!isCurrentAuthScope(mutationScope)) return;
    } catch (e: any) {
      if (!isCurrentAuthScope(mutationScope)) return;
      alert(`启动处理失败：${e.message || e}`);
    } finally {
      if (!isCurrentAuthScope(mutationScope)) return;
      setLitLoading(null);
    }
  }, [authScopeKey, fetchLitJobs, isCurrentAuthScope, knowledgeChunkOverlap, knowledgeEnableFormula, knowledgeExtractImages, knowledgeExtractOcr, knowledgeExtractTables, knowledgeFilterText, knowledgeMaxChunkSize, knowledgeMinChunkSize, knowledgePageRanges, knowledgeParseMode, knowledgeSegmentMode]);

  const uploadKnowledgeFiles = useCallback(async (files: FileList | File[]) => {
    const mutationScope = authScopeKey;
    if (!isCurrentAuthScope(mutationScope)) return;
    const incomingFiles = Array.from(files);
    // 上传/解析支持的文档后缀：PDF、图片、Office 文档（MinerU 均支持）
    const supportedExtensions = [".pdf", ".png", ".jpg", ".jpeg", ".docx", ".pptx", ".xlsx"];
    const invalidFiles = incomingFiles.filter(
      file => !supportedExtensions.some(ext => file.name.toLowerCase().endsWith(ext)),
    );
    if (incomingFiles.length === 0) return;
    if (invalidFiles.length > 0) {
      alert("不支持的文件类型，仅支持 PDF、图片、Office 文档");
      return;
    }
    const oversizeFile = incomingFiles.find(file => file.size > KNOWLEDGE_MAX_PDF_BYTES);
    if (oversizeFile) {
      alert(`单个文件不能超过 100MB：${oversizeFile.name}`);
      return;
    }
    if (incomingFiles.length > 300) {
      alert("单个知识库最多上传 300 个文件");
      return;
    }
    const folderName = (knowledgeFolder || knowledgeName.trim()).trim();
    if (!folderName) {
      alert("请先填写知识库名称");
      return;
    }
    setKnowledgeFolder(folderName);
    setKnowledgeName(folderName);
    setKnowledgeUploadBusy(true);
    const queuedFiles = incomingFiles.map((file, index) => ({
      id: `${Date.now()}-${index}-${file.name}`,
      name: file.name,
      size: file.size,
      progress: 8,
      status: "uploading" as const,
    }));
    setKnowledgeUploadedFiles(queuedFiles);
    try {
      for (const [index, file] of incomingFiles.entries()) {
        const uploadId = queuedFiles[index].id;
        setKnowledgeUploadedFiles(prev => prev.map(item => item.id === uploadId ? { ...item, progress: 24, status: "uploading" } : item));
        const result = await uploadLiteraturePdf(folderName, file);
        if (!isCurrentAuthScope(mutationScope)) return;
        setKnowledgeUploadedFiles(prev => prev.map(item => item.id === uploadId ? {
          ...item,
          name: result.filename || file.name,
          storageName: result.filename || file.name,
          progress: 100,
          status: "done",
        } : item));
      }
      await fetchLitFolders(true);
      if (!isCurrentAuthScope(mutationScope)) return;
      await fetchLiteratureFiles(folderName);
      if (!isCurrentAuthScope(mutationScope)) return;
    } catch (error: any) {
      if (!isCurrentAuthScope(mutationScope)) return;
      setKnowledgeUploadedFiles(prev => prev.map(item => item.status === "uploading" ? { ...item, status: "failed", error: error.message || "上传失败" } : item));
      alert(`上传失败：${error.message || error}`);
    } finally {
      if (!isCurrentAuthScope(mutationScope)) return;
      setKnowledgeUploadBusy(false);
    }
  }, [authScopeKey, fetchLitFolders, fetchLiteratureFiles, isCurrentAuthScope, knowledgeFolder, knowledgeName]);

  const renameKnowledgeFile = useCallback(async (filename: string, newFilename: string) => {
    const mutationScope = authScopeKey;
    if (!isCurrentAuthScope(mutationScope)) return;
    const folderName = selectedKnowledgeFolder.trim();
    const nextName = newFilename.trim();
    if (!folderName || !filename || !nextName || filename === nextName) return;
    try {
      const result = await renameLiteraturePdf(folderName, filename, nextName);
      if (!isCurrentAuthScope(mutationScope)) return;
      const finalName = result.filename || nextName;
      setKnowledgeUploadedFiles(prev => prev.map(item => (item.storageName || item.name) === filename ? { ...item, name: finalName, storageName: finalName } : item));
      await fetchLitFolders(true);
      if (!isCurrentAuthScope(mutationScope)) return;
      await fetchLiteratureFiles(folderName, finalName);
      if (!isCurrentAuthScope(mutationScope)) return;
    } catch (error: any) {
      if (!isCurrentAuthScope(mutationScope)) return;
      alert(`重命名失败：${error.message || error}`);
    }
  }, [authScopeKey, fetchLitFolders, fetchLiteratureFiles, isCurrentAuthScope, selectedKnowledgeFolder]);

  const deleteKnowledgeFile = useCallback(async (filename: string) => {
    const mutationScope = authScopeKey;
    if (!isCurrentAuthScope(mutationScope)) return;
    const folderName = selectedKnowledgeFolder.trim();
    if (!folderName || !filename) return;
    try {
      await deleteLiteraturePdf(folderName, filename);
      if (!isCurrentAuthScope(mutationScope)) return;
      setKnowledgeUploadedFiles(prev => prev.filter(item => (item.storageName || item.name) !== filename));
      setLiteratureFiles(prev => prev.filter(item => item.name !== filename));
      if (selectedLiteratureFile === filename) {
        setSelectedLiteratureFile("");
        setLiteratureFilePreview(null);
      }
      await fetchLitFolders(true);
      if (!isCurrentAuthScope(mutationScope)) return;
      await fetchLiteratureFiles(folderName);
      if (!isCurrentAuthScope(mutationScope)) return;
    } catch (error: any) {
      if (!isCurrentAuthScope(mutationScope)) return;
      alert(`删除失败：${error.message || error}`);
    }
  }, [authScopeKey, fetchLitFolders, fetchLiteratureFiles, isCurrentAuthScope, selectedKnowledgeFolder, selectedLiteratureFile]);

  const deleteKnowledgeFolder = useCallback(async (folder: string) => {
    const mutationScope = authScopeKey;
    if (!isCurrentAuthScope(mutationScope)) return;
    const targetFolder = folder.trim();
    if (!targetFolder) return;
    if (!window.confirm(`确定删除知识库「${targetFolder}」吗？这会删除该知识库的 PDF 和已生成的解析输出。`)) return;
    if (!isCurrentAuthScope(mutationScope)) return;
    try {
      await deleteLiteratureFolder(targetFolder);
      if (!isCurrentAuthScope(mutationScope)) return;
      if (selectedKnowledgeFolder === targetFolder) {
        openKnowledgeCreate();
      }
      await fetchLitFolders(true);
      if (!isCurrentAuthScope(mutationScope)) return;
      await fetchLitJobs(true);
      if (!isCurrentAuthScope(mutationScope)) return;
    } catch (error: any) {
      if (!isCurrentAuthScope(mutationScope)) return;
      alert(`删除知识库失败：${error.message || error}`);
    }
  }, [authScopeKey, fetchLitFolders, fetchLitJobs, isCurrentAuthScope, openKnowledgeCreate, selectedKnowledgeFolder]);

  const mergeKnowledgeFolder = useCallback(async (
    source: string,
    target: string,
    options: { mode?: "new" | "existing"; destination?: string } = {},
  ) => {
    const mutationScope = authScopeKey;
    if (!isCurrentAuthScope(mutationScope)) return;
    const sourceFolder = source.trim();
    const targetFolder = target.trim();
    if (!sourceFolder || !targetFolder || sourceFolder === targetFolder) return;
    const destinationFolder = (options.destination || "").trim();
    if (options.mode === "new" && !destinationFolder) return;
    try {
      const result = await mergeLiteratureFolders(sourceFolder, targetFolder, {
        mode: options.mode || "existing",
        destination: destinationFolder,
      });
      if (!isCurrentAuthScope(mutationScope)) return;
      const selectedFolder = result.destination || destinationFolder || targetFolder;
      alert(`合并完成：复制 ${result.copied_pdfs || 0} 个 PDF${result.copied_output ? "，并复用已处理输出" : ""}`);
      await fetchLitFolders(true);
      if (!isCurrentAuthScope(mutationScope)) return;
      openKnowledgeDetail(selectedFolder);
    } catch (error: any) {
      if (!isCurrentAuthScope(mutationScope)) return;
      alert(`合并知识库失败：${error.message || error}`);
    }
  }, [authScopeKey, fetchLitFolders, isCurrentAuthScope, openKnowledgeDetail]);

  const selectedFolderInfo = useMemo(
    () => litFolders.find(folder => folder.name === selectedKnowledgeFolder),
    [litFolders, selectedKnowledgeFolder],
  );
  const canContinueKnowledgeUpload = Boolean(selectedKnowledgeFolder && knowledgeUploadedFiles.some(file => file.status === "done"));

  const confirmKnowledgeProcessing = useCallback(async () => {
    const mutationScope = authScopeKey;
    if (!isCurrentAuthScope(mutationScope)) return;
    if (!selectedKnowledgeFolder) {
      alert("请先上传或选择 PDF");
      return;
    }
    const uploadedFilenames = knowledgeUploadedFiles
      .filter(file => file.status === "done")
      .map(file => file.storageName || file.name)
      .filter(Boolean);
    await startLitProcessing(selectedKnowledgeFolder, uploadedFilenames);
    if (!isCurrentAuthScope(mutationScope)) return;
    await fetchLitJobs(true);
    if (!isCurrentAuthScope(mutationScope)) return;
    setKnowledgeStep(3);
  }, [authScopeKey, fetchLitJobs, isCurrentAuthScope, knowledgeUploadedFiles, selectedKnowledgeFolder, startLitProcessing]);

  const deleteLitJob = useCallback(async (jobId: string) => {
    const mutationScope = authScopeKey;
    if (!isCurrentAuthScope(mutationScope)) return;
    try {
      await deleteLiteratureJob(jobId);
      if (!isCurrentAuthScope(mutationScope)) return;
      await fetchLitJobs(true);
      if (!isCurrentAuthScope(mutationScope)) return;
    } catch (e) {
      if (!isCurrentAuthScope(mutationScope)) return;
      console.error("Failed to delete job", e);
    }
  }, [authScopeKey, fetchLitJobs, isCurrentAuthScope]);

  useEffect(() => {
    if (showLiterature) {
      fetchLitFolders();
      fetchLitJobs();
      const interval = setInterval(fetchLitJobs, 3000);
      return () => clearInterval(interval);
    }
  }, [showLiterature, fetchLitFolders, fetchLitJobs]);

  useEffect(() => {
    if (!showLiterature || knowledgeViewMode !== "detail" || !selectedKnowledgeFolder) return;
    void fetchLiteratureFiles(selectedKnowledgeFolder);
  }, [fetchLiteratureFiles, knowledgeViewMode, selectedKnowledgeFolder, showLiterature]);

  useEffect(() => {
    const requestSeq = ++literaturePreviewRequestSeqRef.current;
    const requestScope = authScopeKey;
    if (!requestScope || requestScope !== currentAuthScopeKeyRef.current || !selectedKnowledgeFolder || !selectedLiteratureFile) {
      setLiteratureFilePreview(null);
      setLiteraturePreviewLoading(false);
      return;
    }
    let cancelled = false;
    setLiteraturePreviewLoading(true);
    getLiteratureFilePreview(selectedKnowledgeFolder, selectedLiteratureFile)
      .then(data => {
        if (!cancelled && requestSeq === literaturePreviewRequestSeqRef.current && requestScope === currentAuthScopeKeyRef.current) setLiteratureFilePreview(data);
      })
      .catch(error => {
        console.error("Failed to fetch literature preview", error);
        if (!cancelled && requestSeq === literaturePreviewRequestSeqRef.current && requestScope === currentAuthScopeKeyRef.current) setLiteratureFilePreview(null);
      })
      .finally(() => {
        if (!cancelled && requestSeq === literaturePreviewRequestSeqRef.current && requestScope === currentAuthScopeKeyRef.current) setLiteraturePreviewLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [authScopeKey, selectedKnowledgeFolder, selectedLiteratureFile]);

  useEffect(() => {
    const requestScope = authScopeKey;
    if (!requestScope || requestScope !== currentAuthScopeKeyRef.current || !expandedJobId) return;
    let cancelled = false;
    const fetchLogs = async () => {
      try {
        const data = await getLiteratureJobLogs(expandedJobId);
        if (!cancelled && requestScope === currentAuthScopeKeyRef.current && data.ok) {
          setExpandedJobLogs(prev => ({ ...prev, [expandedJobId]: data.logs as string[] }));
        }
      } catch {
        // ignore polling failures
      }
    };
    fetchLogs();
    const interval = setInterval(fetchLogs, 2000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [authScopeKey, expandedJobId]);

  return {
    showLiterature,
    setShowLiterature,
    knowledgeViewMode,
    setKnowledgeViewMode,
    openKnowledgeCreate,
    openKnowledgeAppend,
    openKnowledgeDetail,
    litFolders,
    litFoldersLoading,
    litFoldersLoaded,
    litJobs,
    litLoading,
    expandedJobLogs,
    expandedJobId,
    setExpandedJobId,
    knowledgeStep,
    setKnowledgeStep,
    knowledgeName,
    setKnowledgeName,
    knowledgeFolder,
    setKnowledgeFolder,
    knowledgeUploadedFiles,
    setKnowledgeUploadedFiles,
    knowledgeUploadBusy,
    knowledgeParseMode,
    setKnowledgeParseMode,
    knowledgeExtractImages,
    setKnowledgeExtractImages,
    knowledgeExtractOcr,
    setKnowledgeExtractOcr,
    knowledgeExtractTables,
    setKnowledgeExtractTables,
    knowledgeEnableFormula,
    setKnowledgeEnableFormula,
    knowledgePageRanges,
    setKnowledgePageRanges,
    knowledgeFilterText,
    setKnowledgeFilterText,
    knowledgeSegmentMode,
    setKnowledgeSegmentMode,
    knowledgeMaxChunkSize,
    setKnowledgeMaxChunkSize,
    knowledgeMinChunkSize,
    setKnowledgeMinChunkSize,
    knowledgeChunkOverlap,
    setKnowledgeChunkOverlap,
    literatureFiles,
    literatureFilesLoading,
    selectedLiteratureFile,
    setSelectedLiteratureFile,
    literatureFilePreview,
    literaturePreviewLoading,
    fetchLitFolders,
    fetchLitJobs,
    fetchLiteratureFiles,
    startLitProcessing,
    resetKnowledgeWizard,
    openKnowledgeWizard,
    uploadKnowledgeFiles,
    renameKnowledgeFile,
    deleteKnowledgeFile,
    deleteKnowledgeFolder,
    mergeKnowledgeFolder,
    selectedKnowledgeFolder,
    selectedFolderInfo,
    canContinueKnowledgeUpload,
    confirmKnowledgeProcessing,
    deleteLitJob,
  };
}
