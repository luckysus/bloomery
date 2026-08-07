import type {
  AgentError,
  AgentEventEnvelope,
  AgentRunState,
  PermissionDecision,
  PermissionRisk,
  RunOutcome,
  TaskProgressState,
  ToolOutcome,
  UsageUpdated,
} from "../../bridge/generated/protocol";

export interface AgentToolView {
  toolCallId: string;
  toolId: string;
  name: string;
  status: "requested" | "running" | ToolOutcome;
  progress: number;
  message: string | null;
  output: unknown;
  error: AgentError | null;
}

export interface AgentPermissionView {
  permissionId: string;
  toolCallId: string;
  risk: PermissionRisk;
  reason: string;
  summary: string;
  decision: PermissionDecision | null;
}

export interface AgentTaskProgressView {
  taskId: string;
  kind: string;
  state: TaskProgressState;
  progress: number;
}

export interface AgentRunView {
  runId: string;
  conversationId: string;
  sequence: number;
  state: AgentRunState;
  assistantMessageId: string | null;
  assistantText: string;
  partial: boolean;
  toolCalls: AgentToolView[];
  permissions: AgentPermissionView[];
  evidencePackId: string | null;
  citationNumbers: number[];
  usage: UsageUpdated | null;
  taskProgress: AgentTaskProgressView | null;
  outcome: RunOutcome | null;
  error: AgentError | null;
}

export function createAgentRunView(runId: string, conversationId: string): AgentRunView {
  return {
    runId,
    conversationId,
    sequence: 0,
    state: "created",
    assistantMessageId: null,
    assistantText: "",
    partial: false,
    toolCalls: [],
    permissions: [],
    evidencePackId: null,
    citationNumbers: [],
    usage: null,
    taskProgress: null,
    outcome: null,
    error: null,
  };
}

export function reduceAgentEvent(state: AgentRunView, event: AgentEventEnvelope): AgentRunView {
  if (
    event.run_id !== state.runId
    || event.conversation_id !== state.conversationId
    || event.sequence <= state.sequence
  ) {
    return state;
  }

  const next = { ...state, sequence: event.sequence };
  switch (event.type) {
    case "run_created":
      next.state = event.data.state;
      break;
    case "run_state_changed":
      next.state = event.data.current;
      break;
    case "message_delta":
      if (event.data.role === "assistant") {
        next.assistantMessageId = event.data.message_id;
        next.assistantText += event.data.delta;
      }
      break;
    case "message_completed":
      if (event.data.role === "assistant") {
        next.assistantMessageId = event.data.message_id;
        next.assistantText = event.data.content;
        next.partial = event.data.partial;
      }
      break;
    case "tool_requested":
      next.toolCalls = [
        ...next.toolCalls,
        {
          toolCallId: event.data.tool_call_id,
          toolId: event.data.tool_id,
          name: event.data.tool_name,
          status: "requested",
          progress: 0,
          message: null,
          output: null,
          error: null,
        },
      ];
      break;
    case "tool_started":
      next.toolCalls = updateTool(next.toolCalls, event.data.tool_call_id, (tool) => ({
        ...tool,
        status: "running",
      }));
      break;
    case "tool_progress":
      next.toolCalls = updateTool(next.toolCalls, event.data.tool_call_id, (tool) => ({
        ...tool,
        progress: event.data.progress,
        message: event.data.message,
      }));
      break;
    case "tool_completed":
      next.toolCalls = updateTool(next.toolCalls, event.data.tool_call_id, (tool) => ({
        ...tool,
        status: event.data.outcome,
        progress: event.data.outcome === "succeeded" ? 100 : tool.progress,
        output: event.data.output,
        error: event.data.error,
      }));
      break;
    case "permission_requested":
      next.permissions = [
        ...next.permissions,
        {
          permissionId: event.data.permission_id,
          toolCallId: event.data.tool_call_id,
          risk: event.data.risk,
          reason: event.data.reason,
          summary: event.data.summary,
          decision: null,
        },
      ];
      break;
    case "permission_resolved":
      next.permissions = next.permissions.map((permission) => permission.permissionId === event.data.permission_id
        ? { ...permission, decision: event.data.decision }
        : permission);
      break;
    case "evidence_attached":
      next.evidencePackId = event.data.evidence_pack_id;
      next.citationNumbers = [...event.data.citation_numbers];
      break;
    case "usage_updated":
      next.usage = { ...event.data };
      break;
    case "task_progress":
      next.taskProgress = { ...event.data, taskId: event.data.task_id };
      break;
    case "run_completed":
      next.outcome = event.data.outcome;
      next.state = outcomeState(event.data.outcome);
      next.assistantMessageId = event.data.assistant_message_id;
      break;
    case "error_raised":
      next.error = event.data.error;
      if (event.data.fatal) next.state = "failed";
      break;
  }
  return next;
}

export function reduceAgentEvents(
  initial: AgentRunView,
  events: AgentEventEnvelope[],
): AgentRunView {
  return [...events].sort((left, right) => left.sequence - right.sequence).reduce(reduceAgentEvent, initial);
}

function updateTool(
  tools: AgentToolView[],
  toolCallId: string,
  update: (tool: AgentToolView) => AgentToolView,
) {
  return tools.map((tool) => tool.toolCallId === toolCallId ? update(tool) : tool);
}

function outcomeState(outcome: RunOutcome): AgentRunState {
  switch (outcome) {
    case "completed": return "completed";
    case "cancelled": return "cancelled";
    case "failed": return "failed";
    case "interrupted": return "interrupted";
  }
}
