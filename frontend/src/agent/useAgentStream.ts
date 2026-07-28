import { useCallback, useRef, useState } from "react";
import type { AgentProgressStepKey } from "./AgentProgressBar";
import { AGENT_PROGRESS_ORDER, buildAgentProgress, progressFromWorkflowNode } from "./AgentProgressBar";
import type { ActiveTool, AgentMessage, AgentResponse, ToolProgressEvent } from "./types";

/**
 * SSE payload types dispatched by the backend /api/agent/stream endpoint.
 */
export type AgentSSEPayloadType =
  | "started"
  | "heartbeat"
  | "tool_progress"
  | "direct_stream"
  | "workflow"
  | "workflow_progress"
  | "node_completed"
  | "plan"
  | "tool_call"
  | "evidence"
  | "needs_confirmation"
  | "intent_detected"
  | "answer_delta"
  | "answer_done"
  | "final";

export interface AgentSSEPayload {
  type: AgentSSEPayloadType;
  [key: string]: unknown;
}

export interface AgentStreamCallbacks {
  onProgressChange: (updater: (prev: ReturnType<typeof buildAgentProgress>) => ReturnType<typeof buildAgentProgress>) => void;
  onResponseChange: (updater: (prev: AgentResponse | null) => AgentResponse | null) => void;
  onMessagesChange: (updater: (prev: AgentMessage[]) => AgentMessage[]) => void;
  onLoadingChange: (loading: boolean) => void;
  onSessionIdChange: (sessionId: string) => void;
  onError: (error: string) => void;
  onFinalResponse: (response: AgentResponse) => void;
}

export interface AgentStreamRequest {
  session_id?: string;
  message: string;
  context: Record<string, unknown>;
  confirmed_action_ids?: string[];
}

export interface UseAgentStreamReturn {
  sendStream: (apiBase: string, requestBody: AgentStreamRequest, baseMessages?: AgentMessage[], userMessage?: AgentMessage | null) => Promise<void>;
  abortStream: () => void;
  isAborted: () => boolean;
  /** P0-3: 取消当前运行（调用后端取消接口 + 中断 SSE） */
  cancelRun: () => Promise<void>;
  /** 是否正在流式传输 */
  isStreaming: boolean;
  /** P0-1: 当前活跃工具状态 */
  activeTool: ActiveTool | null;
  /** P1-5: 意图置信度 0-1 */
  intentConfidence: number | null;
  /** P1-1: 当前重试次数 */
  retryCount: number;
}

/** P1-1: 最大重试次数 */
const MAX_RETRIES = 3;
/** P1-1: 指数退避延迟（毫秒） */
const RETRY_DELAYS = [500, 1500, 4000];

/**
 * P1-1: 判断错误是否可重试
 * 网络错误、502、503、504、超时均视为可重试
 */
function isRetryableError(error: unknown): boolean {
  // 中止错误不可重试
  if (error instanceof DOMException && error.name === "AbortError") return false;
  if (error instanceof Error && error.name === "AbortError") return false;
  // TypeError 通常是网络错误（Failed to fetch）
  if (error instanceof TypeError) return true;
  if (error instanceof Error) {
    const status = (error as unknown as { status?: number }).status;
    if (status === 502 || status === 503 || status === 504) return true;
    const msg = error.message.toLowerCase();
    if (msg.includes("timeout") || msg.includes("timed out")) return true;
    if (msg.includes("network") || msg.includes("failed to fetch")) return true;
    if (msg.includes("load failed")) return true;
  }
  return false;
}

/**
 * Custom hook encapsulating the SSE streaming logic for the agent chat.
 * Handles fetch + ReadableStream parsing, SSE event splitting, and event type dispatching.
 */
export function useAgentStream(callbacks: AgentStreamCallbacks): UseAgentStreamReturn {
  const abortControllerRef = useRef<AbortController | null>(null);
  /** P0-3: 当前运行的 run_id */
  const currentRunIdRef = useRef<string | null>(null);
  /** P0-3: 记录 apiBase 供 cancelRun 使用 */
  const apiBaseRef = useRef<string>("");

  // P0-1: 工具进度展示
  const [activeTool, setActiveTool] = useState<ActiveTool | null>(null);
  // P1-5: 意图置信度
  const [intentConfidence, setIntentConfidence] = useState<number | null>(null);
  // P1-1: 重试计数
  const [retryCount, setRetryCount] = useState(0);
  // 流式状态
  const [isStreaming, setIsStreaming] = useState(false);

  const abortStream = useCallback(() => {
    abortControllerRef.current?.abort();
  }, []);

  const isAborted = useCallback(() => {
    return abortControllerRef.current?.signal.aborted ?? false;
  }, []);

  // P0-3: 取消当前运行
  const cancelRun = useCallback(async () => {
    const runId = currentRunIdRef.current;
    const apiBase = apiBaseRef.current;
    if (runId && apiBase) {
      try {
        await fetch(`${apiBase}/api/agent/runs/${runId}/cancel`, { method: "POST" });
      } catch (e) {
        console.error("Cancel failed:", e);
      }
    }
    // 同时中断 SSE 连接
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    setIsStreaming(false);
    setActiveTool(null);
  }, []);

  const sendStream = useCallback(async (
    apiBase: string,
    requestBody: AgentStreamRequest,
    baseMessages?: AgentMessage[],
    userMessage?: AgentMessage | null,
  ) => {
    const controller = new AbortController();
    abortControllerRef.current = controller;
    apiBaseRef.current = apiBase;

    // 重置状态
    setIsStreaming(true);
    setActiveTool(null);
    setIntentConfidence(null);
    setRetryCount(0);

    callbacks.onLoadingChange(true);
    callbacks.onError("");
    callbacks.onResponseChange(() => null);
    callbacks.onProgressChange(() => buildAgentProgress("analysis", "正在分析问题...", "direct"));

    if (userMessage) {
      callbacks.onMessagesChange(prev => [...(baseMessages ?? prev), userMessage]);
    }

    const appendFinalResponse = (json: AgentResponse) => {
      callbacks.onResponseChange(() => json);
      callbacks.onSessionIdChange(json.session_id);
      callbacks.onMessagesChange(prev => {
        const workingMessages = baseMessages && userMessage ? [...baseMessages, userMessage] : prev;
        const nextMessages = [...workingMessages];
        const finalMessage: AgentMessage = {
          role: "agent",
          content: json.answer || json.follow_up_questions.join("\n"),
          response: json,
        };
        if (nextMessages[nextMessages.length - 1]?.role === "agent") {
          nextMessages[nextMessages.length - 1] = finalMessage;
        } else {
          nextMessages.push(finalMessage);
        }
        callbacks.onFinalResponse(json);
        return nextMessages;
      });
    };

    /**
     * 内部流式处理函数（可重试）
     * 每次调用都会重新发起 fetch + SSE 读取
     */
    const streamMessage = async (): Promise<void> => {
      const streamResp = await fetch(`${apiBase}/api/agent/stream`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(requestBody),
        signal: controller.signal,
      });
      if (!streamResp.ok || !streamResp.body) {
        const err = new Error(await streamResp.text());
        (err as unknown as { status?: number }).status = streamResp.status;
        throw err;
      }

      const reader = streamResp.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      let streamedAnswer = "";
      let hasAgentMessage = false;
      let finalResponse: AgentResponse | null = null;
      let pendingAnswerText = "";
      let answerFrame: number | null = null;

      const flushPendingAnswer = () => {
        answerFrame = null;
        if (!pendingAnswerText) return;
        const nextAnswer = pendingAnswerText;
        callbacks.onMessagesChange(prev => {
          const nextMessages = [...prev];
          // 如果最后一条已经是 agent 消息（包括重试残留），直接替换内容
          if (nextMessages[nextMessages.length - 1]?.role === "agent") {
            nextMessages[nextMessages.length - 1] = { ...nextMessages[nextMessages.length - 1], content: nextAnswer };
            hasAgentMessage = true;
          } else {
            nextMessages.push({ role: "agent", content: nextAnswer });
            hasAgentMessage = true;
          }
          return nextMessages;
        });
      };

      const appendDelta = (delta: string) => {
        if (!delta) return;
        streamedAnswer += delta;
        pendingAnswerText = streamedAnswer;
        callbacks.onLoadingChange(false);
        callbacks.onProgressChange(prev => buildAgentProgress("answer", "正在流式生成回答...", prev.mode));
        if (answerFrame === null) {
          answerFrame = window.requestAnimationFrame(flushPendingAnswer);
        }
      };

      const handlePayload = (payload: AgentSSEPayload) => {
        if (payload.type === "started") {
          // P0-3: 记录当前 run_id
          const runId = payload.run_id as string | undefined;
          if (runId) currentRunIdRef.current = runId;
          callbacks.onProgressChange(() => ({ ...buildAgentProgress("analysis", "", "direct"), active: false, statusText: "" }));
        }
        if (payload.type === "heartbeat") return;
        if (payload.type === "tool_progress") {
          // P0-1: 工具进度展示
          const toolEvent = payload as unknown as ToolProgressEvent;
          const toolStatus = toolEvent.status;
          const toolName = toolEvent.tool_name;
          const toolElapsed = toolEvent.elapsed;
          const toolMessage = toolEvent.message;

          if (toolStatus === "started") {
            setActiveTool({ name: toolName, status: toolStatus, elapsed: toolElapsed });
          } else if (toolStatus === "running") {
            setActiveTool(prev => prev
              ? { ...prev, status: toolStatus, elapsed: toolElapsed }
              : { name: toolName, status: toolStatus, elapsed: toolElapsed });
          } else if (toolStatus === "completed" || toolStatus === "error") {
            setActiveTool(null);
          }

          callbacks.onProgressChange(() => buildAgentProgress("retrieval", toolMessage || "工具执行中...", "workflow"));
          return;
        }
        if (payload.type === "intent_detected") {
          // P1-5: 提取意图置信度
          const intent = payload.intent as { confidence?: number } | undefined;
          const directConfidence = payload.confidence as number | undefined;
          const confidence = intent?.confidence ?? directConfidence;
          if (confidence !== undefined && confidence !== null) {
            setIntentConfidence(confidence);
          }
        }
        if (payload.type === "direct_stream") {
          callbacks.onProgressChange(prev => ({ ...prev, active: false, mode: "direct" as const, statusText: "" }));
        }
        if (payload.type === "workflow" && payload.workflow) {
          callbacks.onResponseChange(prev => prev ? { ...prev, workflow: payload.workflow as AgentResponse["workflow"] } : prev);
          callbacks.onProgressChange(() => buildAgentProgress("intent", "已进入智能体工作流...", "workflow"));
        }
        if (payload.type === "workflow_progress") {
          const step = (AGENT_PROGRESS_ORDER as readonly string[]).includes(payload.step as string)
            ? (payload.step as AgentProgressStepKey)
            : "organize";
          callbacks.onProgressChange(() => buildAgentProgress(step, (payload.message as string) || "正在推进智能体工作流...", "workflow"));
        }
        if (payload.type === "node_completed" && payload.node) {
          const node = payload.node as { type?: string; outputs_summary?: string };
          const next = progressFromWorkflowNode(String(node.type || ""));
          callbacks.onProgressChange(() => buildAgentProgress(next.step, node.outputs_summary || next.text, "workflow"));
        }
        if (payload.type === "plan") {
          callbacks.onProgressChange(() => buildAgentProgress("intent", "已生成执行计划...", "workflow"));
        }
        if (payload.type === "tool_call") {
          const toolCall = payload.tool_call as { result_summary?: string } | undefined;
          callbacks.onProgressChange(() => buildAgentProgress("retrieval", toolCall?.result_summary || "正在调用工具并汇总结果...", "workflow"));
        }
        if (payload.type === "evidence") {
          const count = Array.isArray(payload.evidence) ? payload.evidence.length : 0;
          callbacks.onProgressChange(() => buildAgentProgress("organize", count ? `已整理 ${count} 条证据，正在生成回答...` : "未检索到证据，正在整理回答边界...", "workflow"));
        }
        if (payload.type === "needs_confirmation" && payload.confirmations) {
          callbacks.onResponseChange(prev => prev ? { ...prev, pending_confirmations: payload.confirmations as AgentResponse["pending_confirmations"], status: "needs_confirmation" } : prev);
        }
        if (payload.type === "answer_delta") {
          appendDelta(String(payload.delta || ""));
        }
        if (payload.type === "answer_done") {
          if (answerFrame !== null) {
            window.cancelAnimationFrame(answerFrame);
            flushPendingAnswer();
          }
          callbacks.onProgressChange(() => buildAgentProgress("answer", "回答生成完成，正在收尾...", "workflow"));
        }
        if (payload.type === "final" && payload.response) {
          finalResponse = payload.response as AgentResponse;
          // P0-3: 记录最终 run_id
          if (finalResponse.run_id) currentRunIdRef.current = finalResponse.run_id;
          appendFinalResponse(finalResponse);
          callbacks.onProgressChange(prev => ({ ...prev, active: false, completed: AGENT_PROGRESS_ORDER, current: "answer" as AgentProgressStepKey, statusText: "已完成" }));
        }
      };

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const chunks = buffer.split("\n\n");
        buffer = chunks.pop() || "";
        for (const chunk of chunks) {
          const dataLine = chunk.split("\n").find(line => line.startsWith("data: "));
          if (!dataLine) continue;
          const raw = dataLine.slice(6).trim();
          if (!raw || raw === "[DONE]") continue;
          handlePayload(JSON.parse(raw) as AgentSSEPayload);
        }
      }
      if (!finalResponse) throw new Error("智能体流式响应没有返回最终结果。");
    };

    /**
     * P1-1: 指数退避重试
     * 在可重试错误下自动重试，最多 MAX_RETRIES 次
     */
    const streamWithRetry = async (retries = 0): Promise<void> => {
      try {
        await streamMessage();
      } catch (error) {
        // 中止错误不重试，直接抛出
        const isAbort = (error instanceof DOMException && error.name === "AbortError") ||
                        (error instanceof Error && error.name === "AbortError");
        if (isAbort) throw error;

        if (retries < MAX_RETRIES && isRetryableError(error)) {
          setRetryCount(retries + 1);
          callbacks.onProgressChange(prev => ({
            ...prev,
            statusText: `正在重试...（第 ${retries + 1} 次）`,
          }));
          await new Promise(resolve => setTimeout(resolve, RETRY_DELAYS[retries]));
          return streamWithRetry(retries + 1);
        }
        throw error;
      }
    };

    /**
     * Fallback: 非流式接口
     */
    const fallbackChat = async (): Promise<void> => {
      const resp = await fetch(`${apiBase}/api/agent/chat`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(requestBody),
      });
      if (!resp.ok) throw new Error(await resp.text());
      appendFinalResponse(await resp.json() as AgentResponse);
    };

    try {
      await streamWithRetry();
    } catch (err: any) {
      // 中止错误直接返回
      if (err?.name === "AbortError") {
        callbacks.onProgressChange(prev => ({ ...prev, active: false }));
        return;
      }
      // 重试耗尽后尝试 fallback
      try {
        await fallbackChat();
      } catch (fallbackErr: any) {
        callbacks.onError(fallbackErr.message || err.message || String(fallbackErr));
        callbacks.onProgressChange(prev => ({ ...prev, active: false }));
      }
    } finally {
      setIsStreaming(false);
      setActiveTool(null);
      callbacks.onLoadingChange(false);
      if (abortControllerRef.current === controller) abortControllerRef.current = null;
    }
  }, [callbacks]);

  return { sendStream, abortStream, isAborted, cancelRun, isStreaming, activeTool, intentConfidence, retryCount };
}
