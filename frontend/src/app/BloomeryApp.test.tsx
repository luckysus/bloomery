import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import BloomeryApp from "./BloomeryApp";
import { desktop } from "../bridge/desktop";

vi.mock("../bridge/desktop", () => ({
  isDesktopRuntime: vi.fn().mockReturnValue(true),
  desktop: {
    initialize: vi.fn().mockResolvedValue(undefined),
    getSetting: vi.fn((key: string) => Promise.resolve(
      key === "ui.locale"
        ? JSON.stringify({ preference: "zh-CN" })
        : JSON.stringify({ completed: true }),
    )),
    setSetting: vi.fn().mockResolvedValue(undefined),
    listKnowledgeBases: vi.fn().mockResolvedValue([]),
    listKnowledgeDocuments: vi.fn().mockResolvedValue([]),
    listBackgroundTasks: vi.fn().mockResolvedValue([]),
    getKnowledgeHealth: vi.fn().mockResolvedValue({
      knowledge_base_count: 0,
      document_count: 0,
      active_document_count: 0,
      version_count: 0,
      chunk_count: 0,
      indexed_chunk_count: 0,
      active_task_count: 0,
    }),
    listConversations: vi.fn().mockResolvedValue([]),
    createConversation: vi.fn(),
    listMessages: vi.fn().mockResolvedValue([]),
    getConversationDraft: vi.fn().mockResolvedValue(""),
    saveConversationDraft: vi.fn().mockResolvedValue(undefined),
    listenDesktopAgentDeltas: vi.fn().mockResolvedValue(() => undefined),
    desktopAgentChat: vi.fn(),
    cancelDesktopRun: vi.fn().mockResolvedValue(undefined),
    listProviderProfiles: vi.fn().mockResolvedValue([]),
  },
}));

describe("BloomeryApp", () => {
  it("shows the workbench landmark after local initialization", async () => {
    render(<BloomeryApp />);

    await waitFor(() => expect(desktop.initialize).toHaveBeenCalledOnce());
    expect(await screen.findByRole("main", { name: "工作台" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "工作台" })).toBeInTheDocument();
  });

  it("exposes the complete desktop navigation with a single active section", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });

    expect(screen.getByRole("navigation", { name: "主导航" })).toBeInTheDocument();
    for (const label of ["工作台", "对话", "知识库", "数据分析", "扩展", "设置", "诊断"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(screen.getByRole("button", { name: "工作台" })).toHaveAttribute("aria-current", "page");
  });

  it("changes the active section without losing the desktop shell", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });
    fireEvent.click(screen.getByRole("button", { name: "知识库" }));

    expect(screen.getByRole("main", { name: "知识库" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "知识库" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "知识库" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "工作台" })).not.toHaveAttribute("aria-current", "page");
  });

  it("collapses navigation labels while preserving accessible button names", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });
    const toggle = screen.getByRole("button", { name: "折叠侧栏" });
    toggle.focus();
    expect(toggle).toHaveFocus();
    fireEvent.click(toggle);

    expect(screen.queryByTestId("nav-label-chat")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "对话" })).toHaveAttribute("title", "对话");
    expect(screen.getByRole("button", { name: "展开侧栏" })).toBeInTheDocument();
  });

  it("routes the primary workbench action to a useful section", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });
    fireEvent.click(screen.getByRole("button", { name: "导入文档" }));

    expect(screen.getByRole("heading", { name: "知识库" })).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "知识库" })).toBeInTheDocument();
  });

  it("switches the shell language and persists the preference", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });
    const language = screen.getByRole("combobox", { name: "界面语言" });
    fireEvent.change(language, { target: { value: "en-US" } });

    expect(await screen.findByRole("heading", { name: "Workbench" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Chat" })).toBeInTheDocument();
    expect(desktop.setSetting).toHaveBeenCalledWith(
      "ui.locale",
      JSON.stringify({ version: 1, preference: "en-US" }),
    );
  });
});
