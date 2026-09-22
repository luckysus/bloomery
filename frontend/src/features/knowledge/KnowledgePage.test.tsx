import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import KnowledgePage from "./KnowledgePage";
import { desktop, type KnowledgeBaseRecord } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    getSetting: vi.fn(),
    openFileDialog: vi.fn(),
    listKnowledgeBases: vi.fn(),
    listKnowledgeDocuments: vi.fn(),
    listDocumentVersions: vi.fn(),
    listBackgroundTasks: vi.fn(),
    listProviderProfiles: vi.fn(),
    getKnowledgeHealth: vi.fn(),
    getKnowledgeDatabaseHealth: vi.fn(),
    getIndexHealth: vi.fn(),
    createKnowledgeBase: vi.fn(),
    listPostgresKnowledgeBases: vi.fn(),
    createPostgresKnowledgeBase: vi.fn(),
    importPostgresDocument: vi.fn(),
    listPostgresIngestionJobs: vi.fn(),
    renameKnowledgeBase: vi.fn(),
    previewDeleteKnowledgeBase: vi.fn(),
    deleteKnowledgeBaseConfirmed: vi.fn(),
    mergeKnowledgeBases: vi.fn(),
    renameKnowledgeDocument: vi.fn(),
    deleteKnowledgeDocument: vi.fn(),
    getKnowledgeDocumentPreview: vi.fn(),
    importLocalDocument: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    retryBackgroundTask: vi.fn(),
    rebuildKnowledgeIndex: vi.fn(),
  },
}));

const base: KnowledgeBaseRecord = {
  id: "kb-steel",
  name: "钢铁标准",
  created_at: "2026-08-05T10:00:00Z",
  updated_at: "2026-08-05T10:00:00Z",
};

describe("KnowledgePage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.openFileDialog).mockResolvedValue(null);
    vi.mocked(desktop.getSetting).mockResolvedValue(JSON.stringify({
      state: "configured",
      embedding_profile_id: "embedding-1",
      mineru_profile_id: "mineru-1",
    }));
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([]);
    vi.mocked(desktop.getKnowledgeDatabaseHealth).mockResolvedValue({
      configured: true,
      connected: true,
      vector_extension: true,
      migration_version: 2,
      message: "ok",
    });
    vi.mocked(desktop.listPostgresKnowledgeBases).mockResolvedValue([]);
    vi.mocked(desktop.createPostgresKnowledgeBase).mockImplementation(async (name) => ({
      ...base,
      id: `kb-${name}`,
      name,
    }));
    vi.mocked(desktop.importPostgresDocument).mockResolvedValue({
      knowledge_base_id: base.id,
      document_id: "document-1",
      version_id: "version-1",
      chunk_count: 1,
      asset_count: 0,
      duplicate_content: false,
    });
    vi.mocked(desktop.listPostgresIngestionJobs).mockResolvedValue([]);
    vi.mocked(desktop.listKnowledgeDocuments).mockResolvedValue([]);
    vi.mocked(desktop.listDocumentVersions).mockResolvedValue([]);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([{
      id: "embedding-1",
      kind: "siliconflow",
      display_name: "SiliconFlow Embedding",
      base_url: "https://api.siliconflow.cn/v1",
      model_id: "BAAI/bge-m3",
      enabled: true,
      revision: 1,
      secret_generation: 1,
      secret_configured: true,
    }]);
    vi.mocked(desktop.getIndexHealth).mockResolvedValue({
      state: "healthy",
      reason: null,
      serving_mode: "hnsw",
      chunk_count: 0,
      required_rebuild_bytes: 0,
      available_disk_bytes: null,
      stale_temporary_count: 0,
      rebuild_task_id: null,
    });
    vi.mocked(desktop.rebuildKnowledgeIndex).mockResolvedValue("task-rebuild");
    vi.mocked(desktop.mergeKnowledgeBases).mockImplementation(async (request) => ({
      ...base,
      id: request.mode === "new" ? "kb-merged" : request.target_id,
      name: request.mode === "new" ? request.destination_name || "合并知识库" : "目标知识库",
    }));
    vi.mocked(desktop.getKnowledgeDocumentPreview).mockResolvedValue({
      processed: true,
      content: "本地解析内容",
      blocks: [],
    });
    vi.mocked(desktop.getKnowledgeHealth).mockResolvedValue({
      knowledge_base_count: 0,
      document_count: 0,
      active_document_count: 0,
      version_count: 0,
      chunk_count: 0,
      indexed_chunk_count: 0,
      active_task_count: 0,
    });
    vi.mocked(desktop.createKnowledgeBase).mockImplementation(async (name) => ({
      ...base,
      id: `kb-${name}`,
      name,
    }));
    vi.mocked(desktop.importLocalDocument).mockResolvedValue({
      knowledge_base_id: base.id,
      document_id: "document-1",
      version_id: "version-1",
      ingest_attempt_id: "attempt-1",
      task_id: "task-1",
      duplicate_content: false,
    });
  });

  it("uses the copied Web upload flow to create and import local files", async () => {
    vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\docs\\GB 50632.pdf");
    render(<KnowledgePage />);

    expect(await screen.findByRole("heading", { name: "知识库" })).toBeInTheDocument();
    expect(screen.getByText("暂无知识库")).toBeInTheDocument();
    expect(screen.queryByText("LOCAL KNOWLEDGE / STEEL DOMAIN")).not.toBeInTheDocument();
    expect(screen.queryByText("管理标准、论文和工艺资料，让每一次检索都能回到原始文档。")).not.toBeInTheDocument();
    expect(screen.queryByText("知识库会保存文档、版本和可追溯的检索证据。")).not.toBeInTheDocument();
    expect(screen.getByText("点击上传或拖拽文档到这里")).toBeInTheDocument();
    expect(screen.queryByText("上传本地文档")).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("知识库名称"), { target: { value: "钢铁标准" } });
    fireEvent.click(screen.getByRole("button", { name: "点击上传或拖拽文档到这里" }));

    expect(await screen.findByText("GB 50632.pdf")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "下一步" }));
    expect(await screen.findByText("解析内容")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认" }));

    await waitFor(() => expect(desktop.importPostgresDocument).toHaveBeenCalledWith({
      knowledge_base_id: "kb-钢铁标准",
      source_path: "F:\\docs\\GB 50632.pdf",
    }));
    expect(await screen.findByText(/处理中/)).toBeInTheDocument();
  });

  it("uses the migrated Web knowledge workspace shell", async () => {
    render(<KnowledgePage />);

    const workspace = await screen.findByTestId("knowledge-web-workspace");
    expect(workspace).toHaveClass("fixed", "inset-0", "z-50");
    expect(screen.getByRole("button", { name: "新建知识库" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "合并知识库" })).toBeInTheDocument();
    expect(screen.queryByLabelText("知识库统计")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "关闭侧栏" }));
    expect(screen.getByRole("button", { name: "打开边栏" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "打开边栏" }));
    expect(screen.getByRole("button", { name: "关闭侧栏" })).toBeInTheDocument();
  });

  it("shows selected documents without adding local status panels", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    vi.mocked(desktop.listKnowledgeDocuments).mockResolvedValue([{
      id: "document-1",
      knowledge_base_id: base.id,
      display_name: "GB 50632.pdf",
      source_kind: "pdf",
      active_version_id: "version-1",
      created_at: base.created_at,
      updated_at: base.updated_at,
    }]);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([{
      id: "task-1",
      kind: "mineru_parse",
      state: "running",
      progress: 42,
      attempt: 1,
      error_code: null,
      cancel_requested: false,
      can_cancel: true,
      can_retry: false,
      created_at: base.created_at,
      updated_at: base.updated_at,
      started_at: null,
      finished_at: null,
      file_name: null,
    }]);
    vi.mocked(desktop.getKnowledgeHealth).mockResolvedValue({
      knowledge_base_count: 1,
      document_count: 1,
      active_document_count: 1,
      version_count: 1,
      chunk_count: 12,
      indexed_chunk_count: 8,
      active_task_count: 1,
    });

    render(<KnowledgePage />);

    expect((await screen.findAllByText("GB 50632.pdf")).length).toBeGreaterThan(0);
    expect(screen.queryByText(/处理中/)).not.toBeInTheDocument();
    expect(screen.queryByText("8 / 12")).not.toBeInTheDocument();
  });

  it("keeps the migrated Web detail view free of client-only panels", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    vi.mocked(desktop.listKnowledgeDocuments).mockResolvedValue([{
      id: "document-1",
      knowledge_base_id: base.id,
      display_name: "GB 50632.pdf",
      source_kind: "pdf",
      active_version_id: "version-1",
      created_at: base.created_at,
      updated_at: base.updated_at,
    }]);

    render(<KnowledgePage />);

    expect(await screen.findByRole("heading", { name: "GB 50632.pdf" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "索引健康" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "后台任务" })).not.toBeInTheDocument();
  });

  it("adds a local document to an existing base through the Web add flow", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\docs\\GB 50632.pdf");
    render(<KnowledgePage />);

    await screen.findByRole("button", { name: "钢铁标准" });
    fireEvent.click(screen.getByRole("button", { name: "钢铁标准" }));
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    fireEvent.click(screen.getByRole("button", { name: "点击上传或拖拽文档到这里" }));
    expect(await screen.findByText("GB 50632.pdf")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "下一步" }));
    fireEvent.click(await screen.findByRole("button", { name: "确认" }));

    await waitFor(() => expect(desktop.importPostgresDocument).toHaveBeenCalledWith({
      knowledge_base_id: base.id,
      source_path: "F:\\docs\\GB 50632.pdf",
    }));
  });

  it("imports through PostgreSQL when MinerU is not configured", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\docs\\GB 50632.pdf");
    vi.mocked(desktop.getSetting).mockResolvedValue(JSON.stringify({
      state: "partial",
      embedding_profile_id: "embedding-1",
      mineru_profile_id: null,
    }));
    render(<KnowledgePage />);

    await screen.findByRole("button", { name: "钢铁标准" });
    fireEvent.click(screen.getByRole("button", { name: "钢铁标准" }));
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    fireEvent.click(screen.getByRole("button", { name: "点击上传或拖拽文档到这里" }));
    await screen.findByText("GB 50632.pdf");
    fireEvent.click(screen.getByRole("button", { name: "下一步" }));
    fireEvent.click(await screen.findByRole("button", { name: "确认" }));

    await waitFor(() => expect(desktop.importPostgresDocument).toHaveBeenCalledWith({
      knowledge_base_id: base.id,
      source_path: "F:\\docs\\GB 50632.pdf",
    }));
  });

  it("uses the native file picker to fill a document path", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\docs\\GB 50632.pdf");
    render(<KnowledgePage />);

    await screen.findByRole("button", { name: "钢铁标准" });
    fireEvent.click(screen.getByRole("button", { name: "点击上传或拖拽文档到这里" }));

    await waitFor(() => expect(desktop.openFileDialog).toHaveBeenCalled());
    expect(await screen.findByText("GB 50632.pdf")).toBeInTheDocument();
  });

  it("merges knowledge bases through the copied Web dialog using Rust", async () => {
    const target: KnowledgeBaseRecord = { ...base, id: "kb-target", name: "耐磨钢" };
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base, target]);
    render(<KnowledgePage />);

    expect(await screen.findByRole("button", { name: "合并知识库" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "合并知识库" }));
    expect(await screen.findByRole("heading", { name: "合并知识库" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("新知识库名称"), { target: { value: "钢铁合并库" } });
    fireEvent.click(screen.getByRole("button", { name: "合并" }));

    await waitFor(() => expect(desktop.mergeKnowledgeBases).toHaveBeenCalledWith({
      source_id: base.id,
      target_id: target.id,
      mode: "new",
      destination_name: "钢铁合并库",
    }));
  });
  it("keeps the migrated Web detail view free of local index settings", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    vi.mocked(desktop.listKnowledgeDocuments).mockResolvedValue([{
      id: "document-1",
      knowledge_base_id: base.id,
      display_name: "GB 50632.pdf",
      source_kind: "pdf",
      active_version_id: "version-1",
      created_at: base.created_at,
      updated_at: base.updated_at,
    }]);
    vi.mocked(desktop.getIndexHealth).mockResolvedValue({
      state: "rebuild_required",
      reason: "model_changed",
      serving_mode: "flat",
      chunk_count: 12,
      required_rebuild_bytes: 1024,
      available_disk_bytes: 1024 * 1024,
      stale_temporary_count: 0,
      rebuild_task_id: null,
    });

    render(<KnowledgePage />);

    expect(await screen.findByRole("heading", { name: "GB 50632.pdf" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "索引健康" })).not.toBeInTheDocument();
  });
});
