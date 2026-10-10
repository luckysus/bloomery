import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop } from "../../bridge/desktop";
import type { AgentEventData, AgentEventEnvelope } from "../../bridge/generated/protocol";
import AgentRunInspector from "./AgentRunInspector";
import {
  createAgentRunView,
  reduceAgentEvents,
  type AgentPermissionView,
  type AgentRunView,
  type AgentToolView,
} from "./agentEvents";

vi.mock("../../bridge/desktop", () => ({
  isDesktopRuntime: () => false,
  desktop: {
    listAgentChildTurns: vi.fn(),
    replayAgentChildTurn: vi.fn(),
    cancelAgentChildTurn: vi.fn(),
    listenAgentChildEvents: vi.fn(),
  },
}));

const RUN_ID = "run-1";
const CONVERSATION_ID = "conversation-1";

const event = (sequence: number, data: AgentEventData): AgentEventEnvelope => ({
  protocol_version: 1,
  event_id: `event-${sequence}`,
  run_id: RUN_ID,
  conversation_id: CONVERSATION_ID,
  sequence,
  timestamp: "2026-10-10T00:00:00Z",
  ...data,
});

const tool = (
  id: string,
  status: AgentToolView["status"],
  extra: Partial<AgentToolView> = {},
): AgentToolView => ({
  toolCallId: id,
  toolId: `steel.${id}`,
  name: id,
  status,
  progress: 0,
  message: null,
  output: null,
  error: null,
  ...extra,
});

const permission = (overrides: Partial<AgentPermissionView> = {}): AgentPermissionView => ({
  permissionId: "permission-1",
  toolCallId: "tool-call-1",
  risk: "confirmation_required",
  reason: "该工具可能修改本地文件。",
  summary: "Run write_file",
  decision: null,
  ...overrides,
});

const runWith = (overrides: Partial<AgentRunView> = {}): AgentRunView => ({
  ...createAgentRunView(RUN_ID, CONVERSATION_ID),
  sequence: 1,
  ...overrides,
});

function renderInspector(
  run: AgentRunView | null,
  options: { showToolDetails?: boolean; recovery?: React.ComponentProps<typeof AgentRunInspector>["recovery"] } = {},
) {
  const onResolvePermission = vi.fn();
  const onRetry = vi.fn();
  const onResume = vi.fn();
  const renderInspectorTree = (nextRun: AgentRunView | null) => (
    <AgentRunInspector
      run={nextRun}
      recovery={options.recovery ?? null}
      showToolDetails={options.showToolDetails}
      onResolvePermission={onResolvePermission}
      onRetry={onRetry}
      onResume={onResume}
    />
  );
  const view = render(renderInspectorTree(run));
  return { view, rerender: (nextRun: AgentRunView | null) => view.rerender(renderInspectorTree(nextRun)), onResolvePermission, onRetry, onResume };
}

describe("AgentRunInspector", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listAgentChildTurns).mockResolvedValue([]);
    vi.mocked(desktop.replayAgentChildTurn).mockResolvedValue([]);
    vi.mocked(desktop.listenAgentChildEvents).mockResolvedValue(() => undefined);
  });

  it("shows a quiet empty state before a run exists", () => {
    renderInspector(null);

    expect(screen.getByRole("complementary", { name: "Agent Workspace" })).toBeInTheDocument();
    expect(screen.getByText("创建任务后，这里会显示运行状态、工具调用、权限和来源。")).toBeInTheDocument();
    expect(screen.queryByText("运行检查器")).not.toBeInTheDocument();
    expect(screen.queryByText("当前进度")).not.toBeInTheDocument();
  });

  it("reports run sequence, tool count, token usage and citations", () => {
    renderInspector(runWith({
      sequence: 7,
      toolCalls: [tool("knowledge_search", "succeeded"), tool("xgboost", "running")],
      usage: { prompt_tokens: 1200, completion_tokens: 340, total_tokens: 12345, cache_read_tokens: 0, reasoning_tokens: 0 },
      citationNumbers: [1, 2],
    }));

    expect(screen.getByText("7")).toBeInTheDocument();
    expect(screen.getByText(/^12[,.]?345$/)).toBeInTheDocument();
    expect(screen.getByText("[1]")).toBeInTheDocument();
    expect(screen.getByText("[2]")).toBeInTheDocument();
    expect(screen.getByText("工具 · knowledge_search")).toBeInTheDocument();
    expect(screen.getByText("工具 · xgboost")).toBeInTheDocument();
  });

  it("renders each tool status with its progress and error detail", () => {
    renderInspector(runWith({
      state: "executing_tools",
      toolCalls: [
        tool("knowledge_search", "succeeded"),
        tool("xgboost", "running", { progress: 40, message: "正在训练模型" }),
        tool("nsga2", "failed", { error: { code: "tool_failed", category: "internal", message: "优化器不可用", retryable: false, details: null } }),
      ],
    }));

    expect(screen.getByText("已完成")).toBeInTheDocument();
    expect(screen.getByText("40%")).toBeInTheDocument();
    expect(screen.getByText("正在训练模型")).toBeInTheDocument();
    expect(screen.getByText("失败")).toBeInTheDocument();
    expect(screen.getByText("tool_failed: 优化器不可用")).toBeInTheDocument();
  });

  it("hides tool details when the inspector is collapsed", () => {
    renderInspector(
      runWith({ toolCalls: [tool("knowledge_search", "succeeded")] }),
      { showToolDetails: false },
    );

    expect(screen.queryByText("工具 · knowledge_search")).not.toBeInTheDocument();
    expect(screen.queryByText("已完成")).not.toBeInTheDocument();
  });

  it("resolves a pending permission with the exact decision the user chose", () => {
    const { onResolvePermission } = renderInspector(runWith({ permissions: [permission()] }));

    expect(screen.getByText("需要授权")).toBeInTheDocument();
    expect(screen.getByText("授权 · Run write_file")).toBeInTheDocument();
    expect(screen.getByText("该工具可能修改本地文件。")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Agent Workspace permission allow_once" }));
    expect(onResolvePermission).toHaveBeenCalledWith("permission-1", "allow_once");

    fireEvent.click(screen.getByRole("button", { name: "Agent Workspace permission allow_session" }));
    expect(onResolvePermission).toHaveBeenLastCalledWith("permission-1", "allow_session");

    fireEvent.click(screen.getByRole("button", { name: "Agent Workspace permission deny" }));
    expect(onResolvePermission).toHaveBeenLastCalledWith("permission-1", "deny");
    expect(onResolvePermission).toHaveBeenCalledTimes(3);
  });

  it("drops the permission section once the decision is recorded", () => {
    renderInspector(runWith({ permissions: [permission({ decision: "allow_once" })] }));

    expect(screen.queryByText("需要授权")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Agent Workspace permission allow_once" })).not.toBeInTheDocument();
  });

  it("offers retry for a failed run and resume for a checkpoint recovery", () => {
    const checkpoint = { reason: "assistant_error" as const, modelCallIndex: 2, modelCalls: 3, toolCalls: 1, toolRound: 1, recoveryAttempt: 1 };
    const { onRetry, onResume } = renderInspector(
      runWith({ state: "failed", checkpoint, error: { code: "provider_timeout", category: "network", message: "模型请求超时", retryable: true, details: null } }),
      {
        recovery: {
          run: {
            id: RUN_ID,
            workspace_id: "local",
            conversation_id: CONVERSATION_ID,
            user_message_id: "message-1",
            state: "generating",
            next_sequence: 4,
            created_at: "2026-10-10T00:00:00Z",
            updated_at: "2026-10-10T00:00:00Z",
            completed_at: null,
          },
          action: { kind: "resume_from_checkpoint", data: {} },
          events: [],
          recovery_id: "recovery-1",
        },
      },
    );

    expect(screen.getByText("错误")).toBeInTheDocument();
    expect(screen.getByText("network")).toBeInTheDocument();
    expect(screen.getByText("provider_timeout: 模型请求超时")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Agent Workspace resume" }));
    expect(onResume).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Agent Workspace retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("does not offer retry once a run completed successfully", () => {
    renderInspector(runWith({
      state: "completed",
      checkpoint: { reason: "assistant_result", modelCallIndex: 1, modelCalls: 1, toolCalls: 0, toolRound: 0, recoveryAttempt: 0 },
    }));

    expect(screen.queryByRole("button", { name: "Agent Workspace retry" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Agent Workspace resume" })).not.toBeInTheDocument();
  });

  it("follows the runtime event stream as it is reduced into the run view", () => {
    const initial = createAgentRunView(RUN_ID, CONVERSATION_ID);
    const { rerender } = renderInspector(initial);

    expect(screen.getByText("运行检查器")).toBeInTheDocument();
    expect(screen.queryByText("工具 · 知识检索")).not.toBeInTheDocument();
    expect(screen.queryByText("需要授权")).not.toBeInTheDocument();

    const next = reduceAgentEvents(initial, [
      event(1, { type: "run_created", data: { state: "generating", user_message_id: "message-1" } }),
      event(2, { type: "tool_requested", data: { tool_call_id: "tool-1", tool_id: "steel.knowledge_search", tool_name: "知识检索", arguments: {} } }),
      event(3, { type: "tool_started", data: { tool_call_id: "tool-1" } }),
      event(4, { type: "tool_progress", data: { tool_call_id: "tool-1", progress: 60, message: "正在检索标准" } }),
      event(5, { type: "permission_requested", data: { permission_id: "permission-1", tool_call_id: "tool-1", risk: "dangerous", reason: "需要写入本地文件。", summary: "Run write_file" } }),
      event(6, { type: "evidence_attached", data: { evidence_pack_id: "evidence-1", citation_numbers: [1, 2, 3] } }),
    ]);
    rerender(next);

    expect(screen.getByText("运行检查器")).toBeInTheDocument();
    expect(screen.getByText("工具 · 知识检索")).toBeInTheDocument();
    expect(screen.getByText("60%")).toBeInTheDocument();
    expect(screen.getByText("正在检索标准")).toBeInTheDocument();
    expect(screen.getByText("需要授权")).toBeInTheDocument();
    expect(screen.getByText("[3]")).toBeInTheDocument();
    expect(screen.getByText("6")).toBeInTheDocument();
  });
});
