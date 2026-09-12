import { describe, expect, it } from "vitest";
import type { AgentEventData, AgentEventEnvelope } from "../../bridge/generated/protocol";
import { createAgentRunView, reduceAgentEvent } from "./agentEvents";

const runId = "run-1";
const conversationId = "conversation-1";

function event(sequence: number, data: AgentEventData): AgentEventEnvelope {
  return {
    protocol_version: 1,
    event_id: `event-${sequence}`,
    run_id: runId,
    conversation_id: conversationId,
    sequence,
    timestamp: "2026-08-07T10:00:00Z",
    ...data,
  };
}

describe("agent event reducer", () => {
  it("reconstructs streamed answers, tool progress, usage, and terminal state", () => {
    const events = [
      event(1, { type: "run_created", data: { state: "preparing", user_message_id: "message-1" } }),
      event(2, { type: "run_state_changed", data: { previous: "preparing", current: "executing_tools", reason: null } }),
      event(3, { type: "tool_requested", data: { tool_call_id: "tool-call-1", tool_id: "steel.carbon_equivalent", tool_name: "Carbon equivalent", arguments: {} } }),
      event(4, { type: "tool_progress", data: { tool_call_id: "tool-call-1", progress: 45, message: "checking composition" } }),
      event(5, { type: "tool_completed", data: { tool_call_id: "tool-call-1", outcome: "succeeded", output: { value: 0.42 }, error: null } }),
      event(6, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "The result is " } }),
      event(7, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "0.42." } }),
      event(8, { type: "usage_updated", data: {
        prompt_tokens: 20,
        completion_tokens: 8,
        total_tokens: 28,
        cache_read_tokens: 0,
        reasoning_tokens: 0,
      } }),
      event(9, { type: "run_completed", data: { outcome: "completed", assistant_message_id: "message-2" } }),
    ];

    const result = events.reduce(reduceAgentEvent, createAgentRunView(runId, conversationId));

    expect(result.state).toBe("completed");
    expect(result.assistantText).toBe("The result is 0.42.");
    expect(result.toolCalls[0]).toMatchObject({ status: "succeeded", progress: 100 });
    expect(result.usage).toEqual({
      prompt_tokens: 20,
      completion_tokens: 8,
      total_tokens: 28,
      cache_read_tokens: 0,
      reasoning_tokens: 0,
    });
    expect(result.assistantMessageId).toBe("message-2");
  });

  it("reconstructs streamed reasoning separately from the answer and preserves its duration", () => {
    const events = [
      event(1, { type: "reasoning_delta", data: { message_id: "message-2", delta: "先检查 " } }),
      event(2, { type: "reasoning_delta", data: { message_id: "message-2", delta: "材料牌号。" } }),
      event(3, { type: "reasoning_completed", data: { message_id: "message-2", duration_ms: 1842 } }),
      event(4, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "结论。" } }),
    ];

    const result = events.reduce(reduceAgentEvent, createAgentRunView(runId, conversationId));

    expect(result.reasoning).toBe("先检查 材料牌号。");
    expect(result.reasoningMs).toBe(1842);
    expect(result.reasoningStreaming).toBe(false);
    expect(result.assistantText).toBe("结论。");
  });

  it("keeps reasoning events ordered when they arrive after the answer stream", () => {
    const initial = createAgentRunView(runId, conversationId);
    const answer = reduceAgentEvent(
      initial,
      event(2, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "结论。" } }),
    );
    const withReasoning = reduceAgentEvent(
      reduceAgentEvent(
        answer,
        event(1, { type: "reasoning_delta", data: { message_id: "message-2", delta: "思考。" } }),
      ),
      event(3, { type: "reasoning_completed", data: { message_id: "message-2", duration_ms: 12 } }),
    );

    expect(withReasoning.reasoning).toBe("思考。");
    expect(withReasoning.reasoningMs).toBe(12);
    expect(withReasoning.assistantText).toBe("结论。");
  });

  it("ignores stale, duplicate, or cross-run events", () => {
    const initial = createAgentRunView(runId, conversationId);
    const accepted = reduceAgentEvent(
      reduceAgentEvent(initial, event(1, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "A" } })),
      event(1, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "B" } }),
    );
    const otherRun = { ...event(2, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "C" } }), run_id: "other-run" };

    expect(reduceAgentEvent(accepted, otherRun)).toEqual(accepted);
    expect(accepted.assistantText).toBe("A");
    expect(accepted.sequence).toBe(1);
  });

  it("buffers a future event until the missing sequence arrives", () => {
    const initial = createAgentRunView(runId, conversationId);
    const future = reduceAgentEvent(
      initial,
      event(2, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "B" } }),
    );

    expect(future.sequence).toBe(0);
    expect(future.assistantText).toBe("");
    expect(future.pendingEvents).toHaveLength(1);

    const completed = reduceAgentEvent(
      future,
      event(1, { type: "message_delta", data: { message_id: "message-2", role: "assistant", delta: "A" } }),
    );

    expect(completed.sequence).toBe(2);
    expect(completed.assistantText).toBe("AB");
    expect(completed.pendingEvents).toHaveLength(0);
    expect(future.pendingEvents).toHaveLength(1);
  });
});
