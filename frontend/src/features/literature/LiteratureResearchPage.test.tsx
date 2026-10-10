import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import LiteratureResearchPage from "./LiteratureResearchPage";
import { desktop, type LiteratureSearchHit, type KnowledgeBaseRecord, type SourceDocumentRecord } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  isDesktopRuntime: () => false,
  desktop: {
    getSetting: vi.fn(),
    setSetting: vi.fn(),
    listKnowledgeBases: vi.fn(),
    listKnowledgeDocuments: vi.fn(),
    searchLiterature: vi.fn(),
    readLiteratureSection: vi.fn(),
    desktopAgentChat: vi.fn(),
    openFileDialog: vi.fn(),
    processLiterature: vi.fn(),
    movePostgresDocument: vi.fn(),
  },
}));

const hits: LiteratureSearchHit[] = [
  { knowledge_base_id: "kb-1", document_id: "doc-1", version_id: "v1", chunk_id: "chunk-1", source_name: "钢铁研究学报", title_path: "耐磨钢热处理工艺", snippet: "马氏体组织显著提升耐磨性", score: 0.87, source_location: { page: 3 } },
  { knowledge_base_id: "kb-1", document_id: "doc-2", version_id: "v2", chunk_id: "chunk-2", source_name: "材料科学进展", title_path: "高强钢回火工艺", snippet: "回火温度影响韧性", score: 0.71, source_location: { page: 8 } },
];

const bases: KnowledgeBaseRecord[] = [
  { id: "kb-1", name: "耐磨钢", created_at: "", updated_at: "" },
  { id: "kb-2", name: "高强钢", created_at: "", updated_at: "" },
];

const documents: SourceDocumentRecord[] = [
  { id: "doc-1", knowledge_base_id: "kb-1", display_name: "耐磨钢.pdf", source_kind: "pdf", active_version_id: "v1", created_at: "", updated_at: "", metadata: { authors: ["张三"], journal: "钢铁", year: 2024 } },
  { id: "doc-2", knowledge_base_id: "kb-1", display_name: "高强钢.pdf", source_kind: "pdf", active_version_id: "v2", created_at: "", updated_at: "", metadata: {} },
];

async function searchOnce() {
  render(<LiteratureResearchPage />);
  fireEvent.change(screen.getByPlaceholderText(/输入材料、工艺、性能或研究问题/), { target: { value: "耐磨钢 热处理 马氏体" } });
  fireEvent.click(screen.getByRole("button", { name: "检索" }));
  await screen.findAllByText("耐磨钢热处理工艺");
}

describe("LiteratureResearchPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.getSetting).mockResolvedValue(null);
    vi.mocked(desktop.setSetting).mockResolvedValue(undefined);
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue(bases);
    vi.mocked(desktop.listKnowledgeDocuments).mockResolvedValue(documents);
    vi.mocked(desktop.searchLiterature).mockResolvedValue({ success: true, literature_results: hits });
    vi.mocked(desktop.desktopAgentChat).mockResolvedValue({ run_id: "run", session_id: "s", status: "completed", answer: "热点集中在耐磨钢热处理。" });
    vi.mocked(desktop.movePostgresDocument).mockResolvedValue({} as never);
  });

  it("检索结果展示作者、期刊与年份（第 27 章）", async () => {
    await searchOnce();
    expect(screen.getAllByText(/作者：张三/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/期刊：钢铁/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/年份：2024/).length).toBeGreaterThan(0);
    expect(screen.getByText(/作者：未记录/)).toBeInTheDocument();
  });

  it("可以把文献加入其它知识库（第 27 章）", async () => {
    await searchOnce();
    fireEvent.click(screen.getAllByRole("button", { name: /加入知识库/ })[0]);
    fireEvent.pointerDown(screen.getByLabelText("目标知识库"));
    fireEvent.click(await screen.findByRole("option", { name: "高强钢" }));
    fireEvent.click(screen.getByRole("button", { name: "确认加入" }));
    await waitFor(() => expect(desktop.movePostgresDocument).toHaveBeenCalledWith("doc-1", "kb-2"));
  });

  it("提供研究趋势视图（第 26 章）", async () => {
    await searchOnce();
    fireEvent.click(screen.getByRole("button", { name: "研究趋势" }));
    fireEvent.click(screen.getByRole("button", { name: /分析趋势/ }));
    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalled());
    expect(vi.mocked(desktop.desktopAgentChat).mock.calls[0][0].message).toContain("研究趋势");
    expect(await screen.findByText(/热点集中在耐磨钢热处理/)).toBeInTheDocument();
  });

  it("综述支持 10 / 20 / 50 篇选择（第 29 章）", async () => {
    await searchOnce();
    fireEvent.click(screen.getByRole("button", { name: "文献综述" }));
    expect(screen.getByText(/当前将综述 2 篇/)).toBeInTheDocument();
    fireEvent.pointerDown(screen.getByLabelText("综述篇数"));
    expect(await screen.findByRole("option", { name: "50 篇" })).toBeInTheDocument();
  });

  it("对比分析要求输出文档规定的字段表（第 28 章）", async () => {
    await searchOnce();
    fireEvent.click(screen.getAllByRole("button", { name: /加入对比/ })[0]);
    fireEvent.click(screen.getAllByRole("button", { name: /加入对比/ })[1]);
    fireEvent.click(screen.getByRole("button", { name: "生成差异分析" }));
    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalled());
    const message = vi.mocked(desktop.desktopAgentChat).mock.calls[vi.mocked(desktop.desktopAgentChat).mock.calls.length - 1][0].message;
    expect(message).toContain("研究材料");
    expect(message).toContain("差异");
  });
});
