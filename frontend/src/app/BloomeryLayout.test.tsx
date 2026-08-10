import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import BloomeryApp from "./BloomeryApp";
import { desktop } from "../bridge/desktop";
import themeCss from "../design/theme.css?raw";

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
      for (const label of ["工作台", "对话", "知识库", "数据分析", "扩展", "设置", "诊断"]) {
        const button = screen.getByRole("button", { name: label });
        expect(button).toBeInTheDocument();
        expect(button.getBoundingClientRect !== undefined).toBe(true);
      }
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
