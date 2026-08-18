import { StrictMode } from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
    installBundledSteelPackage: vi.fn().mockResolvedValue({}),
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
    listenAgentEvents: vi.fn().mockResolvedValue(() => undefined),
    desktopAgentChat: vi.fn(),
    cancelDesktopRun: vi.fn().mockResolvedValue(undefined),
    listProviderProfiles: vi.fn().mockResolvedValue([]),
    listDatabaseConnections: vi.fn().mockResolvedValue([]),
    listDatabases: vi.fn().mockResolvedValue([]),
    listDatabaseTables: vi.fn().mockResolvedValue([]),
    listDatabaseQueryResults: vi.fn().mockResolvedValue([]),
    submitDatabaseQuery: vi.fn(),
    getDatabaseQueryResult: vi.fn().mockResolvedValue(null),
    cancelBackgroundTask: vi.fn(),
    saveSteelDataset: vi.fn(),
    activateSteelDataset: vi.fn(),
  },
}));

describe("BloomeryApp", () => {
  it("shows the workbench landmark after local initialization", async () => {
    render(<BloomeryApp />);

    await waitFor(() => expect(desktop.initialize).toHaveBeenCalledOnce());
    expect(await screen.findByRole("main", { name: "工作台" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "工作台" })).toBeInTheDocument();
    expect(screen.queryByText("STEEL AGENT WORKBENCH")).not.toBeInTheDocument();
    expect(screen.queryByText("本地工作区")).not.toBeInTheDocument();
    expect(screen.queryByText("LOCAL / 1.0.0")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "界面语言" })).not.toBeInTheDocument();
    expect(screen.queryByText("离线优先 · 数据归本地")).not.toBeInTheDocument();
    expect(screen.queryByText("从本地知识、对话和生产数据开始一次可追溯的工作。")).not.toBeInTheDocument();
  });

  it("initializes the desktop bridge once under React StrictMode", async () => {
    vi.mocked(desktop.initialize).mockClear();

    render(
      <StrictMode>
        <BloomeryApp />
      </StrictMode>,
    );

    await waitFor(() => expect(desktop.initialize).toHaveBeenCalledOnce());
  });

  it("enters the workbench when no first-run configuration exists", async () => {
    vi.mocked(desktop.getSetting).mockImplementation((key) => Promise.resolve(
      key === "ui.locale" ? JSON.stringify({ preference: "zh-CN" }) : null,
    ));

    render(<BloomeryApp />);

    expect(await screen.findByRole("main", { name: "工作台" })).toBeInTheDocument();
    expect(screen.queryByRole("main", { name: "首次启动配置" })).not.toBeInTheDocument();
  });

  it("opens the databases section from navigation", async () => {
    render(<BloomeryApp />);

    fireEvent.click(await screen.findByRole("button", { name: "数据库" }));

    expect(await screen.findByRole("main", { name: "数据库" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "数据库工作台" })).toBeInTheDocument();
  });

  it("exposes the complete desktop navigation with a single active section", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });

    expect(screen.getByRole("navigation", { name: "主导航" })).toBeInTheDocument();
    for (const label of ["工作台", "对话", "知识库", "数据库", "数据分析", "扩展", "设置"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(screen.queryByRole("button", { name: "诊断" })).not.toBeInTheDocument();
    expect(screen.getByTestId("utility-navigation")).toContainElement(
      screen.getByRole("button", { name: "设置" }),
    );
    expect(screen.getByRole("button", { name: "工作台" })).toHaveAttribute("aria-current", "page");
  });

  it("opens diagnostics from settings instead of exposing it as a primary module", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });
    fireEvent.click(screen.getByRole("button", { name: "设置" }));

    const diagnostics = await screen.findByRole("button", { name: "诊断与日志" });
    fireEvent.click(diagnostics);

    expect(await screen.findByRole("heading", { name: "诊断中心" })).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "诊断" })).toBeInTheDocument();
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

  it("switches the chat route to the complete Web conversation shell", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });
    fireEvent.click(screen.getByRole("button", { name: "对话" }));

    expect((await screen.findAllByRole("button", { name: "钢铁智能体" })).length).toBeGreaterThanOrEqual(1);
    const chatPanel = screen.getByRole("region", { name: "Web 风格对话面板" });
    expect(chatPanel).toBeInTheDocument();
    expect(within(chatPanel).queryByRole("button", { name: "知识库" })).not.toBeInTheDocument();
    expect(within(chatPanel).queryByRole("button", { name: "模型训练" })).not.toBeInTheDocument();
    expect(within(chatPanel).queryByRole("button", { name: "工艺优化" })).not.toBeInTheDocument();
    expect(within(chatPanel).queryByRole("button", { name: /账户与设置/ })).not.toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "主导航" })).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "对话" })).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "对话" }).querySelector(".bloomery-main-inner.is-chat-shell")).not.toBeNull();
    expect(screen.getByTestId("utility-navigation")).toContainElement(
      screen.getByRole("button", { name: "设置" }),
    );
  });

  it("keeps outer navigation usable after entering and leaving chat", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });
    const mainNavigation = screen.getByRole("navigation", { name: "主导航" });
    const chatButton = mainNavigation.querySelector('button[aria-label="对话"]') as HTMLButtonElement;
    const knowledgeButton = mainNavigation.querySelector('button[aria-label="知识库"]') as HTMLButtonElement;

    fireEvent.click(chatButton);
    expect(await screen.findByRole("main", { name: "对话" })).toBeInTheDocument();

    fireEvent.click(knowledgeButton);
    expect(await screen.findByRole("heading", { name: "知识库" })).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "知识库" })).toBeInTheDocument();

    fireEvent.click(chatButton);
    expect(await screen.findByRole("main", { name: "对话" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "主导航" })).toBeInTheDocument();
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
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const language = screen.getByRole("combobox", { name: "界面语言" });
    fireEvent.change(language, { target: { value: "en-US" } });

    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Chat" })).toBeInTheDocument();
    expect(desktop.setSetting).toHaveBeenCalledWith(
      "ui.locale",
      JSON.stringify({ version: 1, preference: "en-US" }),
    );
  });

  it("switches the application theme and persists the preference", async () => {
    render(<BloomeryApp />);

    await screen.findByRole("heading", { name: "工作台" });
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(await screen.findByRole("tab", { name: "通用" }));
    fireEvent.click(await screen.findByRole("button", { name: "深色" }));

    await waitFor(() =>
      expect(desktop.setSetting).toHaveBeenCalledWith(
        "ui.theme",
        JSON.stringify({ version: 1, preference: "dark" }),
      ),
    );
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
  });
});
