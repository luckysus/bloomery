import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ChatPage from "./ChatPage";
import { desktop, type Conversation, type EvidencePack, type Message } from "../../bridge/desktop";
import type { AgentEventEnvelope, PermissionDecision } from "../../bridge/generated/protocol";

let publishAgentEvent: ((event: AgentEventEnvelope) => void) | undefined;

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
    listenDesktopAgentDeltas: vi.fn(),
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

const conversation: Conversation = {
  id: "conversation-1",
  title: "Q355B 标准",
  created_at: "2026-08-05T10:00:00Z",
  updated_at: "2026-08-05T10:00:00Z",
  pinned: false,
  archived: false,
};

const userMessage: Message = {
  id: "message-1",
  conversation_id: conversation.id,
  role: "user",
  content: "Q355B 的屈服强度是多少？",
  response_json: null,
  created_at: conversation.created_at,
};

const evidencePack: EvidencePack = {
  id: "evidence-pack-1",
  workspace_id: "local",
  query: "Q355B strength",
  configuration: {
    knowledge_base_ids: ["kb-steel"],
    lexical_limit: 40,
    dense_limit: 40,
    candidate_limit: 20,
    rrf_k: 60,
    embedding_provider_profile_id: "embedding-1",
    embedding_model_id: "BAAI/bge-m3",
    rerank_provider_profile_id: null,
    rerank_model_id: null,
    rerank_degradation: null,
  },
  evidence: [{
    citation_number: 1,
    chunk: {
      knowledge_base_id: "kb-steel",
      document_id: "document-1",
      version_id: "version-1",
      chunk_id: "chunk-1",
      source_name: "GB 50017",
      source_location: { kind: "pdf_page", page: 12, bbox: null },
      text: "Q355B has a nominal yield strength of 355 MPa.",
      lexical_rank: 1,
      dense_rank: 1,
      rrf_score: 1,
      rerank_score: 0.9,
    },
    assets: [],
  }],
  created_at: "2026-08-05T10:00:00Z",
};

describe("ChatPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    publishAgentEvent = undefined;
    vi.mocked(desktop.listConversations).mockResolvedValue([conversation]);
    vi.mocked(desktop.searchHistory).mockResolvedValue([]);
    vi.mocked(desktop.updateConversationTitle).mockResolvedValue(undefined);
    vi.mocked(desktop.updateConversationPinned).mockResolvedValue(undefined);
    vi.mocked(desktop.archiveConversation).mockResolvedValue(undefined);
    vi.mocked(desktop.deleteConversationLocal).mockResolvedValue(undefined);
    vi.mocked(desktop.listMessages).mockResolvedValue([]);
    vi.mocked(desktop.getConversationDraft).mockResolvedValue("");
    vi.mocked(desktop.saveConversationDraft).mockResolvedValue(undefined);
    vi.mocked(desktop.listenDesktopAgentDeltas).mockResolvedValue(() => undefined);
    vi.mocked(desktop.listenAgentEvents).mockImplementation(async (handler) => {
      publishAgentEvent = handler;
      return () => undefined;
    });
    vi.mocked(desktop.replayAgentRun).mockResolvedValue([]);
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([{ id: "kb-steel", name: "Steel standards", created_at: conversation.created_at, updated_at: conversation.updated_at }]);
    vi.mocked(desktop.queryLocalKnowledge).mockResolvedValue({ ...evidencePack, evidence: [] });
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([{
      id: "chat-profile-1",
      kind: "open_ai_compatible",
      display_name: "本地钢铁模型",
      base_url: "http://127.0.0.1:8001",
      model_id: "steel-model",
      enabled: true,
      revision: 1,
      secret_generation: 1,
      secret_configured: true,
    }]);
    vi.mocked(desktop.setDefaultProvider).mockResolvedValue(undefined);
    vi.mocked(desktop.resolveKnowledgeCitation).mockResolvedValue(null);
    vi.mocked(desktop.resolveAgentPermission).mockResolvedValue(undefined);
    vi.mocked(desktop.desktopAgentChat).mockResolvedValue({
      run_id: "run-1",
      session_id: conversation.id,
      status: "completed",
      answer: "Q355B 的要求需要结合厚度和适用标准判断。",
    });
  });

  it("loads a local conversation and its message history", async () => {
    vi.mocked(desktop.listMessages).mockResolvedValue([userMessage]);
    render(<ChatPage />);

    expect(await screen.findByRole("button", { name: "Q355B 标准" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "新聊天" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "搜索聊天" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "运行状态" })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox")).toBeInTheDocument();
    expect(await screen.findByText("Q355B 的屈服强度是多少？")).toBeInTheDocument();
  });

  it("renders the copied Web conversation controls", async () => {
    render(<ChatPage />);

    await screen.findByRole("button", { name: "Q355B 标准" });
    expect(screen.getByRole("region", { name: "钢铁智能体" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Web 风格对话面板" })).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "新聊天" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "搜索聊天" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "智能搜索" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "语音" })).toBeInTheDocument();
    expect(screen.getByTitle("切换当前对话模型")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "知识库" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "模型训练" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "工艺优化" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /账户与设置/ })).not.toBeInTheDocument();
  });

  it("renders the local Rust chat panel without Web-only retrieval controls", async () => {
    render(<ChatPage />);

    await screen.findByRole("button", { name: "Q355B 标准" });
    expect(screen.getByTestId("web-agent-composer")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "知识库" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "模型训练" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "工艺优化" })).not.toBeInTheDocument();
  });

  it("sends through the local bridge without invoking Web fetch", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    vi.mocked(desktop.desktopAgentChat).mockResolvedValue({
      run_id: "run-local",
      session_id: conversation.id,
      status: "completed",
      answer: "本地 Rust 回答",
    });

    render(<ChatPage />);
    await screen.findByRole("button", { name: "Q355B 标准" });
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "本地测试" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalledWith(expect.objectContaining({
      message: "本地测试",
    })));
    expect(fetchSpy).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("connects Web-style local controls to the Rust bridge", async () => {
    render(<ChatPage />);

    await screen.findByRole("button", { name: "Q355B 标准" });
    fireEvent.click(screen.getByTitle("切换当前对话模型"));
    fireEvent.click(screen.getByRole("menuitem", { name: "steel-model" }));
    await waitFor(() => expect(desktop.setDefaultProvider).toHaveBeenCalledWith("chat", "chat-profile-1"));

    fireEvent.click(screen.getByRole("button", { name: "更多操作" }));
    fireEvent.click(screen.getByRole("button", { name: "重命名" }));
    const renameInput = screen.getByDisplayValue("Q355B 标准");
    fireEvent.change(renameInput, { target: { value: "Q355B 新标题" } });
    fireEvent.keyDown(renameInput, { key: "Enter" });
    await waitFor(() => expect(desktop.updateConversationTitle).toHaveBeenCalledWith(conversation.id, "Q355B 新标题"));

    fireEvent.click(screen.getByRole("button", { name: "置顶聊天" }));
    await waitFor(() => expect(desktop.updateConversationPinned).toHaveBeenCalledWith(conversation.id, true));
  });

  it("can disable local smart search without changing the agent request", async () => {
    render(<ChatPage />);

    await screen.findByRole("button", { name: "Q355B 标准" });
    fireEvent.click(screen.getByRole("button", { name: "智能搜索" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "只使用当前对话回答" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalledWith(expect.objectContaining({
      message: "只使用当前对话回答",
      evidencePackId: undefined,
    })));
    expect(desktop.queryLocalKnowledge).not.toHaveBeenCalled();
  });

  it("sends a message through the local agent command", async () => {
    vi.mocked(desktop.desktopAgentChat).mockImplementation(async () => {
      vi.mocked(desktop.listMessages).mockResolvedValue([userMessage, {
        ...userMessage,
        id: "message-2",
        role: "agent",
        content: "Q355B 的要求需要结合厚度和适用标准判断。",
      }]);
      return {
        run_id: "run-1",
        session_id: conversation.id,
        status: "completed",
        answer: "Q355B 的要求需要结合厚度和适用标准判断。",
      };
    });
    render(<ChatPage />);
    await screen.findByRole("button", { name: "Q355B 标准" });
    await screen.findByText("从一个具体问题开始");
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "比较 Q345B 和 Q355B" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalledWith(expect.objectContaining({
      sessionId: conversation.id,
      message: "比较 Q345B 和 Q355B",
    })));
    expect(await screen.findByText("Q355B 的要求需要结合厚度和适用标准判断。")).toBeInTheDocument();
  });

  it("keeps the empty conversation state focused on the input", async () => {
    render(<ChatPage />);

    await screen.findByRole("button", { name: "Q355B 标准" });
    expect(screen.getByText("从一个具体问题开始")).toBeInTheDocument();
    expect(screen.queryByText("例如：比较 Q345B 与 Q355B 的屈服强度要求，并指出适用标准。")).not.toBeInTheDocument();
  });

  it("retrieves selected local evidence before sending the agent request", async () => {
    vi.mocked(desktop.queryLocalKnowledge).mockResolvedValue(evidencePack);
    render(<ChatPage />);
    await screen.findByRole("button", { name: "Q355B 标准" });
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Q355B strength" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(desktop.queryLocalKnowledge).toHaveBeenCalledWith(expect.objectContaining({
      query: "Q355B strength",
      knowledge_base_ids: ["kb-steel"],
    })));
    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalledWith(expect.objectContaining({
      evidencePackId: evidencePack.id,
    })));
  });

  it("supports Web-style pasted image attachments and forwards them to the local bridge", async () => {
    render(<ChatPage />);
    await screen.findByRole("button", { name: "Q355B 标准" });

    const image = new File(["fake-image"], "炉况.png", { type: "image/png" });
    fireEvent.paste(screen.getByRole("textbox"), {
      clipboardData: { files: [image] },
    });

    expect(await screen.findByRole("img", { name: "炉况.png" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalledWith(expect.objectContaining({
      message: "请分析附加图片",
      attachments: [{
        name: "炉况.png",
        mime: "image/png",
        data: expect.any(String),
      }],
    })));
  });

  it("shows a citation entry for an assistant response with evidence", async () => {
    vi.mocked(desktop.listMessages).mockResolvedValue([{
      ...userMessage,
      role: "agent",
      content: "The yield strength is 355 MPa 文献1。",
      response_json: JSON.stringify({ evidence_pack_id: evidencePack.id, evidence: evidencePack.evidence }),
    }]);
    render(<ChatPage />);

    expect(await screen.findByText("文献1")).toBeInTheDocument();
  });

  it("renders the complete Web response blocks from the local agent payload", async () => {
    vi.mocked(desktop.listMessages).mockResolvedValue([
      userMessage,
      {
        ...userMessage,
        id: "message-2",
        role: "agent",
        content: "建议先核对钢级和板厚。",
        response_json: JSON.stringify({
          follow_up_questions: ["请补充板厚"],
          recommendations: [{
            title: "优先核对适用标准",
            summary: "先确认钢级与板厚范围。",
            details: { standard: "GB/T 1591" },
          }],
          pending_confirmations: [{
            action_id: "action-1",
            tool_name: "write_file",
            title: "写入本地分析草稿",
            permission: "confirm",
            arguments: {},
            warning: "将创建一个本地草稿文件。",
          }],
        }),
      },
    ]);
    render(<ChatPage />);

    expect(await screen.findByText("需要补充的信息")).toBeInTheDocument();
    expect(screen.getByText("推荐方案")).toBeInTheDocument();
    expect(screen.getByText("优先核对适用标准")).toBeInTheDocument();
    expect(screen.getByText("需要确认的操作")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "有用" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "本次对话问题导航" })).toBeInTheDocument();
  });

  it("fills the composer with a Web follow-up question", async () => {
    vi.mocked(desktop.listMessages).mockResolvedValue([{
      ...userMessage,
      role: "agent",
      content: "请补充信息。",
      response_json: JSON.stringify({ follow_up_questions: ["请补充板厚"] }),
    }]);
    render(<ChatPage />);

    fireEvent.click(await screen.findByRole("button", { name: "请补充板厚" }));
    expect(screen.getByRole("textbox")).toHaveValue("请补充板厚");
  });

  it("renders the standard agent run state from protocol events", async () => {
    let finish: ((response: { run_id: string; session_id: string; status: string; answer: string }) => void) | undefined;
    let latestRunId = "";
    vi.mocked(desktop.desktopAgentChat).mockImplementation(async (request) => {
      latestRunId = request.runId!;
      publishAgentEvent?.({
        protocol_version: 1,
        event_id: "event-ui-1",
        run_id: request.runId!,
        conversation_id: conversation.id,
        sequence: 1,
        timestamp: "2026-08-07T10:00:00Z",
        type: "run_state_changed",
        data: { previous: "created", current: "generating", reason: null },
      });
      publishAgentEvent?.({
        protocol_version: 1,
        event_id: "event-ui-2",
        run_id: request.runId!,
        conversation_id: conversation.id,
        sequence: 2,
        timestamp: "2026-08-07T10:00:01Z",
        type: "message_delta",
        data: { message_id: "message-2", role: "assistant", delta: "protocol answer" },
      });
      return new Promise((resolve) => {
        finish = resolve;
      });
    });
    render(<ChatPage />);
    await screen.findByRole("button", { name: "Q355B 标准" });
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "What is CE?" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    expect(await screen.findByText("protocol answer")).toBeInTheDocument();
    expect(desktop.listenAgentEvents).toHaveBeenCalled();
    finish?.({
      run_id: latestRunId,
      session_id: conversation.id,
      status: "completed",
      answer: "protocol answer",
    });
  });

  it("replays persisted agent events for the latest assistant response", async () => {
    const runId = "run-history";
    vi.mocked(desktop.listMessages).mockResolvedValue([{
      ...userMessage,
      role: "agent",
      content: "Persisted answer",
      response_json: JSON.stringify({ run_id: runId }),
    }]);
    vi.mocked(desktop.replayAgentRun).mockResolvedValue([{
      protocol_version: 1,
      event_id: "event-history-1",
      run_id: runId,
      conversation_id: conversation.id,
      sequence: 1,
      timestamp: "2026-08-07T10:00:00Z",
      type: "run_completed",
      data: { outcome: "completed", assistant_message_id: "message-2" },
    }]);

    render(<ChatPage />);

    await waitFor(() => expect(desktop.replayAgentRun).toHaveBeenCalledWith(runId));
    expect(await screen.findByText("Persisted answer")).toBeInTheDocument();
  });

  it("shows permission actions and resolves the selected decision through the desktop bridge", async () => {
    let finish: ((response: { run_id: string; session_id: string; status: string; answer: string }) => void) | undefined;
    vi.mocked(desktop.desktopAgentChat).mockImplementation(async (request) => {
      publishAgentEvent?.({
        protocol_version: 1,
        event_id: "event-permission-1",
        run_id: request.runId!,
        conversation_id: conversation.id,
        sequence: 1,
        timestamp: "2026-08-07T10:00:00Z",
        type: "permission_requested",
        data: {
          permission_id: "permission-1",
          tool_call_id: "tool-call-1",
          risk: "confirmation_required",
          reason: "The tool may modify a local file.",
          summary: "Run write_file",
        },
      });
      return new Promise((resolve) => {
        finish = resolve;
      });
    });

    render(<ChatPage />);
    await screen.findByRole("button", { name: /Q355B/ });
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Write a draft" } });
    fireEvent.click(screen.getByRole("button", { name: /发送|Send/i }));

    expect(await screen.findByText("Run write_file")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Allow once|允许一次/ }));
    await waitFor(() => expect(desktop.resolveAgentPermission).toHaveBeenCalledWith(
      "permission-1",
      "allow_once" satisfies PermissionDecision,
    ));
    finish?.({ run_id: "run-1", session_id: conversation.id, status: "completed", answer: "done" });
  });
});
