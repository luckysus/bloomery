import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ChatPage from "./ChatPage";
import { desktop } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    listConversations: vi.fn(),
    searchHistory: vi.fn(),
    createConversation: vi.fn(),
    updateConversationTitle: vi.fn(),
    updateConversationPinned: vi.fn(),
    archiveConversation: vi.fn(),
    deleteConversationLocal: vi.fn(),
    listMessages: vi.fn(),
    getConversationDraft: vi.fn(),
    saveConversationDraft: vi.fn(),
    listenAgentEvents: vi.fn(),
    replayAgentRun: vi.fn(),
    desktopAgentChat: vi.fn(),
    resolveAgentPermission: vi.fn(),
    cancelDesktopRun: vi.fn(),
    listKnowledgeBases: vi.fn(),
    queryLocalKnowledge: vi.fn(),
    listProviderProfiles: vi.fn(),
    setDefaultProvider: vi.fn(),
    resolveKnowledgeCitation: vi.fn(),
    saveFileDialog: vi.fn(),
    exportConversation: vi.fn(),
  },
}));

describe("WebChatWorkspace", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listConversations).mockResolvedValue([]);
    vi.mocked(desktop.searchHistory).mockResolvedValue([]);
    vi.mocked(desktop.listMessages).mockResolvedValue([]);
    vi.mocked(desktop.getConversationDraft).mockResolvedValue("");
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([]);
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([]);
    vi.mocked(desktop.listenAgentEvents).mockResolvedValue(() => undefined);
    vi.mocked(desktop.replayAgentRun).mockResolvedValue([]);
  });

  it("renders the copied Web conversation page", async () => {
    const onOpenSection = vi.fn();
    render(<ChatPage onOpenSection={onOpenSection} />);

    expect(await screen.findByTitle("钢铁智能体")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Web 风格对话面板" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "知识库" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "模型训练" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "工艺优化" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /账户与设置/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "新聊天" })).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toBeInTheDocument();

    expect(onOpenSection).not.toHaveBeenCalled();
  });

  it("supports the Web sidebar collapse affordance", async () => {
    render(<ChatPage />);

    const workspace = await screen.findByRole("region", { name: "钢铁智能体" });
    expect(screen.getByRole("button", { name: "关闭侧栏" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "关闭侧栏" }));
    expect(workspace).toHaveClass("is-sidebar-collapsed");
    expect(screen.getByTitle("打开侧栏")).toBeInTheDocument();

    fireEvent.click(screen.getByTitle("打开侧栏"));
    expect(workspace).not.toHaveClass("is-sidebar-collapsed");
    expect(screen.getByRole("button", { name: "关闭侧栏" })).toBeInTheDocument();
  });

  it("keeps the Web recent-chat affordance available while collapsed", async () => {
    render(<ChatPage />);

    await screen.findByTitle("钢铁智能体");
    fireEvent.click(screen.getByRole("button", { name: "关闭侧栏" }));
    expect(screen.getByRole("button", { name: "最近聊天" })).toBeInTheDocument();
  });

  it("keeps Web conversation actions connected to the local controller", async () => {
    vi.mocked(desktop.listConversations).mockResolvedValue([{
      id: "conversation-1",
      title: "高炉温度分析",
      created_at: "2026-08-16T00:00:00Z",
      updated_at: "2026-08-16T00:00:00Z",
      pinned: false,
      archived: false,
    }]);

    render(<ChatPage />);

    await screen.findByRole("button", { name: "高炉温度分析" });
    fireEvent.click(screen.getByRole("button", { name: "更多操作" }));
    const conversationMenu = document.querySelector(".bloomery-web-chat-session-menu");
    expect(conversationMenu).toBeInTheDocument();
    expect(conversationMenu?.querySelector(".bloomery-web-chat-session-menu-divider")).toBeInTheDocument();
    expect(conversationMenu?.querySelector(".bloomery-web-chat-session-menu-danger")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重命名" }));

    const renameInput = screen.getByDisplayValue("高炉温度分析");
    fireEvent.change(renameInput, { target: { value: "炼钢温度分析" } });
    fireEvent.keyDown(renameInput, { key: "Enter" });

    await waitFor(() => {
      expect(desktop.updateConversationTitle).toHaveBeenCalledWith("conversation-1", "炼钢温度分析");
    });
  });

  it("opens local history search and selects a matching conversation", async () => {
    vi.mocked(desktop.searchHistory).mockResolvedValue([{
      conversation_id: "conversation-2",
      conversation_title: "高炉温度分析",
      message_id: "message-2",
      role: "user",
      content: "分析高炉温度",
      created_at: "2026-08-16T01:00:00Z",
      score: 1,
      snippet: "分析高炉温度",
    }]);
    vi.mocked(desktop.listMessages).mockResolvedValue([]);

    render(<ChatPage />);

    fireEvent.click(await screen.findByRole("button", { name: "搜索聊天" }));
    const searchInput = screen.getByPlaceholderText("搜索聊天...");
    fireEvent.change(searchInput, { target: { value: "高炉温度" } });

    expect(await screen.findByRole("button", { name: "高炉温度分析" })).toBeInTheDocument();
    expect(desktop.searchHistory).toHaveBeenCalledWith({ query: "高炉温度", limit: 12 });

    fireEvent.click(screen.getByRole("button", { name: "高炉温度分析" }));
    await waitFor(() => expect(desktop.listMessages).toHaveBeenCalledWith("conversation-2"));
  });
});
