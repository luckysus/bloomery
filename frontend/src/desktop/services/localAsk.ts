import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createDesktopRunId, invokeDesktop, isTauriRuntime } from "./tauri";

type DesktopAskDelta = {
  run_id: string;
  delta: string;
};

export type DesktopAskPayload = {
  query: string;
  contexts: string[];
  mode?: string;
  runId?: string;
};

export async function streamDesktopAsk(
  payload: DesktopAskPayload,
  onDelta: (delta: string) => void,
  signal?: AbortSignal,
) {
  if (!isTauriRuntime()) {
    throw new Error("当前不在 Tauri 桌面运行时中");
  }
  const runId = payload.runId || createDesktopRunId("ask");
  let aborted = false;
  let activeRunId = runId;
  const abort = () => {
    aborted = true;
    void invokeDesktop<void>("desktop_cancel_llm_run", { runId }).catch(() => {});
  };
  signal?.addEventListener("abort", abort, { once: true });
  let unlisten: UnlistenFn | null = await listen<DesktopAskDelta>("desktop-ask-delta", (event) => {
    if (aborted) return;
    const runId = event.payload?.run_id || "";
    if (!activeRunId && runId) activeRunId = runId;
    if (activeRunId && runId !== activeRunId) return;
    const delta = event.payload?.delta || "";
    if (delta) onDelta(delta);
  });
  try {
    const answer = await invokeDesktop<string>("desktop_llm_ask", { request: { ...payload, runId } });
    return answer;
  } catch (error) {
    if (aborted) throw new DOMException("Aborted", "AbortError");
    throw error;
  } finally {
    signal?.removeEventListener("abort", abort);
    unlisten?.();
    unlisten = null;
  }
}
