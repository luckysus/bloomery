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
    getIndexHealth: vi.fn(),
    createKnowledgeBase: vi.fn(),
    renameKnowledgeBase: vi.fn(),
    previewDeleteKnowledgeBase: vi.fn(),
    deleteKnowledgeBaseConfirmed: vi.fn(),
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

  it("creates a knowledge base from the empty state", async () => {
    render(<KnowledgePage />);

    expect(await screen.findByRole("heading", { name: "知识库" })).toBeInTheDocument();
    expect(screen.getByText("还没有知识库")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("知识库名称"), { target: { value: "钢铁标准" } });
    fireEvent.click(screen.getByRole("button", { name: "创建知识库" }));

    await waitFor(() => expect(desktop.createKnowledgeBase).toHaveBeenCalledWith("钢铁标准"));
    expect(await screen.findByRole("button", { name: "钢铁标准" })).toBeInTheDocument();
  });

  it("shows selected documents and active task state", async () => {
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

    expect(await screen.findByText("GB 50632.pdf")).toBeInTheDocument();
    expect(screen.getByText(/处理中/)).toBeInTheDocument();
    expect(screen.getByText("8 / 12")).toBeInTheDocument();
  });

  it("imports a local document with the configured retrieval profiles", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    render(<KnowledgePage />);

    await screen.findByRole("button", { name: "钢铁标准" });
    fireEvent.change(screen.getByLabelText("文件路径"), { target: { value: "F:\\docs\\GB 50632.pdf" } });
    fireEvent.click(screen.getByRole("button", { name: "导入文档" }));

    await waitFor(() => expect(desktop.importLocalDocument).toHaveBeenCalledWith({
      source_path: "F:\\docs\\GB 50632.pdf",
      knowledge_base: { mode: "existing", id: base.id },
      mineru_profile_id: "mineru-1",
      embedding_profile_id: "embedding-1",
      embedding_dimension: 1024,
    }));
    expect(await screen.findByText("导入任务已创建")).toBeInTheDocument();
  });

  it("uses the native file picker to fill a document path", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\docs\\GB 50632.pdf");
    render(<KnowledgePage />);

    await screen.findByRole("button", { name: "钢铁标准" });
    fireEvent.click(screen.getByRole("button", { name: "选择文件" }));

    await waitFor(() => expect(desktop.openFileDialog).toHaveBeenCalled());
    expect(screen.getByLabelText("文件路径")).toHaveValue("F:\\docs\\GB 50632.pdf");
  });
  it("offers cancellation and retry for recoverable background tasks", async () => {
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([base]);
    const runningTask = {
      id: "task-running",
      kind: "mineru_parse",
      state: "running" as const,
      progress: 42,
      attempt: 1,
      error_code: null,
      cancel_requested: false,
      can_cancel: true,
      can_retry: false,
      created_at: base.created_at,
      updated_at: base.updated_at,
    };
    const failedTask = {
      ...runningTask,
      id: "task-failed",
      state: "failed" as const,
      progress: 18,
      error_code: "provider_timeout",
      can_cancel: false,
      can_retry: true,
    };
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([runningTask, failedTask]);
    vi.mocked(desktop.cancelBackgroundTask).mockResolvedValue({
      ...runningTask,
      state: "cancelled",
      can_cancel: false,
      can_retry: true,
    });
    vi.mocked(desktop.retryBackgroundTask).mockResolvedValue({
      ...failedTask,
      state: "queued",
      can_cancel: true,
      can_retry: false,
      attempt: 2,
    });

    render(<KnowledgePage />);

    expect(await screen.findByRole("button", { name: "取消任务" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "取消任务" }));
    await waitFor(() => expect(desktop.cancelBackgroundTask).toHaveBeenCalledWith("task-running"));

    const retryButtons = screen.getAllByRole("button", { name: "重试任务" });
    fireEvent.click(retryButtons[retryButtons.length - 1]);
    await waitFor(() => expect(desktop.retryBackgroundTask).toHaveBeenCalledWith("task-failed"));
  });

  it("shows document versions and queues a rebuild for an unhealthy index", async () => {
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
    vi.mocked(desktop.listDocumentVersions).mockResolvedValue([{
      id: "version-1",
      document_id: "document-1",
      content_sha256: "a".repeat(64),
      mime_type: "application/pdf",
      parser: "mineru",
      parser_version: "2",
      chunk_policy_version: "steel-v1",
      embedding_profile_id: "embedding-1",
      embedding_model_id: "BAAI/bge-m3",
      embedding_dimension: 1024,
      expected_asset_count: 0,
      expected_chunk_count: 12,
      manifest_sealed: true,
      created_at: base.created_at,
      activated_at: base.updated_at,
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

    expect(await screen.findByText("BAAI/bge-m3")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "索引健康" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重建索引" }));

    await waitFor(() => expect(desktop.rebuildKnowledgeIndex).toHaveBeenCalledWith({
      provider_profile_id: "embedding-1",
      model_id: "BAAI/bge-m3",
      dimension: 1024,
    }));
  });
});
