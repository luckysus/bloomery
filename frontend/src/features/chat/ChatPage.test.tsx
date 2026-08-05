import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ChatPage from "./ChatPage";
import { desktop, type Conversation, type Message } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    listConversations: vi.fn(),
    createConversation: vi.fn(),
    listMessages: vi.fn(),
    getConversationDraft: vi.fn(),
    saveConversationDraft: vi.fn(),
    listenDesktopAgentDeltas: vi.fn(),
    desktopAgentChat: vi.fn(),
    cancelDesktopRun: vi.fn(),
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

describe("ChatPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listConversations).mockResolvedValue([conversation]);
    vi.mocked(desktop.listMessages).mockResolvedValue([]);
    vi.mocked(desktop.getConversationDraft).mockResolvedValue("");
    vi.mocked(desktop.saveConversationDraft).mockResolvedValue(undefined);
    vi.mocked(desktop.listenDesktopAgentDeltas).mockResolvedValue(() => undefined);
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
});
