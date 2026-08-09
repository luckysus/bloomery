import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ChatPage from "./ChatPage";
import { desktop, type Conversation, type EvidencePack, type Message } from "../../bridge/desktop";
import type { AgentEventEnvelope, PermissionDecision } from "../../bridge/generated/protocol";

let publishAgentEvent: ((event: AgentEventEnvelope) => void) | undefined;

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    listConversations: vi.fn(),
    createConversation: vi.fn(),
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

const exportDesktop = desktop as typeof desktop & {
  exportConversation: (conversationId: string, outputPath: string, format: "markdown" | "json") => Promise<unknown>;
};

describe("ChatPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    publishAgentEvent = undefined;
    vi.mocked(desktop.listConversations).mockResolvedValue([conversation]);
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
    expect(await screen.findByText("Q355B 的屈服强度是多少？")).toBeInTheDocument();
  });

  it("exports the selected conversation as Markdown to the chosen path", async () => {
    vi.mocked(desktop.saveFileDialog).mockResolvedValue("C:\\Exports\\q355b.md");
    vi.mocked(exportDesktop.exportConversation).mockResolvedValue({
      format: "markdown",
      output_path: "C:\\Exports\\q355b.md",
      message_count: 1,
      bytes: 128,
    });
    render(<ChatPage />);

    await screen.findByText(/Q355B/);
    fireEvent.click(screen.getByTestId("export-conversation-markdown"));

    await waitFor(() => expect(exportDesktop.exportConversation).toHaveBeenCalledWith(
      conversation.id,
      "C:\\Exports\\q355b.md",
      "markdown",
    ));
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
    fireEvent.change(screen.getByLabelText("输入消息"), { target: { value: "比较 Q345B 和 Q355B" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalledWith(expect.objectContaining({
      sessionId: conversation.id,
      message: "比较 Q345B 和 Q355B",
    })));
    expect(await screen.findByText("Q355B 的要求需要结合厚度和适用标准判断。")).toBeInTheDocument();
  });

  it("retrieves selected local evidence before sending the agent request", async () => {
    vi.mocked(desktop.queryLocalKnowledge).mockResolvedValue(evidencePack);
    render(<ChatPage />);
    await screen.findByRole("button", { name: "Q355B 标准" });
    fireEvent.change(screen.getByLabelText("输入消息"), { target: { value: "Q355B strength" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(desktop.queryLocalKnowledge).toHaveBeenCalledWith(expect.objectContaining({
      query: "Q355B strength",
      knowledge_base_ids: ["kb-steel"],
    })));
    await waitFor(() => expect(desktop.desktopAgentChat).toHaveBeenCalledWith(expect.objectContaining({
      evidencePackId: evidencePack.id,
    })));
  });

  it("shows a citation entry for an assistant response with evidence", async () => {
    vi.mocked(desktop.listMessages).mockResolvedValue([{
      ...userMessage,
      role: "agent",
      content: "The yield strength is 355 MPa [1].",
      response_json: JSON.stringify({ evidence_pack_id: evidencePack.id, evidence: evidencePack.evidence }),
    }]);
    render(<ChatPage />);

    expect(await screen.findByRole("button", { name: /引用 1/i })).toBeInTheDocument();
  });

  it("renders the standard agent run state from protocol events", async () => {
    vi.mocked(desktop.desktopAgentChat).mockImplementation(async (request) => {
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
      return {
        run_id: request.runId!,
        session_id: conversation.id,
        status: "completed",
        answer: "protocol answer",
      };
    });
    render(<ChatPage />);
    await screen.findByRole("button", { name: "Q355B 标准" });
    fireEvent.change(screen.getByLabelText("输入消息"), { target: { value: "What is CE?" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    expect(await screen.findByTestId("agent-run-status")).toHaveTextContent(/Generating|正在生成/);
    expect(desktop.listenAgentEvents).toHaveBeenCalled();
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

    expect(await screen.findByTestId("agent-run-status")).toHaveTextContent(/本地就绪|Local ready/);
    expect(desktop.replayAgentRun).toHaveBeenCalledWith(runId);
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
    fireEvent.change(screen.getByLabelText(/输入消息|message/i), { target: { value: "Write a draft" } });
    fireEvent.click(screen.getByRole("button", { name: /发送|Send/i }));

    expect(await screen.findByText("Run write_file")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Allow once|鍏佽涓€娆?/i }));
    await waitFor(() => expect(desktop.resolveAgentPermission).toHaveBeenCalledWith(
      "permission-1",
      "allow_once" satisfies PermissionDecision,
    ));
    finish?.({ run_id: "run-1", session_id: conversation.id, status: "completed", answer: "done" });
  });
});
