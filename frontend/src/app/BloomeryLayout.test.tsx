import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import BloomeryApp from "./BloomeryApp";
import { desktop } from "../bridge/desktop";
import tokensCss from "../design/tokens.css?raw";
import themeCss from "../design/theme.css?raw";
import polishCss from "../design/polish.css?raw";

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
    listProviderProfiles: vi.fn().mockResolvedValue([]),
  },
}));

const SUPPORTED_SIZES = [
  { width: 1024, height: 720 },
  { width: 1440, height: 900 },
  { width: 1920, height: 1080 },
];

function setWindowSize(width: number, height: number) {
  window.innerWidth = width;
  window.innerHeight = height;
  window.dispatchEvent(new Event("resize"));
}

describe("Bloomery desktop layout", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  for (const size of SUPPORTED_SIZES) {
    it(`renders the full shell with navigation at ${size.width}x${size.height}`, async () => {
      setWindowSize(size.width, size.height);
      render(<BloomeryApp />);

      expect(await screen.findByRole("heading", { name: "工作台" })).toBeInTheDocument();
      expect(screen.getByRole("navigation", { name: "主导航" })).toBeInTheDocument();
      expect(screen.getByRole("main")).toBeInTheDocument();
      expect(screen.getByRole("region", { name: "常用操作" })).toBeInTheDocument();
      for (const label of ["工作台", "对话", "知识库", "数据分析", "扩展", "设置"]) {
        const button = screen.getByRole("button", { name: label });
        expect(button).toBeInTheDocument();
        expect(button.getBoundingClientRect !== undefined).toBe(true);
      }
      expect(screen.queryByRole("button", { name: "诊断" })).not.toBeInTheDocument();
      expect(screen.getByTestId("utility-navigation")).toContainElement(
        screen.getByRole("button", { name: "设置" }),
      );
    });

    it(`keeps collapse behavior accessible at ${size.width}x${size.height}`, async () => {
      setWindowSize(size.width, size.height);
      const { container } = render(<BloomeryApp />);
      await screen.findByRole("heading", { name: "工作台" });

      fireEvent.click(screen.getByRole("button", { name: "折叠侧栏" }));
      expect(container.querySelector(".bloomery-app")).toHaveClass("is-collapsed");
      expect(screen.getByRole("button", { name: "展开侧栏" })).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "展开侧栏" }));
      expect(container.querySelector(".bloomery-app")).not.toHaveClass("is-collapsed");
    });
  }

  it("declares responsive breakpoints for the supported window sizes", () => {
    const theme = themeCss;
    for (const breakpoint of ["1100px", "880px", "760px", "640px"]) {
      expect(theme).toContain(`max-width: ${breakpoint}`);
    }
    expect(theme).toContain("prefers-reduced-motion");
  });

  it("keeps the desktop shell quiet and the chat workbench focused on the conversation", () => {
    expect(polishCss).not.toContain("backdrop-filter: blur(18px)");
    expect(polishCss).not.toContain("background: linear-gradient(120deg, rgba(255, 255, 255, 0.98), rgba(235, 244, 248, 0.92))");
    expect(polishCss).not.toContain("background: linear-gradient(90deg, #e5f0f5, #f6fafc)");
    expect(polishCss).toContain("@media (max-width: 980px)");
    expect(polishCss).toContain("var(--bloomery-chat-session-width)");
    expect(polishCss).toContain("grid-template-columns: var(--bloomery-chat-session-width) minmax(0, 1fr)");
  });

  it("keeps language settings out of the top bar and removes page-level cards", () => {
    expect(polishCss).toContain(".bloomery-page-surface");
    expect(polishCss).toContain("box-shadow: none");
    expect(polishCss).toContain("border-radius: 0");
    expect(polishCss).toContain("background: transparent");
  });

  it("keeps the desktop shell fixed while the main content owns scrolling", () => {
    expect(polishCss).toContain("height: 100dvh;");
    expect(polishCss).toContain("min-height: 0;");
    expect(polishCss).toContain("overflow: hidden;");
    expect(polishCss).toContain(".bloomery-body");
    expect(polishCss).toContain(".bloomery-sidebar");
    expect(polishCss).toContain(".bloomery-main");
    expect(polishCss).toContain(".bloomery-main-inner.is-chat-shell");
    expect(polishCss).toContain(".bloomery-web-chat-embedded");
  });

  it("keeps the sidebar anchored to the body and the utility footer in flow", () => {
    expect(polishCss).toContain(".bloomery-sidebar {\n  height: 100%;");
    expect(polishCss).toContain("  position: relative;\n  top: 0;");
    expect(polishCss).toContain(".bloomery-sidebar-footer {\n  display: block;");
    expect(polishCss).toContain("  flex: 0 0 auto;");
    expect(polishCss).toContain("  visibility: visible;");
  });

  it("uses one content track for non-chat pages", () => {
    expect(polishCss).toContain(".bloomery-main-inner:not(.is-chat-shell)");
    expect(polishCss).toContain("  width: 100%;");
    expect(polishCss).toContain("  max-width: 1440px;");
    expect(polishCss).toContain("  margin: 0 auto;");
  });

  it("keeps the embedded Web mobile overlay from covering desktop navigation", () => {
    expect(polishCss).toContain(".bloomery-web-chat-embedded > .fixed.inset-0.z-40");
    expect(polishCss).toContain("  display: none;");
  });

  it("keeps analysis and knowledge content inside consistent card gutters", () => {
    expect(polishCss).toContain(".bloomery-analysis-tool,\n.bloomery-analysis-result");
    expect(polishCss).toContain("  padding: 24px;");
    expect(polishCss).toContain(".bloomery-knowledge-content-header,\n.bloomery-knowledge-import,\n.bloomery-knowledge-list-section");
    expect(polishCss).toContain("  padding-left: 24px;");
    expect(polishCss).toContain("  padding-right: 24px;");
    expect(polishCss).toContain("  overflow-wrap: anywhere;");
  });

  it("keeps the desktop Web chat focused on conversation controls", () => {
    expect(polishCss).toContain(".bloomery-web-chat-desktop-clean > aside > div > div:first-child > div > button:first-child");
    expect(polishCss).toContain(".bloomery-web-chat-desktop-clean > aside > div > div:nth-child(2) > div > div:last-child");
    expect(polishCss).toContain(".bloomery-web-chat-desktop-clean > main > section:first-child");
    expect(polishCss).toContain(".bloomery-web-chat-desktop-clean h3 + p");
    expect(polishCss).toContain("--agent-turn-gutter: clamp(24px, 4vw, 64px);");
    expect(polishCss).toContain("min-height: 88px !important;");
    expect(polishCss).toContain("max-height: 220px !important;");
  });

  it("uses a complete domain package install grid", () => {
    expect(polishCss).toContain("grid-template-columns: auto minmax(0, 1fr) repeat(3, max-content);");
    expect(polishCss).toContain(".bloomery-domain-install input");
    expect(polishCss).toContain(".bloomery-domain-install .bloomery-secondary-button");
  });

  it("uses the Web palette and exposes a compact workbench header", async () => {
    expect(tokensCss).toContain("--bloomery-bg: #faf9f5");
    expect(tokensCss).toContain("--bloomery-bg-raised: #fffdf9");
    expect(tokensCss).toContain("--bloomery-amber: #cc785c");
    expect(tokensCss).toContain("--bloomery-text: #141413");
    expect(tokensCss).toContain("--bloomery-line: #e6dfd8");
    expect(`${themeCss}\n${polishCss}`).not.toContain("#557684");
    expect(`${themeCss}\n${polishCss}`).toContain(".bloomery-workbench-header");

    render(<BloomeryApp />);
    const header = await screen.findByTestId("workbench-header");
    expect(header.querySelector(".bloomery-action-strip")).not.toBeNull();
  });

  it("defines the dark theme palette for the complete desktop shell", () => {
    expect(tokensCss).toContain('[data-theme="dark"]');
    expect(tokensCss).toContain("--bloomery-bg: #171614");
    expect(tokensCss).toContain("--bloomery-text: #f5f1ea");
    expect(tokensCss).toContain("--bloomery-line: #39342f");
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-topbar');
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-settings-card');
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-app input');
    expect(polishCss).toContain("background-color: var(--bloomery-bg-raised) !important;");
  });

  it("keeps the copied Web chat surfaces dark in dark mode", () => {
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-web-chat-embedded,');
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-web-chat-desktop-clean .web-agent-chat-panel,');
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-web-chat-desktop-clean .web-agent-chat-panel > div,');
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-web-chat-desktop-clean .web-agent-chat-panel header,');
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-web-chat-desktop-clean .web-agent-chat-panel form');
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-web-chat-desktop-clean [class*="bg-[#fbf7ef]"]');
    expect(polishCss).toContain('[data-theme="dark"] .bloomery-web-chat-desktop-clean .web-agent-chat-panel textarea');
  });

  it("keeps copied Web conversation menus readable in dark mode", () => {
    expect(polishCss).toContain(
      '[data-theme="dark"] .bloomery-web-chat-desktop-clean .bloomery-web-chat-session-menu',
    );
    expect(polishCss).toContain(".bloomery-web-chat-session-menu-divider");
    expect(polishCss).toContain(".bloomery-web-chat-session-menu-danger:hover");
  });

  it("shows the degraded provider state when no chat provider is configured", async () => {
    setWindowSize(1440, 900);
    render(<BloomeryApp />);
    await screen.findByRole("heading", { name: "工作台" });

    const providerRow = await screen.findByTestId("workbench-provider-status");
    expect(providerRow).toHaveTextContent("未配置（对话降级）");
  });

  it("shows the ready provider state with the configured display name", async () => {
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([
      {
        id: "chat-1",
        kind: "open_ai_compatible",
        display_name: "本地网关",
        base_url: "http://127.0.0.1:11434/v1",
        model_id: "qwen",
        enabled: true,
        revision: 1,
        secret_generation: 1,
        secret_configured: true,
      },
    ]);
    setWindowSize(1440, 900);
    render(<BloomeryApp />);
    await screen.findByRole("heading", { name: "工作台" });

    const providerRow = await screen.findByTestId("workbench-provider-status");
    expect(providerRow).toHaveTextContent("本地网关");
  });
});
