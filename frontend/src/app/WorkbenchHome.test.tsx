import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import WorkbenchHome from "./WorkbenchHome";
import { desktop } from "../bridge/desktop";

vi.mock("../bridge/desktop", () => ({
  desktop: {
    listConversations: vi.fn(),
    listKnowledgeBases: vi.fn(),
    listBackgroundTasks: vi.fn(),
    getKnowledgeHealth: vi.fn(),
  },
}));

describe("WorkbenchHome", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("loads local activity summaries instead of showing static empty values", async () => {
    vi.mocked(desktop.listConversations).mockResolvedValue([
      { id: "conversation-1", title: "Q355B 质量分析", created_at: "2026-08-07T10:00:00Z", updated_at: "2026-08-07T10:10:00Z", pinned: false, archived: false },
      { id: "conversation-2", title: "连铸温度窗口", created_at: "2026-08-07T09:00:00Z", updated_at: "2026-08-07T09:05:00Z", pinned: false, archived: false },
    ]);
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([
      { id: "knowledge-1", name: "钢铁标准", created_at: "2026-08-07T09:00:00Z", updated_at: "2026-08-07T09:00:00Z" },
    ]);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([
      { id: "task-1", kind: "mineru_parse", state: "running", progress: 42, attempt: 1, error_code: null, cancel_requested: false, can_cancel: true, can_retry: false, created_at: "2026-08-07T10:00:00Z", updated_at: "2026-08-07T10:01:00Z" },
    ]);
    vi.mocked(desktop.getKnowledgeHealth).mockResolvedValue({
      knowledge_base_count: 1,
      document_count: 3,
      active_document_count: 2,
      version_count: 3,
      chunk_count: 100,
      indexed_chunk_count: 84,
      active_task_count: 1,
    });

    render(<WorkbenchHome initializationState="ready" onOpenSection={() => undefined} />);

    await waitFor(() => expect(desktop.listConversations).toHaveBeenCalledOnce());
    expect(await screen.findByTestId("workbench-record-count")).toHaveTextContent("2");
    expect(screen.getByTestId("workbench-knowledge-status")).toHaveTextContent("1");
    expect(screen.getByTestId("workbench-task-status")).toHaveTextContent("1");
    expect(screen.getByText("Q355B 质量分析")).toBeInTheDocument();
  });

  it("keeps successful local data visible when one overview source degrades", async () => {
    vi.mocked(desktop.listConversations).mockResolvedValue([
      { id: "conversation-1", title: "Q355B 质量分析", created_at: "2026-08-07T10:00:00Z", updated_at: "2026-08-07T10:10:00Z", pinned: false, archived: false },
    ]);
    vi.mocked(desktop.listKnowledgeBases).mockRejectedValue(new Error("knowledge store unavailable"));
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
    vi.mocked(desktop.getKnowledgeHealth).mockResolvedValue({
      knowledge_base_count: 0,
      document_count: 0,
      active_document_count: 0,
      version_count: 0,
      chunk_count: 0,
      indexed_chunk_count: 0,
      active_task_count: 0,
    });

    render(<WorkbenchHome initializationState="ready" onOpenSection={() => undefined} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("本地工作台数据读取不完整");
    expect(screen.getByText("Q355B 质量分析")).toBeInTheDocument();
    expect(screen.getByTestId("workbench-knowledge-status")).toHaveTextContent("需要检查");
  });
});
