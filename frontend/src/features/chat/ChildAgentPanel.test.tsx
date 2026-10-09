import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop, type AgentChildTurnRecord } from "../../bridge/desktop";
import type { AgentEventData, AgentEventEnvelope } from "../../bridge/generated/protocol";
import ChildAgentPanel from "./ChildAgentPanel";

vi.mock("../../bridge/desktop", () => ({ desktop: {
  listAgentChildTurns: vi.fn(), replayAgentChildTurn: vi.fn(), cancelAgentChildTurn: vi.fn(), listenAgentChildEvents: vi.fn(),
} }));

const child: AgentChildTurnRecord = {
  child_turn_id: "child-1", parent_turn_id: "parent-1", parent_conversation_id: "conversation-1", session_id: "conversation-1", workspace_id: "local",
  state: "executing_tools", created_at: "2026-10-09T01:00:00Z", updated_at: "2026-10-09T01:00:00Z", completed_at: null,
  agent_id: "knowledge", provider: "open_ai_compatible", model: "steel-model", task_summary: "核验 Q355B 性能要求",
};
const event = (sequence: number, data: AgentEventData): AgentEventEnvelope => ({
  protocol_version: 1, event_id: `event-${sequence}`, run_id: child.child_turn_id, conversation_id: child.session_id,
  sequence, timestamp: child.created_at, ...data,
});
let publish: ((event: AgentEventEnvelope) => void) | undefined;

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => { resolve = complete; });
  return { promise, resolve };
}

describe("ChildAgentPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks(); publish = undefined;
    vi.mocked(desktop.listAgentChildTurns).mockResolvedValue([child]);
    vi.mocked(desktop.replayAgentChildTurn).mockResolvedValue([
      event(1, { type: "tool_requested", data: { tool_call_id: "tool-1", tool_id: "steel.knowledge_search", tool_name: "知识检索", arguments: {} } }),
      event(2, { type: "tool_started", data: { tool_call_id: "tool-1" } }),
      event(3, { type: "tool_progress", data: { tool_call_id: "tool-1", progress: 40, message: "正在检索标准" } }),
    ]);
    vi.mocked(desktop.listenAgentChildEvents).mockImplementation(async (handler) => { publish = handler; return () => undefined; });
    vi.mocked(desktop.cancelAgentChildTurn).mockResolvedValue({ child: { ...child, state: "cancelled" }, replay_only: false, events: [event(4, { type: "run_completed", data: { outcome: "cancelled", assistant_message_id: null } })] });
  });

  it("replays real child progress and updates its result from streamed events", async () => {
    render(<ChildAgentPanel parentRunId="parent-1" parentConversationId="conversation-1" onResolvePermission={vi.fn()} />);
    expect(await screen.findByText("40% · 正在检索标准")).toBeInTheDocument();
    expect(desktop.listAgentChildTurns).toHaveBeenCalledWith("parent-1");
    expect(desktop.replayAgentChildTurn).toHaveBeenCalledWith("child-1", 0);
    act(() => publish?.(event(4, { type: "message_completed", data: { message_id: "answer-1", role: "assistant", content: "经检索，要求为 355 MPa。", partial: false } })));
    expect(screen.getByText("经检索，要求为 355 MPa。")).toBeInTheDocument();
    act(() => publish?.(event(5, { type: "run_completed", data: { outcome: "completed", assistant_message_id: "answer-1" } })));
    expect(screen.queryByRole("button", { name: "取消子任务" })).not.toBeInTheDocument();
  });

  it("cancels the selected child and displays the acknowledged terminal state", async () => {
    render(<ChildAgentPanel parentRunId="parent-1" parentConversationId="conversation-1" onResolvePermission={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "取消子任务" }));
    await waitFor(() => expect(desktop.cancelAgentChildTurn).toHaveBeenCalledWith("child-1"));
    await waitFor(() => expect(screen.queryByRole("button", { name: "取消子任务" })).not.toBeInTheDocument());
    expect(screen.getAllByText("已取消").length).toBeGreaterThan(0);
  });

  it("ignores children from another parent conversation", async () => {
    vi.mocked(desktop.listAgentChildTurns).mockResolvedValue([{ ...child, parent_conversation_id: "other-conversation" }]);
    render(<ChildAgentPanel parentRunId="parent-1" parentConversationId="conversation-1" onResolvePermission={vi.fn()} />);
    expect(await screen.findByText("此运行尚未委派子任务。")).toBeInTheDocument();
    expect(desktop.replayAgentChildTurn).not.toHaveBeenCalled();
  });

  it("subscribes before replay and preserves events arriving during list and replay requests", async () => {
    const subscription = deferred<() => void>();
    const listing = deferred<AgentChildTurnRecord[]>();
    const replay = deferred<AgentEventEnvelope[]>();
    vi.mocked(desktop.listenAgentChildEvents).mockImplementation((handler) => { publish = handler; return subscription.promise; });
    vi.mocked(desktop.listAgentChildTurns).mockReturnValue(listing.promise);
    vi.mocked(desktop.replayAgentChildTurn).mockReturnValue(replay.promise);
    render(<ChildAgentPanel parentRunId="parent-1" parentConversationId="conversation-1" onResolvePermission={vi.fn()} />);
    expect(desktop.listAgentChildTurns).not.toHaveBeenCalled();
    await act(async () => subscription.resolve(() => undefined));
    await waitFor(() => expect(desktop.listAgentChildTurns).toHaveBeenCalledWith("parent-1"));
    act(() => publish?.(event(2, { type: "message_completed", data: { message_id: "answer-1", role: "assistant", content: "初步结果。", partial: true } })));
    await act(async () => listing.resolve([child]));
    await waitFor(() => expect(desktop.replayAgentChildTurn).toHaveBeenCalledWith("child-1", 0));
    act(() => publish?.(event(3, { type: "message_delta", data: { message_id: "answer-1", role: "assistant", delta: "补充证据。" } })));
    await act(async () => replay.resolve([event(1, { type: "run_created", data: { state: "generating", user_message_id: "user-1" } })]));
    expect(await screen.findByText("初步结果。补充证据。")).toBeInTheDocument();
    expect(screen.getByText(/^事件 3/)).toBeInTheDocument();
  });

  it("shows a requested cancellation until the runtime confirms its terminal event", async () => {
    vi.mocked(desktop.cancelAgentChildTurn).mockResolvedValue({ child, events: [], replay_only: false });
    render(<ChildAgentPanel parentRunId="parent-1" parentConversationId="conversation-1" onResolvePermission={vi.fn()} />);
    await screen.findByText("40% · 正在检索标准");
    fireEvent.click(screen.getByRole("button", { name: "取消子任务" }));
    expect(await screen.findByRole("button", { name: "已请求取消" })).toBeDisabled();
    expect(screen.queryByText("已取消")).not.toBeInTheDocument();
    act(() => publish?.(event(4, { type: "run_completed", data: { outcome: "cancelled", assistant_message_id: null } })));
    expect(screen.queryByRole("button", { name: "已请求取消" })).not.toBeInTheDocument();
    expect(screen.getAllByText("已取消").length).toBeGreaterThan(0);
  });
});
