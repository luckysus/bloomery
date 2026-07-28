import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AgentResponse } from "../../agent/types";
import { createDesktopRunId, invokeDesktop, isTauriRuntime } from "./tauri";

type DesktopAgentDelta = {
  run_id: string;
  delta: string;
};

export type DesktopAgentChatPayload = {
  sessionId?: string;
  message: string;
  runId?: string;
};

export type DesktopConfirmCloudJobPayload = {
  conversationId: string;
  actionId: string;
  taskType: string;
  message: string;
  approved: boolean;
};

export async function streamDesktopAgent(
  payload: DesktopAgentChatPayload,
  onDelta: (delta: string) => void,
  signal: AbortSignal,
): Promise<AgentResponse> {
  if (!isTauriRuntime()) {
    throw new Error("当前不在 Tauri 桌面运行时中");
  }
  const runId = payload.runId || createDesktopRunId("agent");
  let aborted = false;
  let activeRunId = runId;
  const abort = () => {
    aborted = true;
    void invokeDesktop<void>("desktop_cancel_llm_run", { runId }).catch(() => {});
  };
  signal.addEventListener("abort", abort, { once: true });
  let unlisten: UnlistenFn | null = await listen<DesktopAgentDelta>("desktop-agent-delta", (event) => {
    if (aborted) return;
    const runId = event.payload?.run_id || "";
    if (!activeRunId && runId) activeRunId = runId;
    if (activeRunId && runId !== activeRunId) return;
    const delta = event.payload?.delta || "";
    if (delta) onDelta(delta);
  });
  try {
    const response = await invokeDesktop<AgentResponse>("desktop_agent_chat", { request: { ...payload, runId } });
    return response;
  } catch (error) {
    if (aborted) throw new DOMException("Aborted", "AbortError");
    throw error;
  } finally {
    signal.removeEventListener("abort", abort);
    unlisten?.();
    unlisten = null;
  }
}

export function confirmDesktopCloudJob(payload: DesktopConfirmCloudJobPayload): Promise<AgentResponse> {
  return invokeDesktop<AgentResponse>("desktop_confirm_cloud_job", {
    request: payload,
  });
}
