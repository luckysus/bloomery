import { useEffect, useState, type DragEvent, type ReactNode } from "react";
import { ArrowRight, BookOpen, ChevronDown, Clock3, Copy, Database, ExternalLink, FileText, History, Info, MessageSquarePlus, Network, Pencil, Plus, Search, SlidersHorizontal, Upload, X } from "lucide-react";
import { desktop, type EvidencePack, type KnowledgeBaseRecord, type KnowledgeHealth, type PostgresIngestionJob, type PostgresKnowledgeChunk, type PostgresKnowledgeEdge, type PostgresKnowledgeSearchHit, type PostgresWikiPage, type PostgresWikiRevision, type ProviderProfileResponse, type SourceDocumentRecord } from "../../bridge/desktop";

type Tab = "documents" | "chunks" | "search" | "wiki" | "graph" | "settings";
type LibraryTab = "mine" | "public" | "manage";
type ImportQueueItem = { path: string; name: string; state: "pending" | "submitting" | "submitted" | "failed"; error?: string };

const IMPORT_EXTENSIONS = new Set(["pdf", "docx", "xls", "xlsx", "csv", "txt", "md", "markdown", "json", "html", "htm"]);

function formatSourceLocation(location: Record<string, unknown>) {
  const kind = String(location.kind || "");
  if (kind === "pdf_page") return `PDF 第 ${String(location.page || "?")} 页`;
  if (kind === "sheet_range") return `${String(location.sheet || "工作表")}!${String(location.range || "")}`;
  if (kind === "heading") return Array.isArray(location.path) ? location.path.join(" / ") : String(location.path || "标题段落");
  if (kind === "text_offsets") return `文本位置 ${String(location.start || 0)}-${String(location.end || 0)}`;
  return Object.keys(location).length ? JSON.stringify(location) : "未标注位置";
}

function degradationLabel(reason: string) {
  const labels: Record<string, string> = {
    missing_credential: "未配置凭据",
    invalid_configuration: "配置无效",
    network: "网络不可用",
    authentication: "认证失败",
    quota: "额度不足",
    timeout: "请求超时",
    cancelled: "请求已取消",
    unsupported_capability: "当前环境不支持",
    provider_response: "服务返回错误",
    malformed_response: "服务返回格式无效",
  };
  return labels[reason] || reason;
}

export default function KnowledgeCenterWorkspace() {
  const [bases, setBases] = useState<KnowledgeBaseRecord[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [documents, setDocuments] = useState<SourceDocumentRecord[]>([]);
  const [documentCounts, setDocumentCounts] = useState<Record<string, number>>({});
  const [health, setHealth] = useState<KnowledgeHealth | null>(null);
  const [jobs, setJobs] = useState<PostgresIngestionJob[]>([]);
  const [chunks, setChunks] = useState<PostgresKnowledgeChunk[]>([]);
  const [pages, setPages] = useState<PostgresWikiPage[]>([]);
  const [edges, setEdges] = useState<PostgresKnowledgeEdge[]>([]);
  const [tab, setTab] = useState<Tab>("documents");
  const [libraryTab, setLibraryTab] = useState<LibraryTab>("mine");
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [typeFilter, setTypeFilter] = useState("全部类型");
  const [sortMode, setSortMode] = useState("最近更新");
  const [doc, setDoc] = useState<SourceDocumentRecord | null>(null);
  const [preview, setPreview] = useState<import("../../bridge/desktop").KnowledgeDocumentPreview | null>(null);
  const [previewPage, setPreviewPage] = useState<number | null>(null);
  const [documentVersions, setDocumentVersions] = useState<import("../../bridge/desktop").DocumentVersionRecord[]>([]);
  const [page, setPage] = useState<PostgresWikiPage | null>(null);
  const [revisions, setRevisions] = useState<PostgresWikiRevision[]>([]);
  const [wikiTags, setWikiTags] = useState<string[]>([]);
  const [availableWikiTags, setAvailableWikiTags] = useState<string[]>([]);
  const [wikiSourceDocumentId, setWikiSourceDocumentId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadFailed, setLoadFailed] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [createName, setCreateName] = useState("");
  const [createDescription, setCreateDescription] = useState("");
  const [createType, setCreateType] = useState("research");
  const [createTags, setCreateTags] = useState("");
  const [createEmbedding, setCreateEmbedding] = useState("");
  const [createChunkStrategy, setCreateChunkStrategy] = useState("semantic");
  const [renameTarget, setRenameTarget] = useState<SourceDocumentRecord | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<SourceDocumentRecord | null>(null);
  const [wikiCreateOpen, setWikiCreateOpen] = useState(false);
  const [wikiTitle, setWikiTitle] = useState("新的研究笔记");
  const [moveTarget, setMoveTarget] = useState<SourceDocumentRecord | null>(null);
  const [moveBaseId, setMoveBaseId] = useState("");
  const [embedTarget, setEmbedTarget] = useState<SourceDocumentRecord | null>(null);
  const [embedProfiles, setEmbedProfiles] = useState<ProviderProfileResponse[]>([]);
  const [embedProfileId, setEmbedProfileId] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<PostgresKnowledgeSearchHit[]>([]);
  const [searchAuditId, setSearchAuditId] = useState<string | null>(null);
  const [searchBusy, setSearchBusy] = useState(false);
  const [searchEmbeddingDegradation, setSearchEmbeddingDegradation] = useState<string | null>(null);
  const [searchRerankDegradation, setSearchRerankDegradation] = useState<string | null>(null);
  const [searchFilters, setSearchFilters] = useState({ document_type: "", material: "", process: "", property: "", year: "", tag: "" });
  const [favoriteChunkIds, setFavoriteChunkIds] = useState<string[]>([]);

  useEffect(() => {
    void desktop.getSetting("knowledge.favorites").then((raw) => {
      if (!raw) return;
      try {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed?.chunk_ids)) setFavoriteChunkIds(parsed.chunk_ids.filter((id: unknown): id is string => typeof id === "string"));
      } catch { /* Ignore malformed local preference and keep an empty list. */ }
    }).catch(() => undefined);
  }, []);
  const [importQueue, setImportQueue] = useState<ImportQueueItem[]>([]);

  useEffect(() => {
    const timer = window.setTimeout(() => setDebouncedQuery(query.trim().toLocaleLowerCase()), 180);
    return () => window.clearTimeout(timer);
  }, [query]);

  const loadBase = async (id: string | null) => {
    if (!id) return;
    const results = await Promise.all([
      desktop.listKnowledgeDocuments(id),
      desktop.listPostgresIngestionJobs(id),
      desktop.listPostgresWikiPages(id),
      desktop.listPostgresKnowledgeEdges(id),
      desktop.listPostgresKnowledgeChunks(id),
    ]);
    setDocuments(results[0]);
    setJobs(results[1]);
    setPages(results[2]);
    setEdges(results[3]);
    setChunks(results[4]);
  };
  const reload = async (preferred?: string) => {
    setLoading(true);
    try {
      const next = await desktop.listKnowledgeBases();
      setBases(next);
      const counts = await Promise.all(next.map(async (item) => [item.id, (await desktop.listKnowledgeDocuments(item.id)).length] as const));
      setDocumentCounts(Object.fromEntries(counts));
      const id = preferred || selectedId || next[0]?.id || null;
      setSelectedId(next.some((item) => item.id === id) ? id : next[0]?.id || null);
      setHealth(await desktop.getKnowledgeHealth());
      await loadBase(id);
      setLoadFailed(false);
      setMessage(null);
    } catch (reason) {
      setLoadFailed(true);
      setMessage(reason instanceof Error ? reason.message : "知识中心加载失败");
      throw reason;
    } finally { setLoading(false); }
  };
  useEffect(() => { void reload().catch(() => setMessage("知识库加载失败")); }, []);
  useEffect(() => { void loadBase(selectedId).catch(() => setMessage("知识库内容加载失败")); }, [selectedId]);
  useEffect(() => {
    if (!selectedId) { setAvailableWikiTags([]); return; }
    void desktop.listPostgresTags(selectedId).then((tags) => setAvailableWikiTags(tags.map((tag) => tag.name))).catch(() => setAvailableWikiTags([]));
  }, [selectedId]);
  useEffect(() => {
    if (!selectedId || (!busy && !jobs.some((job) => ["queued", "uploading", "parsing", "chunking", "embedding", "indexing", "pending", "running", "retrying"].includes(job.state)))) return;
    let timer = 0;
    let stopped = false;
    const refresh = async () => {
      try { await loadBase(selectedId); } catch { /* Keep the last known task state during transient failures. */ }
      if (!stopped) timer = window.setTimeout(() => void refresh(), 1500);
    };
    timer = window.setTimeout(() => void refresh(), 1500);
    return () => { stopped = true; window.clearTimeout(timer); };
  }, [jobs, selectedId]);

  const createBase = async () => {
    const name = createName.trim();
    if (!name) return;
    setBusy(true);
    try {
      const base = await desktop.createKnowledgeBase({ name, description: createDescription.trim(), library_type: createType, tags: createTags.split(",").map((tag) => tag.trim()).filter(Boolean), visibility: "private", embedding_model: createEmbedding.trim(), chunk_strategy: createChunkStrategy });
      await reload(base.id);
      setCreateOpen(false); setCreateName(""); setCreateDescription(""); setCreateTags(""); setCreateEmbedding("");
      setMessage("知识库已创建");
    } catch (reason) {
      setMessage(reason instanceof Error ? reason.message : "创建知识库失败");
    } finally {
      setBusy(false);
    }
  };
  const updateImportQueueItem = (path: string, patch: Partial<ImportQueueItem>) => {
    setImportQueue((items) => items.map((item) => item.path === path ? { ...item, ...patch } : item));
  };
  const submitImportFiles = async (paths: string[]) => {
    if (!selectedId) { setMessage("请先选择知识库"); return; }
    const uniquePaths = Array.from(new Set(paths.map((path) => path.trim()).filter(Boolean)));
    const accepted = uniquePaths.filter((path) => IMPORT_EXTENSIONS.has(path.split(".").pop()?.toLocaleLowerCase() || ""));
    if (!accepted.length) { setMessage("没有可导入的支持格式文件"); return; }
    const ignored = uniquePaths.length - accepted.length;
    setImportQueue(accepted.map((path) => ({ path, name: path.split(/[\\/]/).pop() || path, state: "pending" })));
    setBusy(true);
    let submitted = 0;
    let failed = 0;
    const submissions = accepted.map(async (path) => {
      updateImportQueueItem(path, { state: "submitting" });
      try {
        await desktop.importPostgresDocument({ knowledge_base_id: selectedId, source_path: path });
        updateImportQueueItem(path, { state: "submitted" });
        submitted += 1;
      } catch (reason) {
        updateImportQueueItem(path, { state: "failed", error: reason instanceof Error ? reason.message : "提交失败" });
        failed += 1;
      }
    });
    void reload(selectedId).catch(() => undefined);
    await Promise.all(submissions);
    try { await reload(selectedId); } catch { /* Keep the per-file queue visible when refresh fails. */ }
    setBusy(false);
    setMessage(`已提交 ${submitted} 个文档处理任务${failed ? `，${failed} 个失败` : ""}${ignored ? `，忽略 ${ignored} 个不支持的文件` : ""}`);
  };
  const importFiles = async () => {
    if (!selectedId) { setMessage("请先选择知识库"); return; }
    const chosen = await desktop.openFileDialog({ directory: false, multiple: true, title: "导入文档", filters: [{ name: "文档", extensions: ["pdf", "docx", "xls", "xlsx", "csv", "txt", "md", "markdown", "json", "html", "htm"] }] });
    const files = Array.isArray(chosen) ? chosen : chosen ? [chosen] : [];
    if (!files.length) return;
    await submitImportFiles(files);
  };
  const onDropFiles = async (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    if (busy || libraryTab === "public") return;
    const paths = Array.from(event.dataTransfer.files).map((file) => {
      const candidate = file as File & { path?: string };
      return candidate.path || "";
    });
    if (paths.some((path) => !path)) { setMessage("无法读取拖拽文件路径，请使用导入文档按钮选择文件"); return; }
    await submitImportFiles(paths);
  };
  const importFolder = async () => {
    if (!selectedId) { setMessage("请先选择知识库"); return; }
    const chosen = await desktop.openFileDialog({ directory: true, multiple: false, title: "导入文件夹" });
    const directory = Array.isArray(chosen) ? chosen[0] : chosen;
    if (!directory) return;
    setBusy(true);
    try { const result = await desktop.importPostgresDirectory(selectedId, directory); await reload(selectedId); setMessage("已提交 " + result.length + " 个文档处理任务"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "文件夹导入失败"); }
    finally { setBusy(false); }
  };
  const openDocument = async (item: SourceDocumentRecord, sourceLocation?: Record<string, unknown>) => {
    setDoc(item);
    const requestedPage = sourceLocation?.kind === "pdf_page" ? Number(sourceLocation.page) : NaN;
    setPreviewPage(Number.isInteger(requestedPage) && requestedPage > 0 ? requestedPage : null);
    try {
      const [result, versions] = await Promise.all([desktop.getKnowledgeDocumentPreview(item.id), desktop.listDocumentVersions(item.id)]);
      setPreview(result);
      setDocumentVersions(versions);
    } catch { setPreview({ processed: false, blocks: [], content: "文档预览暂不可用" }); setDocumentVersions([]); }
  };
  const saveDocumentMetadata = async (metadata: Record<string, unknown>) => {
    if (!doc || libraryTab === "public") return;
    try {
      await desktop.updatePostgresDocumentMetadata(doc.id, metadata);
      setPreview((current) => current ? { ...current, metadata } : current);
      setDocuments((items) => items.map((item) => item.id === doc.id ? { ...item, metadata } : item));
      setMessage("文档元数据已保存");
    } catch (reason) { setMessage(reason instanceof Error ? reason.message : "保存文档元数据失败"); }
  };
  const copyPreview = async () => {
    if (!preview?.content) return;
    await navigator.clipboard.writeText(preview.content);
    setMessage("文档内容已复制");
  };
  const addPreviewToChat = async () => {
    if (!preview?.content) return;
    window.dispatchEvent(new CustomEvent("suna:knowledge-context", { detail: { content: `请基于以下文档内容回答我的问题：\n\n${preview.content}` } }));
    setMessage("文档内容已加入 Agent 对话");
  };
  const downloadDocument = async (item: SourceDocumentRecord) => {
    try {
      const destination = await desktop.saveFileDialog({ defaultPath: item.display_name, filters: [{ name: "原始文件", extensions: [item.display_name.split(".").pop() || "bin"] }] });
      if (!destination) return;
      await desktop.exportPostgresDocument(item.id, destination);
      setMessage("原始文档已导出");
    } catch (reason) { setMessage(reason instanceof Error ? reason.message : "下载失败"); }
  };
  const reparseDocument = async (item: SourceDocumentRecord) => {
    setBusy(true);
    try { await desktop.reparsePostgresDocument(item.id); await reload(selectedId || undefined); setMessage("已重新提交文档解析"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "重新解析失败"); }
    finally { setBusy(false); }
  };
  const moveDocument = async () => {
    if (!moveTarget || !moveBaseId || moveBaseId === moveTarget.knowledge_base_id) return;
    try { await desktop.movePostgresDocument(moveTarget.id, moveBaseId); await reload(selectedId || undefined); setMessage("文档已移动"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "移动文档失败"); }
    finally { setMoveTarget(null); }
  };
  const openEmbed = async (item: SourceDocumentRecord) => {
    if (!item.active_version_id) { setMessage("文档还没有可向量化的激活版本"); return; }
    try { const profiles = await desktop.listProviderProfiles(); setEmbedProfiles(profiles.filter((profile) => profile.enabled)); setEmbedProfileId(profiles.find((profile) => profile.enabled)?.id || ""); setEmbedTarget(item); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "读取 Embedding 配置失败"); }
  };
  const embedDocument = async () => {
    if (!embedTarget?.active_version_id || !embedProfileId) return;
    setBusy(true);
    try { await desktop.embedPostgresDocument({ version_id: embedTarget.active_version_id, embedding_profile_id: embedProfileId }); await reload(selectedId || undefined); setMessage("已重新生成 Embedding"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "重新 Embedding 失败"); }
    finally { setBusy(false); setEmbedTarget(null); }
  };
  const renameDocument = async (item: SourceDocumentRecord) => {
    setRenameTarget(item); setRenameValue(item.display_name);
  };
  const deleteDocument = async (item: SourceDocumentRecord) => {
    setDeleteTarget(item);
  };
  const confirmRename = async () => {
    if (!renameTarget) return;
    const value = renameValue.trim();
    if (!value || value === renameTarget.display_name) { setRenameTarget(null); return; }
    try { await desktop.renameKnowledgeDocument(renameTarget.id, value); await reload(selectedId || undefined); setMessage("文档已重命名"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "重命名失败"); }
    finally { setRenameTarget(null); }
  };
  const confirmDelete = async () => {
    if (!deleteTarget) return;
    try { await desktop.deleteKnowledgeDocument(deleteTarget.id); await reload(selectedId || undefined); setMessage("文档已删除"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "删除失败"); }
    finally { setDeleteTarget(null); }
  };
  const createPage = async () => {
    if (!selectedId) return;
    setWikiCreateOpen(true);
  };
  const confirmCreatePage = async () => {
    if (!selectedId) return;
    const title = wikiTitle.trim();
    if (!title) return;
    const created = await desktop.createPostgresWikiPage({ knowledge_base_id: selectedId, slug: "page-" + Date.now(), title, body_markdown: "# " + title + "\n\n开始记录研究内容。", tags: [] });
    setPages((items) => items.concat(created)); setPage(created); setWikiTags([]); setWikiSourceDocumentId(null); setRevisions([]); setWikiCreateOpen(false); setWikiTitle("新的研究笔记");
  };
  const selectPage = async (item: PostgresWikiPage) => {
    setPage(item); setWikiSourceDocumentId(item.source_document_id); setRevisions(await desktop.listPostgresWikiRevisions(item.id));
    try { setWikiTags((await desktop.listPostgresWikiPageTags(item.id)).map((tag) => tag.name)); } catch { setWikiTags([]); }
  };
  const savePage = async () => {
    if (!page) return;
    const saved = await desktop.updatePostgresWikiPage(page.id, page.title, page.body_markdown, wikiTags, wikiSourceDocumentId);
    setPages((items) => items.map((item) => item.id === saved.id ? saved : item));
    setPage(saved); setWikiSourceDocumentId(saved.source_document_id); setRevisions(await desktop.listPostgresWikiRevisions(saved.id)); setMessage("Wiki 页面已保存");
  };
  const restore = async (revision: number) => {
    if (!page) return;
    const restored = await desktop.restorePostgresWikiRevision(page.id, revision);
    setPage(restored); setWikiSourceDocumentId(restored.source_document_id); setPages((items) => items.map((item) => item.id === restored.id ? restored : item));
    setRevisions(await desktop.listPostgresWikiRevisions(restored.id)); setMessage("已恢复到 v" + revision);
  };
  const runSearch = async () => {
    if (!selectedId || !searchQuery.trim()) return;
    setSearchBusy(true); setMessage(null);
    const year = Number.parseInt(searchFilters.year, 10);
    try {
      const pack: EvidencePack = await desktop.queryLocalKnowledge({
        query: searchQuery.trim(),
        knowledge_base_ids: [selectedId],
        lexical_limit: 60,
        dense_limit: 60,
        candidate_limit: 20,
        rrf_k: 60,
        rerank_limit: 20,
        filters: {
          document_type: searchFilters.document_type || undefined,
          material: searchFilters.material || undefined,
          process: searchFilters.process || undefined,
          property: searchFilters.property || undefined,
          year: Number.isFinite(year) ? year : undefined,
          tag: searchFilters.tag || undefined,
        },
      });
      setSearchAuditId(pack.id);
      setSearchEmbeddingDegradation(pack.configuration.embedding_degradation ?? null);
      setSearchRerankDegradation(pack.configuration.rerank_degradation ?? null);
      setSearchResults(pack.evidence.map((item) => ({
        knowledge_base_id: item.chunk.knowledge_base_id,
        chunk_id: item.chunk.chunk_id,
        version_id: item.chunk.version_id,
        document_id: item.chunk.document_id,
        document_name: item.chunk.source_name,
        text: item.chunk.text,
        source_location: item.chunk.source_location as Record<string, unknown>,
        rank: item.chunk.rerank_score ?? item.chunk.rrf_score,
        citation_number: item.citation_number,
      })));
    }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "知识检索失败"); }
    finally { setSearchBusy(false); }
  };
  const openSearchHit = async (hit: PostgresKnowledgeSearchHit) => {
    const document = documents.find((item) => item.id === hit.document_id);
    if (document) await openDocument(document, hit.source_location);
    else setMessage("来源文档已不在当前知识库中");
  };
  const copySearchHit = async (hit: PostgresKnowledgeSearchHit) => {
    await navigator.clipboard.writeText(hit.text);
    setMessage("知识片段已复制");
  };
  const addSearchHitToChat = (hit: PostgresKnowledgeSearchHit) => {
    window.dispatchEvent(new CustomEvent("suna:knowledge-context", { detail: { content: `请基于以下知识片段回答我的问题：\n\n来源：${hit.document_name}\n位置：${formatSourceLocation(hit.source_location)}\n\n${hit.text}` } }));
    setMessage("知识片段已加入 Agent 对话");
  };
  const toggleSearchFavorite = (hit: PostgresKnowledgeSearchHit) => {
    const next = favoriteChunkIds.includes(hit.chunk_id) ? favoriteChunkIds.filter((id) => id !== hit.chunk_id) : [...favoriteChunkIds, hit.chunk_id];
    setFavoriteChunkIds(next);
    void desktop.setSetting("knowledge.favorites", JSON.stringify({ version: 1, chunk_ids: next })).catch(() => setMessage("保存收藏失败"));
    setMessage(next.includes(hit.chunk_id) ? "知识片段已收藏" : "已取消收藏");
  };
  const citeSearchHit = async (hit: PostgresKnowledgeSearchHit) => {
    if (!searchAuditId || !hit.citation_number) {
      setMessage("当前结果没有可验证的引用编号");
      return;
    }
    try {
      const citation = await desktop.resolvePostgresCitation(searchAuditId, hit.citation_number);
      if (!citation) {
        setMessage("引用来源已不可用");
        return;
      }
      await navigator.clipboard.writeText(`来源：${citation.hit.document_name}，${formatSourceLocation(citation.hit.source_location)}\n${citation.hit.text}`);
      setMessage("已复制可验证引用");
    } catch (reason) { setMessage(reason instanceof Error ? reason.message : "读取引用失败"); }
  };
  const retryJob = async (job: PostgresIngestionJob) => {
    setBusy(true); setMessage(null);
    try { await desktop.retryPostgresIngestionJob(job.id); await reload(selectedId || undefined); setMessage("已重新提交处理任务"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "任务重试失败"); }
    finally { setBusy(false); }
  };
  const cancelJob = async (job: PostgresIngestionJob) => {
    setBusy(true); setMessage(null);
    try { await desktop.cancelPostgresIngestionJob(job.id); await reload(selectedId || undefined); setMessage("已取消处理任务"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "任务取消失败"); }
    finally { setBusy(false); }
  };
  const selected = bases.find((item) => item.id === selectedId);
  const saveSettings = async (input: { description: string; library_type: string; tags: string[]; visibility: string; embedding_model: string; chunk_strategy: string }) => {
    if (!selected) return;
    try { const updated = await desktop.updateKnowledgeBaseSettings(selected.id, { name: selected.name, ...input }); setBases((items) => items.map((item) => item.id === updated.id ? updated : item)); setMessage("知识库设置已保存"); }
    catch (reason) { setMessage(reason instanceof Error ? reason.message : "保存知识库设置失败"); }
  };
  const filtered = documents
    .filter((item) => !debouncedQuery || [item.display_name, item.source_kind].some((value) => value.toLocaleLowerCase().includes(debouncedQuery)))
    .filter((item) => typeFilter === "全部类型" || item.source_kind.toLowerCase().includes(typeFilter.toLowerCase()))
    .sort((a, b) => sortMode === "名称" ? a.display_name.localeCompare(b.display_name) : b.updated_at.localeCompare(a.updated_at));
  const cardStyles = [["#e6f1ff", "#146bff"], ["#e4faf7", "#08a69d"], ["#f1eaff", "#7237dd"], ["#fff0df", "#f07814"], ["#e1faef", "#0ba66f"], ["#e6f1ff", "#146bff"]] as const;
  const displayBases = bases
    .filter((base) => !debouncedQuery || [base.name, base.description || "", ...(base.tags || [])].some((value) => value.toLocaleLowerCase().includes(debouncedQuery)))
    .map((base, index) => ({ base, title: base.name, description: base.description || "暂无知识库描述", tint: cardStyles[index % cardStyles.length][0], accent: cardStyles[index % cardStyles.length][1] }));
  const visibleBases = libraryTab === "public" ? displayBases.filter(({ base }) => base.visibility === "public") : displayBases;
  const switchLibraryTab = (next: LibraryTab) => {
    setLibraryTab(next);
    setTab("documents");
    if (next === "public") {
      setSelectedId(null);
      setDocuments([]);
      setJobs([]);
      setChunks([]);
      setPages([]);
      setEdges([]);
    } else if (next === "mine") {
      setSelectedId(bases[0]?.id || null);
    }
  };
  const formatDate = (value: string) => value ? new Date(value).toLocaleString("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }) : "—";
  const formatSize = (bytes = 0) => bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(0)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  const extension = (name: string) => name.split(".").pop()?.toUpperCase() || "DOC";
  return <section className="suna-knowledge-center">
    <header className="suna-knowledge-center-header"><div><span className="suna-module-kicker">SUNA RESEARCH PLATFORM</span><h1><BookOpen size={23} />知识中心</h1><p>管理钢铁材料研发知识</p></div></header>
    {health && <div className="suna-knowledge-health-strip" aria-label="知识中心统计"><span><strong>{health.knowledge_base_count}</strong>知识库</span><span><strong>{health.document_count}</strong>文档</span><span><strong>{health.chunk_count}</strong>片段</span><span><strong>{health.indexed_chunk_count}</strong>已向量化</span><span><strong>{health.retrieval_count ?? 0}</strong>次检索</span><span><strong>{health.average_retrieval_duration_ms == null ? "—" : `${Math.round(health.average_retrieval_duration_ms)} ms`}</strong>平均检索</span></div>}
    {message && <div className="suna-knowledge-center-alert">{message}{loadFailed && <button className="suna-ghost-button" onClick={() => void reload().catch(() => undefined)}>重新加载</button>}<button onClick={() => setMessage(null)} aria-label="关闭"><X size={14} /></button></div>}
    <main className="suna-knowledge-main"><div className="suna-knowledge-library-tabs"><button className={libraryTab === "mine" ? "is-active" : ""} onClick={() => switchLibraryTab("mine")}>我的知识库</button><button className={libraryTab === "public" ? "is-active" : ""} onClick={() => switchLibraryTab("public")}>公共知识库</button><button className={libraryTab === "manage" ? "is-active" : ""} onClick={() => switchLibraryTab("manage")}>知识库管理</button></div>
      <div className="suna-knowledge-tools"><button className="suna-ghost-button" onClick={() => setTab("chunks")}><Database size={15} />知识片段</button><button className="suna-ghost-button" onClick={() => setTab("search")}><Search size={15} />知识检索</button><button className="suna-ghost-button" onClick={() => setTab("graph")}><Network size={15} />知识图谱</button><button className="suna-ghost-button" onClick={() => setTab("wiki")}><BookOpen size={15} />Wiki</button><button className="suna-ghost-button" onClick={() => setTab("settings")} disabled={!selected}>设置</button></div>{libraryTab === "manage" && <KnowledgeBaseManagement bases={bases} onRefresh={() => void reload(selectedId || undefined)} />}
      {tab === "documents" && libraryTab !== "manage" && <><div className="suna-knowledge-toolbar"><label className="suna-knowledge-search"><Search size={18} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索知识库、文档、标签..." /></label><label className="suna-knowledge-filter"><SlidersHorizontal size={16} /><select value={typeFilter} onChange={(event) => setTypeFilter(event.target.value)}><option>全部类型</option>{Array.from(new Set(documents.map((item) => item.source_kind))).map((kind) => <option key={kind} value={kind}>{kind}</option>)}</select><ChevronDown size={14} /></label><label className="suna-knowledge-filter"><Clock3 size={16} /><select value={sortMode} onChange={(event) => setSortMode(event.target.value)}><option>最近更新</option><option>名称</option></select><ChevronDown size={14} /></label><button className="suna-ghost-button" onClick={() => void importFolder()} disabled={busy || libraryTab === "public"}><Database size={15} />导入文件夹</button><button className="suna-ghost-button" onClick={() => void importFiles()} disabled={busy || libraryTab === "public"}><Upload size={15} />导入文档</button><button className="suna-primary-button" onClick={() => setCreateOpen(true)} disabled={libraryTab === "public"}><Plus size={16} />新建知识库</button></div><div className={"suna-knowledge-dropzone" + (libraryTab === "public" ? " is-disabled" : "")} onDragOver={(event) => event.preventDefault()} onDrop={(event) => void onDropFiles(event)} role="region" aria-label="拖拽导入文档"><Upload size={18} /><span>拖拽文档到这里，或使用“导入文档”选择文件</span><small>支持 PDF、DOCX、XLSX、XLS、CSV、TXT、Markdown、JSON、HTML</small></div>{importQueue.length > 0 && <ImportQueuePanel items={importQueue} busy={busy} onClear={() => setImportQueue([])} />}{loading ? <div className="suna-knowledge-loading" aria-label="正在加载知识库"><span /><span /><span /></div> : visibleBases.length ? <div className="suna-knowledge-library-grid">{visibleBases.map(({ base, title, description, tint, accent }) => <button key={base.id} className={base.id === selectedId ? "suna-knowledge-library-card is-active" : "suna-knowledge-library-card"} onClick={() => setSelectedId(base.id)}><span className="suna-knowledge-library-icon" style={{ background: tint, color: accent }}><BookOpen size={28} /></span><span className="suna-knowledge-library-copy"><strong>{title}</strong><small>{description}</small></span><ArrowRight size={18} className="suna-knowledge-library-arrow" /><span className="suna-knowledge-library-meta"><span><FileText size={15} />文档 {documentCounts[base.id] ?? 0}</span><span><Clock3 size={15} />更新于 {new Date(base.updated_at).toLocaleDateString("zh-CN")}</span></span></button>)}</div> : <KnowledgeBaseEmpty publicTab={libraryTab === "public"} onCreate={() => setCreateOpen(true)} />}{!loading && libraryTab !== "public" && <section className="suna-knowledge-recent"><div className="suna-knowledge-recent-head"><h2><Clock3 size={19} />最近文档</h2><button onClick={() => { setQuery(""); setTypeFilter("全部类型"); setSortMode("最近更新"); }}>查看更多 <ArrowRight size={15} /></button></div>{filtered.length ? <div className="suna-knowledge-table-wrap"><table><thead><tr><th>文档名称</th><th>所属知识库</th><th>更新时间</th><th>大小</th><th>状态</th><th /></tr></thead><tbody>{filtered.slice(0, 6).map((item) => <tr key={item.id} onClick={() => void openDocument(item)}><td><span className={"suna-file-type suna-file-type-" + extension(item.display_name).toLowerCase()}>{extension(item.display_name).slice(0, 3)}</span>{item.display_name}</td><td>{bases.find((base) => base.id === item.knowledge_base_id)?.name || "当前知识库"}</td><td>{formatDate(item.updated_at)}</td><td>{formatSize(item.file_size)}</td><td><span className={item.active_version_id ? "suna-doc-state is-ready" : "suna-doc-state"}>{item.active_version_id ? "已索引" : "处理中"}</span></td><td><button onClick={(event) => { event.stopPropagation(); void renameDocument(item); }} aria-label="重命名"><Pencil size={14} /></button><button onClick={(event) => { event.stopPropagation(); deleteDocument(item); }} aria-label="删除">×</button></td></tr>)}</tbody></table></div> : <Empty onImport={() => void importFiles()} />}</section>}{!loading && libraryTab !== "public" && <IngestionJobs jobs={jobs} documents={documents} busy={busy} onRetry={(job) => void retryJob(job)} onCancel={(job) => void cancelJob(job)} />}</>}
      {tab === "search" && <KnowledgeSearchPanel query={searchQuery} filters={searchFilters} documentTypes={Array.from(new Set(documents.map((item) => item.source_kind)))} results={searchResults} favoriteChunkIds={favoriteChunkIds} busy={searchBusy} hasKnowledgeBase={Boolean(selectedId)} embeddingDegradation={searchEmbeddingDegradation} rerankDegradation={searchRerankDegradation} onQueryChange={setSearchQuery} onFiltersChange={setSearchFilters} onSearch={() => void runSearch()} onOpen={openSearchHit} onCopy={copySearchHit} onAddToChat={addSearchHitToChat} onToggleFavorite={toggleSearchFavorite} onCite={citeSearchHit} />}
      {tab === "chunks" && <ChunkPanel chunks={chunks} />}
      {tab === "wiki" && <WikiPanel pages={pages} page={page} revisions={revisions} documents={documents} tags={wikiTags} availableTags={availableWikiTags} sourceDocumentId={wikiSourceDocumentId} onTagsChange={setWikiTags} onSourceDocumentChange={setWikiSourceDocumentId} onCreate={() => void createPage()} onSelect={(item) => void selectPage(item)} onChange={setPage} onSave={() => void savePage()} onRestore={(revision) => void restore(revision)} />}
      {tab === "graph" && <GraphPanel knowledgeBaseId={selectedId} edges={edges} pages={pages} readOnly={libraryTab === "public"} onChanged={() => void loadBase(selectedId)} onMessage={setMessage} />}
      {tab === "settings" && selected && <KnowledgeSettingsPanel base={selected} onSave={(input) => void saveSettings(input)} />}
    </main>
    {createOpen && <div className="suna-knowledge-modal-overlay" onClick={() => setCreateOpen(false)}><form className="suna-knowledge-modal" onSubmit={(event) => { event.preventDefault(); void createBase(); }} onClick={(event) => event.stopPropagation()}><header><div><strong>新建知识库</strong><span>创建后可以导入文档并建立检索索引</span></div><button type="button" onClick={() => setCreateOpen(false)} aria-label="关闭"><X size={17} /></button></header><label>知识库名称<input autoFocus value={createName} onChange={(event) => setCreateName(event.target.value)} placeholder="例如：高强钢研究" required maxLength={200} /></label><label>描述<textarea value={createDescription} onChange={(event) => setCreateDescription(event.target.value)} placeholder="说明知识库的研究范围" rows={2} /></label><div className="suna-knowledge-modal-grid"><label>类型<select value={createType} onChange={(event) => setCreateType(event.target.value)}><option value="research">研究资料</option><option value="standard">标准规范</option><option value="experiment">实验记录</option><option value="other">其他</option></select></label><label>存储范围<input value="仅本机当前用户" readOnly /></label></div><label>标签<input value={createTags} onChange={(event) => setCreateTags(event.target.value)} placeholder="钢铁, 工艺, 文献（逗号分隔）" /></label><div className="suna-knowledge-modal-grid"><label>Embedding 模型<input value={createEmbedding} onChange={(event) => setCreateEmbedding(event.target.value)} placeholder="使用默认配置" /></label><label>Chunk 策略<select value={createChunkStrategy} onChange={(event) => setCreateChunkStrategy(event.target.value)}><option value="semantic">语义切分</option><option value="fixed">固定长度</option><option value="heading">按标题</option></select></label></div><footer><button type="button" className="suna-ghost-button" onClick={() => setCreateOpen(false)}>取消</button><button type="submit" className="suna-primary-button"><Plus size={15} />创建知识库</button></footer></form></div>}
    {renameTarget && <KnowledgeActionModal title="重命名文档" onClose={() => setRenameTarget(null)}><label>文档名称<input autoFocus value={renameValue} onChange={(event) => setRenameValue(event.target.value)} /></label><footer><button className="suna-ghost-button" onClick={() => setRenameTarget(null)}>取消</button><button className="suna-primary-button" onClick={() => void confirmRename()}>保存</button></footer></KnowledgeActionModal>}
    {wikiCreateOpen && <KnowledgeActionModal title="新建 Wiki 页面" onClose={() => setWikiCreateOpen(false)}><label>页面标题<input autoFocus value={wikiTitle} onChange={(event) => setWikiTitle(event.target.value)} /></label><footer><button className="suna-ghost-button" onClick={() => setWikiCreateOpen(false)}>取消</button><button className="suna-primary-button" onClick={() => void confirmCreatePage()}>创建</button></footer></KnowledgeActionModal>}
    {deleteTarget && <KnowledgeActionModal title="删除文档" onClose={() => setDeleteTarget(null)}><p>确认删除“{deleteTarget.display_name}”？历史版本将保留。</p><footer><button className="suna-ghost-button" onClick={() => setDeleteTarget(null)}>取消</button><button className="suna-danger-button" onClick={() => void confirmDelete()}>确认删除</button></footer></KnowledgeActionModal>}
    {doc && <div className="suna-knowledge-drawer-overlay" onClick={() => { setDoc(null); setPreviewPage(null); }}><aside className="suna-knowledge-drawer" onClick={(event) => event.stopPropagation()}><header><strong>{doc.display_name}</strong><div><button className="suna-ghost-button" onClick={() => void reparseDocument(doc)} disabled={busy}>重新解析</button><button className="suna-ghost-button" onClick={() => void openEmbed(doc)} disabled={!doc.active_version_id}>重新 Embedding</button><button className="suna-ghost-button" onClick={() => { setMoveTarget(doc); setMoveBaseId(bases.find((base) => base.id !== doc.knowledge_base_id)?.id || ""); }}>移动</button><button className="suna-ghost-button" onClick={() => void downloadDocument(doc)}>下载</button><button onClick={() => { setDoc(null); setPreviewPage(null); }} aria-label="关闭"><X size={17} /></button></div></header><div className="suna-knowledge-drawer-body"><div className="suna-document-preview-toolbar"><div><h3>文档预览</h3><small>{preview?.mime_type || extension(doc.display_name)} · {preview?.updated_at ? formatDate(preview.updated_at) : "未处理"}</small></div><div><button className="suna-ghost-button" onClick={() => void copyPreview()} disabled={!preview?.content}><Copy size={14} />复制</button><button className="suna-ghost-button" onClick={() => void addPreviewToChat()} disabled={!preview?.content}><MessageSquarePlus size={14} />加入对话</button></div></div>{preview?.processed ? <>{preview.raw_sheets?.length ? <SheetPreview sheets={preview.raw_sheets} /> : preview.mime_type?.includes("wordprocessingml") && preview.structured_blocks?.length ? <StructuredDocumentPreview blocks={preview.structured_blocks} /> : preview.blocks?.length ? <div className="suna-document-preview-blocks">{preview.blocks.map((block) => <article key={block.ordinal ?? block.content.slice(0, 24)}><p>{block.content}</p>{block.source_location && <small>来源位置：{JSON.stringify(block.source_location)}</small>}</article>)}</div> : <pre>{preview.content || "暂无文本预览"}</pre>}</> : <div className="suna-knowledge-document-empty"><Info size={24} /><h3>{preview?.content || "文档尚未完成处理"}</h3></div>}<DocumentPagesPanel pages={preview?.pages} initialPage={previewPage} /><DocumentMetadataPanel sourcePath={preview?.source_path} metadata={preview?.metadata} readOnly={libraryTab === "public"} onSave={(metadata) => void saveDocumentMetadata(metadata)} /><div className="suna-document-version-list"><strong>版本历史</strong>{documentVersions.length ? documentVersions.map((version) => <span key={version.id}>v{version.created_at} · {version.expected_chunk_count} 个片段{version.activated_at ? " · 当前激活" : ""}</span>) : <small>暂无版本记录</small>}</div></div></aside></div>}
    {moveTarget && <KnowledgeActionModal title="移动文档" onClose={() => setMoveTarget(null)}><label>目标知识库<select value={moveBaseId} onChange={(event) => setMoveBaseId(event.target.value)}>{bases.filter((base) => base.id !== moveTarget.knowledge_base_id).map((base) => <option key={base.id} value={base.id}>{base.name}</option>)}</select></label><footer><button className="suna-ghost-button" onClick={() => setMoveTarget(null)}>取消</button><button className="suna-primary-button" onClick={() => void moveDocument()} disabled={!moveBaseId}>移动</button></footer></KnowledgeActionModal>}
    {embedTarget && <KnowledgeActionModal title="重新 Embedding" onClose={() => setEmbedTarget(null)}><label>Embedding Provider<select value={embedProfileId} onChange={(event) => setEmbedProfileId(event.target.value)}>{embedProfiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.display_name}{profile.model_id ? " · " + profile.model_id : ""}</option>)}</select></label><footer><button className="suna-ghost-button" onClick={() => setEmbedTarget(null)}>取消</button><button className="suna-primary-button" onClick={() => void embedDocument()} disabled={!embedProfileId || busy}>开始</button></footer></KnowledgeActionModal>}
  </section>;
}
function SheetPreview({ sheets }: { sheets: import("../../bridge/desktop").KnowledgePreviewSheet[] }) {
  const [selected, setSelected] = useState(sheets[0]?.name || "");
  useEffect(() => { if (!sheets.some((sheet) => sheet.name === selected)) setSelected(sheets[0]?.name || ""); }, [sheets, selected]);
  const sheet = sheets.find((item) => item.name === selected);
  return <div className="suna-document-preview-sheets"><label>工作表<select aria-label="选择工作表" value={selected} onChange={(event) => setSelected(event.target.value)}>{sheets.map((item) => <option key={item.name} value={item.name}>{item.name}</option>)}</select></label>{sheet && <section><h4>{sheet.name}</h4><div dangerouslySetInnerHTML={{ __html: sheet.html }} />{sheet.truncated && <small>预览仅显示前 100 行和 40 列，完整内容仍保存在原始文件中。</small>}</section>}</div>;
}

function StructuredDocumentPreview({ blocks }: { blocks: import("../../bridge/desktop").KnowledgeStructuredPreviewBlock[] }) {
  return <div className="suna-document-structured">{blocks.map((block, index) => <article key={index}>
    {block.kind === "heading" && <h3 style={{ fontSize: Math.max(14, 20 - (block.level || 1) * 2) }}>{block.text}</h3>}
    {block.kind === "paragraph" && <p>{block.text}</p>}
    {block.kind === "formula" && <pre>{block.text}</pre>}
    {block.kind === "image" && <p className="suna-document-image-alt">图片：{block.text || "未提供说明"}</p>}
    {block.kind === "list" && (block.ordered ? <ol>{block.items?.map((item, itemIndex) => <li key={itemIndex}>{item}</li>)}</ol> : <ul>{block.items?.map((item, itemIndex) => <li key={itemIndex}>{item}</li>)}</ul>)}
    {block.kind === "table" && <div className="suna-document-structured-table"><table><tbody>{block.rows?.map((row, rowIndex) => <tr key={rowIndex}>{row.map((cell, cellIndex) => rowIndex === 0 ? <th key={cellIndex}>{cell}</th> : <td key={cellIndex}>{cell}</td>)}</tr>)}</tbody></table></div>}
  </article>)}</div>;
}

function DocumentPagesPanel({ pages, initialPage }: { pages?: import("../../bridge/desktop").KnowledgePreviewPage[]; initialPage?: number | null }) {
  if (!pages?.length) return null;
  return <PagePreview pages={pages} initialPage={initialPage} />;
}

function PagePreview({ pages, initialPage }: { pages: import("../../bridge/desktop").KnowledgePreviewPage[]; initialPage?: number | null }) {
  const firstPage = initialPage && pages.some((item) => item.page === initialPage) ? initialPage : pages[0]?.page || 1;
  const [pageNumber, setPageNumber] = useState(firstPage);
  useEffect(() => {
    const requested = initialPage && pages.some((item) => item.page === initialPage) ? initialPage : null;
    if (requested) setPageNumber(requested);
    else if (!pages.some((item) => item.page === pageNumber)) setPageNumber(pages[0]?.page || 1);
  }, [pages, initialPage, pageNumber]);
  const page = pages.find((item) => item.page === pageNumber);
  return <section className="suna-document-pages"><div><strong>PDF 分页预览</strong><label>页码<select aria-label="选择 PDF 页码" value={pageNumber} onChange={(event) => setPageNumber(Number(event.target.value))}>{pages.map((item) => <option key={item.page} value={item.page}>第 {item.page} 页</option>)}</select></label></div>{page && <article><h4>第 {page.page} 页</h4><p>{page.content}</p></article>}</section>;
}

function DocumentMetadataPanel({ sourcePath, metadata, readOnly, onSave }: { sourcePath?: string | null; metadata?: Record<string, unknown>; readOnly: boolean; onSave: (metadata: Record<string, unknown>) => void }) {
  const [value, setValue] = useState("");
  useEffect(() => setValue(JSON.stringify(metadata || {}, null, 2)), [metadata]);
  const save = () => {
    try {
      const parsed = JSON.parse(value);
      if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") throw new Error();
      onSave(parsed as Record<string, unknown>);
    } catch { /* keep invalid JSON visible for correction */ }
  };
  return <section className="suna-document-metadata"><strong>文档元数据</strong>{sourcePath && <small>来源：{sourcePath}</small>}<textarea aria-label="文档元数据 JSON" value={value} onChange={(event) => setValue(event.target.value)} readOnly={readOnly} rows={5} /><button className="suna-ghost-button" onClick={save} disabled={readOnly}>保存元数据</button></section>;
}
function Empty({ onImport }: { onImport: () => void }) { return <div className="suna-knowledge-document-empty"><FileText size={27} /><h3>这个知识库还没有文档</h3><p>导入文档后，Suna 会解析、切块并建立检索索引。</p><button className="suna-primary-button" onClick={onImport}><Upload size={15} />导入文档</button></div>; }
function ImportQueuePanel({ items, busy, onClear }: { items: ImportQueueItem[]; busy: boolean; onClear: () => void }) {
  const completed = items.filter((item) => item.state === "submitted").length;
  const failed = items.filter((item) => item.state === "failed").length;
  return <section className="suna-knowledge-import-queue" aria-label="导入队列"><header><div><strong>导入队列</strong><span>{completed}/{items.length} 个文件已提交{failed ? `，${failed} 个失败` : ""}</span></div><button className="suna-ghost-button" onClick={onClear} disabled={busy}>清空</button></header><div className="suna-knowledge-import-list">{items.map((item) => <div key={item.path}><FileText size={15} /><span title={item.path}>{item.name}</span><small className={`is-${item.state}`}>{item.state === "pending" ? "等待中" : item.state === "submitting" ? "提交中" : item.state === "submitted" ? "已提交" : item.error || "失败"}</small></div>)}</div></section>;
}
function KnowledgeBaseEmpty({ publicTab, onCreate }: { publicTab: boolean; onCreate: () => void }) { return <div className="suna-knowledge-base-empty"><Database size={30} /><h3>{publicTab ? "还没有公共知识库" : "还没有知识库"}</h3><p>{publicTab ? "单用户客户端不提供共享知识库。" : "创建第一个知识库，开始管理钢铁材料研发资料。"}</p>{!publicTab && <button className="suna-primary-button" onClick={onCreate}><Plus size={15} />创建第一个知识库</button>}</div>; }
function WikiPanel({ pages, page, revisions, documents, tags, availableTags, sourceDocumentId, onTagsChange, onSourceDocumentChange, onCreate, onSelect, onChange, onSave, onRestore }: { pages: PostgresWikiPage[]; page: PostgresWikiPage | null; revisions: PostgresWikiRevision[]; documents: SourceDocumentRecord[]; tags: string[]; availableTags: string[]; sourceDocumentId: string | null; onTagsChange: (tags: string[]) => void; onSourceDocumentChange: (id: string | null) => void; onCreate: () => void; onSelect: (page: PostgresWikiPage) => void; onChange: (page: PostgresWikiPage) => void; onSave: () => void; onRestore: (revision: number) => void }) {
  const [tagInput, setTagInput] = useState("");
  const addTag = () => {
    const next = tagInput.trim();
    if (next && !tags.includes(next)) onTagsChange([...tags, next]);
    setTagInput("");
  };
  return <div className="suna-wiki-workspace"><div className="suna-wiki-list"><div className="suna-knowledge-doc-head"><div><h2>Wiki 页面</h2><span>{pages.length} 个页面</span></div><button className="suna-primary-button" onClick={onCreate}><Plus size={14} />新建</button></div>{pages.map((item) => <button key={item.id} className={page?.id === item.id ? "is-active" : ""} onClick={() => onSelect(item)}><BookOpen size={15} /><span>{item.title}</span><small>v{item.revision}</small></button>)}</div><div className="suna-wiki-editor">{page ? <><div className="suna-wiki-editor-head"><input value={page.title} onChange={(event) => onChange({ ...page, title: event.target.value })} /><button className="suna-primary-button" onClick={onSave}><Pencil size={14} />保存</button></div><div className="suna-wiki-editor-meta"><label>来源文档<select aria-label="Wiki 来源文档" value={sourceDocumentId || ""} onChange={(event) => onSourceDocumentChange(event.target.value || null)}><option value="">不关联来源文档</option>{documents.map((document) => <option key={document.id} value={document.id}>{document.display_name}</option>)}</select></label><label>标签<div className="suna-wiki-tag-editor"><input aria-label="Wiki 标签" value={tagInput} onChange={(event) => setTagInput(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); addTag(); } }} placeholder="输入标签后回车" /><button className="suna-ghost-button" onClick={addTag}>添加</button></div><div className="suna-wiki-tags">{tags.map((tag) => <button type="button" key={tag} onClick={() => onTagsChange(tags.filter((item) => item !== tag))}>{tag} ×</button>)}{availableTags.filter((tag) => !tags.includes(tag)).slice(0, 5).map((tag) => <button type="button" key={tag} onClick={() => onTagsChange([...tags, tag])}>+ {tag}</button>)}</div></label></div><textarea value={page.body_markdown} onChange={(event) => onChange({ ...page, body_markdown: event.target.value })} /><div className="suna-wiki-history"><History size={14} /><strong>版本历史</strong>{revisions.map((item) => <button key={item.id} onClick={() => onRestore(item.revision)}>v{item.revision} · {new Date(item.created_at).toLocaleString()}</button>)}</div></> : <div className="suna-knowledge-document-empty"><BookOpen size={25} /><h3>选择一个 Wiki 页面</h3></div>}</div></div>;
}
function GraphPanel({ knowledgeBaseId, edges, pages, readOnly, onChanged, onMessage }: { knowledgeBaseId: string | null; edges: PostgresKnowledgeEdge[]; pages: PostgresWikiPage[]; readOnly: boolean; onChanged: () => void; onMessage: (message: string) => void }) {
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState("");
  const [relation, setRelation] = useState("关联");
  const [saving, setSaving] = useState(false);
  const labels = new Map(pages.map((item) => [item.id, item.title]));
  const connectedIds = new Set<string>();
  edges.forEach((edge) => { if (edge.source_page_id) connectedIds.add(edge.source_page_id); if (edge.target_page_id) connectedIds.add(edge.target_page_id); });
  const nodes = pages.filter((page) => connectedIds.has(page.id) || !edges.length);
  const positions = nodes.map((node, index) => {
    const angle = (Math.PI * 2 * index) / Math.max(nodes.length, 1) - Math.PI / 2;
    return { node, left: 50 + Math.cos(angle) * (nodes.length > 2 ? 34 : 24), top: 50 + Math.sin(angle) * (nodes.length > 2 ? 30 : 18) };
  });
  const point = new Map(positions.map((item) => [item.node.id, item]));
  const createEdge = async () => {
    if (!knowledgeBaseId || !source || !target || source === target || !relation.trim()) { onMessage("请选择不同的源页面和目标页面，并填写关系类型"); return; }
    setSaving(true);
    try {
      await desktop.createPostgresKnowledgeEdge({ knowledge_base_id: knowledgeBaseId, source_page_id: source, target_page_id: target, relation: relation.trim(), metadata: { source: "manual" } });
      setSource(""); setTarget(""); setRelation("关联"); setSelectedEdge(null); onChanged(); onMessage("知识边已创建");
    } catch (reason) { onMessage(reason instanceof Error ? reason.message : "创建知识边失败"); }
    finally { setSaving(false); }
  };
  const removeEdge = async (edge: PostgresKnowledgeEdge) => {
    if (readOnly) return;
    setSaving(true);
    try { await desktop.deletePostgresKnowledgeEdge(edge.id); setSelectedEdge(null); onChanged(); onMessage("知识边已软删除"); }
    catch (reason) { onMessage(reason instanceof Error ? reason.message : "删除知识边失败"); }
    finally { setSaving(false); }
  };
  const selected = selectedEdge ? edges.find((edge) => edge.id === selectedEdge) : null;
  const selectedPage = selectedNode ? pages.find((page) => page.id === selectedNode) : null;
  return <section className="suna-graph-workspace">
    <header className="suna-graph-head"><div><h2><Network size={19} />知识关系图谱</h2><span>{pages.length} 个 Wiki 节点 · {edges.length} 条关系边</span></div><span className="suna-graph-readonly">{readOnly ? "公共知识库 · 只读" : "本机个人知识库"}</span></header>
    {!readOnly && <div className="suna-graph-create"><select aria-label="关系源页面" value={source} onChange={(event) => setSource(event.target.value)}><option value="">选择源页面</option>{pages.map((page) => <option key={page.id} value={page.id}>{page.title}</option>)}</select><input aria-label="关系类型" value={relation} onChange={(event) => setRelation(event.target.value)} placeholder="关系类型" maxLength={100} /><select aria-label="关系目标页面" value={target} onChange={(event) => setTarget(event.target.value)}><option value="">选择目标页面</option>{pages.map((page) => <option key={page.id} value={page.id}>{page.title}</option>)}</select><button className="suna-primary-button" onClick={() => void createEdge()} disabled={saving || !knowledgeBaseId}><Plus size={14} />添加关系</button></div>}
    <div className="suna-graph-layout"><div className="suna-graph-canvas" aria-label="知识关系图谱画布">
      {positions.length ? <><svg className="suna-graph-lines" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">{edges.map((edge) => { const from = edge.source_page_id ? point.get(edge.source_page_id) : undefined; const to = edge.target_page_id ? point.get(edge.target_page_id) : undefined; if (!from || !to) return null; return <line key={edge.id} x1={from.left} y1={from.top} x2={to.left} y2={to.top} className={selectedEdge === edge.id ? "is-selected" : ""} onClick={() => { setSelectedEdge(edge.id); setSelectedNode(null); }} />; })}</svg>{positions.map(({ node, left, top }) => <button key={node.id} className={`suna-graph-node${selectedNode === node.id ? " is-selected" : ""}`} style={{ left: `${left}%`, top: `${top}%` }} onClick={() => { setSelectedNode(node.id); setSelectedEdge(null); }}><BookOpen size={14} /><span>{node.title}</span></button>)}{edges.length > 0 && <div className="suna-graph-edge-labels" aria-label="关系列表">{edges.map((edge) => <button key={edge.id} className={selectedEdge === edge.id ? "is-selected" : ""} onClick={() => { setSelectedEdge(edge.id); setSelectedNode(null); }}><span>{labels.get(edge.source_page_id || "") || "来源"}</span><b>{edge.relation}</b><span>{labels.get(edge.target_page_id || "") || "目标"}</span></button>)}</div>}</> : <div className="suna-graph-empty"><Network size={32} /><strong>还没有可展示的知识关系</strong><span>先创建 Wiki 页面，再通过 Markdown 链接或上方表单建立关系。</span></div>}
    </div><aside className="suna-graph-inspector">{selected ? <><h3>关系详情</h3><dl><dt>源页面</dt><dd>{labels.get(selected.source_page_id || "") || "未命名页面"}</dd><dt>关系类型</dt><dd>{selected.relation}</dd><dt>目标页面</dt><dd>{labels.get(selected.target_page_id || "") || "未命名页面"}</dd><dt>来源</dt><dd>{selected.metadata?.source === "manual" ? "手工创建" : "Wiki 链接"}</dd></dl>{!readOnly && <button className="suna-danger-button" onClick={() => void removeEdge(selected)} disabled={saving}>删除这条关系</button>}</> : selectedPage ? <><h3>节点详情</h3><dl><dt>页面标题</dt><dd>{selectedPage.title}</dd><dt>版本</dt><dd>v{selectedPage.revision}</dd><dt>更新时间</dt><dd>{new Date(selectedPage.updated_at).toLocaleString("zh-CN")}</dd></dl></> : <div className="suna-graph-inspector-empty"><Info size={20} /><span>选择节点或关系查看详情</span></div>}</aside></div>
  </section>;
}

function KnowledgeSettingsPanel({ base, onSave }: { base: KnowledgeBaseRecord; onSave: (input: { description: string; library_type: string; tags: string[]; visibility: string; embedding_model: string; chunk_strategy: string }) => void }) {
  const [description, setDescription] = useState(base.description || "");
  const [libraryType, setLibraryType] = useState(base.library_type || "research");
  const [tags, setTags] = useState((base.tags || []).join(", "));
  const [embedding, setEmbedding] = useState(base.embedding_model || "");
  const [chunkStrategy, setChunkStrategy] = useState(base.chunk_strategy || "semantic");
  return <section className="suna-knowledge-settings"><div className="suna-knowledge-recent-head"><div><h2>知识库设置</h2><span>{base.name} · 本机个人知识库</span></div><button className="suna-primary-button" onClick={() => onSave({ description, library_type: libraryType, visibility: "private", tags: tags.split(",").map((item) => item.trim()).filter(Boolean), embedding_model: embedding, chunk_strategy: chunkStrategy })}>保存设置</button></div><div className="suna-knowledge-settings-grid"><label>描述<textarea rows={4} value={description} onChange={(event) => setDescription(event.target.value)} /></label><label>类型<select value={libraryType} onChange={(event) => setLibraryType(event.target.value)}><option value="research">研究资料</option><option value="standard">标准规范</option><option value="experiment">实验记录</option><option value="other">其他</option></select></label><label>存储范围<input value="仅本机当前用户" readOnly /></label><label>标签<input value={tags} onChange={(event) => setTags(event.target.value)} placeholder="逗号分隔" /></label><label>Embedding 模型<input value={embedding} onChange={(event) => setEmbedding(event.target.value)} placeholder="使用默认配置" /></label><label>Chunk 策略<select value={chunkStrategy} onChange={(event) => setChunkStrategy(event.target.value)}><option value="semantic">语义切分</option><option value="fixed">固定长度</option><option value="heading">按标题</option></select></label></div></section>;
}






function KnowledgeBaseManagement({ bases, onRefresh }: { bases: KnowledgeBaseRecord[]; onRefresh: () => void }) {
  const [deleteTarget, setDeleteTarget] = useState<{ base: KnowledgeBaseRecord; documentCount: number; versionCount: number; chunkCount: number; assetCount: number; activeTaskCount: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const preview = async (base: KnowledgeBaseRecord) => {
    setError(null);
    try {
      const impact = await desktop.previewDeleteKnowledgeBase(base.id);
      setDeleteTarget({ base, documentCount: impact.document_count, versionCount: impact.version_count, chunkCount: impact.chunk_count, assetCount: impact.asset_count, activeTaskCount: impact.active_task_count });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "读取删除影响失败");
    }
  };
  const remove = async () => {
    if (!deleteTarget) return;
    setBusy(true);
    setError(null);
    try {
      await desktop.deleteKnowledgeBaseConfirmed(deleteTarget.base.id);
      setDeleteTarget(null);
      onRefresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "删除知识库失败");
    } finally {
      setBusy(false);
    }
  };
  return <><section className="suna-knowledge-management"><div className="suna-knowledge-recent-head"><div><h2>知识库管理</h2><span>管理本机知识库和数据生命周期</span></div></div>{error && <p role="alert" className="suna-knowledge-error">{error}</p>}{bases.length ? <div className="suna-knowledge-management-list">{bases.map((base) => <article key={base.id}><div><strong>{base.name}</strong><small>本机个人知识库 · {base.library_type || "research"} · {base.description || "暂无描述"}</small></div><button className="suna-danger-button" onClick={() => void preview(base)} disabled={busy}>删除</button></article>)}</div> : <div className="suna-knowledge-base-empty">暂无知识库</div>}</section>{deleteTarget && <KnowledgeActionModal title="删除知识库" onClose={() => { if (!busy) setDeleteTarget(null); }}><p>确认删除“{deleteTarget.base.name}”？将处理 {deleteTarget.documentCount} 个文档、{deleteTarget.versionCount} 个版本、{deleteTarget.chunkCount} 个片段和 {deleteTarget.assetCount} 个解析资产，历史数据会保留为软删除。</p>{deleteTarget.activeTaskCount > 0 && <p role="alert" className="suna-knowledge-error">当前有 {deleteTarget.activeTaskCount} 个活动任务，需先完成、取消或隔离后才能删除。</p>}{error && <p role="alert" className="suna-knowledge-error">{error}</p>}<footer><button className="suna-ghost-button" onClick={() => setDeleteTarget(null)} disabled={busy}>取消</button><button className="suna-danger-button" onClick={() => void remove()} disabled={busy || deleteTarget.activeTaskCount > 0}>{busy ? "处理中..." : "确认删除"}</button></footer></KnowledgeActionModal>}</>;
}

function KnowledgeSearchPanel({ query, filters, documentTypes, results, favoriteChunkIds, busy, hasKnowledgeBase, embeddingDegradation, rerankDegradation, onQueryChange, onFiltersChange, onSearch, onOpen, onCopy, onAddToChat, onToggleFavorite, onCite }: { query: string; filters: { document_type: string; material: string; process: string; property: string; year: string; tag: string }; documentTypes: string[]; results: PostgresKnowledgeSearchHit[]; favoriteChunkIds: string[]; busy: boolean; hasKnowledgeBase: boolean; embeddingDegradation: string | null; rerankDegradation: string | null; onQueryChange: (value: string) => void; onFiltersChange: (value: { document_type: string; material: string; process: string; property: string; year: string; tag: string }) => void; onSearch: () => void; onOpen: (hit: PostgresKnowledgeSearchHit) => void; onCopy: (hit: PostgresKnowledgeSearchHit) => void; onAddToChat: (hit: PostgresKnowledgeSearchHit) => void; onToggleFavorite: (hit: PostgresKnowledgeSearchHit) => void; onCite: (hit: PostgresKnowledgeSearchHit) => void }) {
  const setFilter = (key: keyof typeof filters, value: string) => onFiltersChange({ ...filters, [key]: value });
  const degradation = embeddingDegradation || rerankDegradation;
  return <section className="suna-knowledge-search-panel"><div className="suna-knowledge-search-row"><label className="suna-knowledge-search"><Search size={18} /><input value={query} onChange={(event) => onQueryChange(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") onSearch(); }} placeholder="搜索材料、工艺、性能和文档内容..." /></label><button className="suna-primary-button" onClick={onSearch} disabled={busy || !hasKnowledgeBase}>{busy ? "检索中..." : "开始检索"}</button></div>{degradation && <div className="suna-knowledge-search-degraded" role="status">当前使用降级检索：{embeddingDegradation ? `Embedding ${degradationLabel(embeddingDegradation)}` : `Reranker ${degradationLabel(rerankDegradation || "")}`}，仍会返回可引用的全文结果。</div>}<div className="suna-knowledge-search-filters"><select aria-label="文档类型筛选" value={filters.document_type} onChange={(event) => setFilter("document_type", event.target.value)}><option value="">全部文档类型</option>{documentTypes.map((type) => <option key={type}>{type}</option>)}</select><input aria-label="材料筛选" value={filters.material} onChange={(event) => setFilter("material", event.target.value)} placeholder="材料" /><input aria-label="工艺筛选" value={filters.process} onChange={(event) => setFilter("process", event.target.value)} placeholder="工艺" /><input aria-label="性能筛选" value={filters.property} onChange={(event) => setFilter("property", event.target.value)} placeholder="性能" /><input aria-label="年份筛选" inputMode="numeric" value={filters.year} onChange={(event) => setFilter("year", event.target.value.replace(/[^0-9]/g, ""))} placeholder="年份" /><input aria-label="标签筛选" value={filters.tag} onChange={(event) => setFilter("tag", event.target.value)} placeholder="标签" /><button className="suna-ghost-button" onClick={() => onFiltersChange({ document_type: "", material: "", process: "", property: "", year: "", tag: "" })}>清除筛选</button></div>{results.length ? <div className="suna-knowledge-search-results">{results.map((hit) => <article key={hit.chunk_id} className="suna-knowledge-result"><div><strong>{hit.document_name}</strong><span>相关度 {hit.rank.toFixed(3)}</span></div><p>{hit.text}</p><small>来源位置：{formatSourceLocation(hit.source_location)}</small><footer><button className="suna-ghost-button" onClick={() => onOpen(hit)}><ExternalLink size={14} />打开原文</button><button className="suna-ghost-button" onClick={() => onCopy(hit)}><Copy size={14} />复制片段</button><button className="suna-ghost-button" onClick={() => onAddToChat(hit)}><MessageSquarePlus size={14} />加入对话</button><button className="suna-ghost-button" aria-pressed={favoriteChunkIds.includes(hit.chunk_id)} onClick={() => onToggleFavorite(hit)}>{favoriteChunkIds.includes(hit.chunk_id) ? "已收藏" : "收藏"}</button><button className="suna-ghost-button" onClick={() => onCite(hit)}>引用</button></footer></article>)}</div> : <div className="suna-knowledge-search-empty"><Search size={28} /><strong>{query ? "未找到匹配的知识库或文档" : "输入问题开始检索"}</strong><span>结果将显示文档名称、片段、来源位置和相关度。</span></div>}</section>;
}
function ChunkPanel({ chunks }: { chunks: PostgresKnowledgeChunk[] }) {
  return <section className="suna-knowledge-chunks"><div className="suna-knowledge-recent-head"><div><h2>知识片段</h2><span>{chunks.length} 个已索引片段</span></div></div>{chunks.length ? <div className="suna-knowledge-chunk-list">{chunks.map((chunk) => <article key={chunk.id}><header><strong>{chunk.document_name}</strong><span>#{chunk.ordinal + 1}</span></header>{chunk.title_path && <small>{chunk.title_path}</small>}<p>{chunk.text}</p><footer>来源位置：{JSON.stringify(chunk.source_location)}</footer></article>)}</div> : <div className="suna-knowledge-document-empty"><Database size={25} /><h3>当前知识库还没有知识片段</h3><p>导入并完成文档处理后，片段会显示在这里。</p></div>}</section>;
}
function KnowledgeActionModal({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) { return <div className="suna-knowledge-modal-overlay" onClick={onClose}><div className="suna-knowledge-modal" onClick={(event) => event.stopPropagation()}><header><strong>{title}</strong><button onClick={onClose} aria-label="关闭"><X size={17} /></button></header>{children}</div></div>; }

function IngestionJobs({ jobs, documents, busy, onRetry, onCancel }: { jobs: PostgresIngestionJob[]; documents: SourceDocumentRecord[]; busy: boolean; onRetry: (job: PostgresIngestionJob) => void; onCancel: (job: PostgresIngestionJob) => void }) {
  const labels: Record<string, string> = { queued: "排队中", pending: "等待处理", uploading: "上传中", parsing: "解析中", chunking: "切分中", embedding: "Embedding 中", indexing: "索引中", running: "处理中", retrying: "重试中", completed: "处理完成", failed: "处理失败", quarantined: "已隔离", cancelled: "已取消" };
  return <section className="suna-knowledge-jobs"><div className="suna-knowledge-recent-head"><h2><Clock3 size={19} />处理任务</h2><span>{jobs.length} 条</span></div>{jobs.length ? <div className="suna-knowledge-jobs-list">{jobs.slice(0, 8).map((job) => { const progress = Math.max(0, Math.min(100, job.progress ?? 0)); return <article key={job.id} className="suna-knowledge-job"><div><strong>{job.source_document_id ? (documents.find((item) => item.id === job.source_document_id)?.display_name || "文档任务") : "文档任务"}</strong><span className={"suna-job-state is-" + job.state}>{labels[job.state] || job.state}</span></div><div className="suna-knowledge-job-progress" aria-label={`${labels[job.state] || job.state} ${progress}%`}><span style={{ width: `${progress}%` }} /></div><small>{progress}% · 尝试 {job.attempts} 次 · {job.updated_at}</small>{job.error_message && <p>{job.error_message}</p>}<div className="suna-knowledge-job-actions">{["failed", "quarantined"].includes(job.state) && <button className="suna-ghost-button" onClick={() => onRetry(job)} disabled={busy}>重试任务</button>}{["pending", "queued", "uploading", "parsing", "chunking", "embedding", "indexing", "running", "retrying"].includes(job.state) && <button className="suna-ghost-button" onClick={() => onCancel(job)} disabled={busy}>取消任务</button>}</div></article>; })}</div> : <div className="suna-knowledge-jobs-empty">暂无导入任务。导入文档后，解析、切块、Embedding 和索引状态会显示在这里。</div>}</section>;
}






