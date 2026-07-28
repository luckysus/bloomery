import { createDesktopRunId, invokeDesktop } from "./tauri";

export type DesktopSummaryResult = {
  summarized: boolean;
  summary?: string | null;
  coveredMessageId?: string | null;
  totalTokens: number;
  foldedTokens: number;
};

export async function summarizeConversation(
  conversationId: string,
  coveredMessageId?: string,
  signal?: AbortSignal,
) {
  const runId = createDesktopRunId("summary");
  const abort = () => {
    void invokeDesktop<void>("desktop_cancel_llm_run", { runId }).catch(() => {});
  };
  if (signal?.aborted) throw new DOMException("Aborted", "AbortError");
  signal?.addEventListener("abort", abort, { once: true });
  try {
    const result = await invokeDesktop<DesktopSummaryResult>("desktop_summarize_conversation", {
      request: {
        conversationId,
        coveredMessageId: coveredMessageId || null,
        runId,
      },
    });
    if (signal?.aborted) throw new DOMException("Aborted", "AbortError");
    return result;
  } finally {
    signal?.removeEventListener("abort", abort);
  }
}
