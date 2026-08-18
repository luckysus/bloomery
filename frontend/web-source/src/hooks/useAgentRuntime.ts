import { useCallback, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import {
  AGENT_PROGRESS_ORDER,
  buildAgentProgress,
  initialAgentProgress,
  progressFromWorkflowNode,
  type AgentProgressState,
  type AgentProgressStepKey,
} from "../agent/AgentProgressBar";
import type { AgentEvidence, AgentMessage, AgentPendingConfirmation, AgentResponse, AgentWebSource } from "../agent/types";
import { API_BASE } from "../services/api";
import { createClientRequestId } from "../services/requestId";
import type { QueryRequest, SearchResponse } from "../types/rag";
import {
  buildAgentChatHistory,
  parseAgentRetrievalFlowTargets,
  shouldRunAgentRetrievalOptimizationFlow,
  type AgentRetrievalFlowTargets,
} from "../utils/agentFlow";
import { readSearchResponse } from "../utils/searchResponse";

type NumericInputValue = number | "";

export type AgentAttachment = { data: string; mime: string; name: string };

type UseAgentRuntimeArgs = {
  query: string;
  setQuery: Dispatch<SetStateAction<string>>;
  agentSessionId: string;
  setAgentSessionId: Dispatch<SetStateAction<string>>;
  agentMessages: AgentMessage[];
  setAgentMessages: Dispatch<SetStateAction<AgentMessage[]>>;
  agentResponse: AgentResponse | null;
  setAgentResponse: Dispatch<SetStateAction<AgentResponse | null>>;
  agentLoading: boolean;
  setAgentLoading: Dispatch<SetStateAction<boolean>>;
  agentStreaming: boolean;
  setAgentStreaming: Dispatch<SetStateAction<boolean>>;
  setAgentError: Dispatch<SetStateAction<string>>;
  agentProgress: AgentProgressState;
  setAgentProgress: Dispatch<SetStateAction<AgentProgressState>>;
  setAgentModelMenuOpen: Dispatch<SetStateAction<boolean>>;
  setCopiedAgentMessageIndex: Dispatch<SetStateAction<number | null>>;
  editingAgentMessageIndex: number | null;
  editingAgentMessageText: string;
  cancelEditAgentMessage: () => void;
  persistAgentConversation: (sessionId: string, messages: AgentMessage[], response: AgentResponse | null) => void;
  removeConversationBySessionId: (sessionId: string) => void;
  abortControllerRef: MutableRefObject<AbortController | null>;
  setLoading: Dispatch<SetStateAction<boolean>>;
  setAiAnswer: Dispatch<SetStateAction<string>>;
  setYieldRp02Value: Dispatch<SetStateAction<NumericInputValue>>;
  setTensileStrengthValue: Dispatch<SetStateAction<NumericInputValue>>;
  setElongationValue: Dispatch<SetStateAction<NumericInputValue>>;
  setIsCompositionMode: Dispatch<SetStateAction<boolean>>;
  setIncludeProduction: Dispatch<SetStateAction<boolean>>;
  slabWidthMin: number;
  slabWidthMax: number;
  slabThicknessMin: number;
  slabThicknessMax: number;
  yieldRp02Min: number;
  yieldRp02Max: number;
  tensileStrengthMin: number;
  tensileStrengthMax: number;
  elongationMin: number;
  elongationMax: number;
  topK: number;
  steelMark: string;
  steelGrade: string;
  prepareAgentRetrievalOptimizationFlow: (response: SearchResponse, targets: AgentRetrievalFlowTargets) => void;
  optimizerComposition: Record<string, number>;
  data: SearchResponse | null;
  optimizeMaxiter: string;
  optimizePopsize: string;
  optimizeAlgorithm: string;
  activeLlmConfig: { api_key_configured?: boolean } | null | undefined;
  profileInfo: { llm?: { api_key_configured?: boolean } } | null | undefined;
  currentChatProvider: string;
  currentChatBaseUrl: string;
  currentChatModelName: string;
};

export function useAgentRuntime({
  query,
  setQuery,
  agentSessionId,
  setAgentSessionId,
  agentMessages,
  setAgentMessages,
  agentResponse,
  setAgentResponse,
  agentLoading,
  setAgentLoading,
  agentStreaming,
  setAgentStreaming,
  setAgentError,
  agentProgress,
  setAgentProgress,
  setAgentModelMenuOpen,
  setCopiedAgentMessageIndex,
  editingAgentMessageIndex,
  editingAgentMessageText,
  cancelEditAgentMessage,
  persistAgentConversation,
  removeConversationBySessionId,
  abortControllerRef,
  setLoading,
  setAiAnswer,
  setYieldRp02Value,
  setTensileStrengthValue,
  setElongationValue,
  setIsCompositionMode,
  setIncludeProduction,
  slabWidthMin,
  slabWidthMax,
  slabThicknessMin,
  slabThicknessMax,
  yieldRp02Min,
  yieldRp02Max,
  tensileStrengthMin,
  tensileStrengthMax,
  elongationMin,
  elongationMax,
  topK,
  steelMark,
  steelGrade,
  prepareAgentRetrievalOptimizationFlow,
  optimizerComposition,
  data,
  optimizeMaxiter,
  optimizePopsize,
  optimizeAlgorithm,
  activeLlmConfig,
  profileInfo,
  currentChatProvider,
  currentChatBaseUrl,
  currentChatModelName,
}: UseAgentRuntimeArgs) {
const [webSearchEnabled, setWebSearchEnabled] = useState(false);
const [agentAttachments, setAgentAttachments] = useState<AgentAttachment[]>([]);
const toggleWebSearch = useCallback(() => setWebSearchEnabled(value => !value), []);
const runAgentRetrievalOptimizationFlow = useCallback(async (
  message: string,
  targets: AgentRetrievalFlowTargets,
  baseMessages?: AgentMessage[],
) => {
  const userMessage = { role: "user" as const, content: message };
  const flowSessionId = agentSessionId || `agent_flow_${Date.now()}`;
  if (!agentSessionId) setAgentSessionId(flowSessionId);
  setAgentLoading(true);
  setAgentError("");
  setAgentResponse(null);
  setAgentProgress(buildAgentProgress("retrieval", "正在按检索模式匹配生产数据和标准记录...", "workflow"));
  setLoading(true);
  setAiAnswer("");

  const targetYieldValue = targets.yieldValue ?? "";
  const targetTensileValue = targets.tensileValue ?? "";
  const targetElongValue = targets.elongationValue ?? "";
  const yieldMin = targets.yieldValue ? targets.yieldValue - 1 : 0;
  const yieldMax = targets.yieldValue ? targets.yieldValue + 1 : 99999;
  const tensileMin = targets.tensileValue ? targets.tensileValue - 1 : 0;
  const tensileMax = targets.tensileValue ? targets.tensileValue + 1 : 99999;
  const elongMin = targets.elongationValue ? Math.round((targets.elongationValue - 1) * 10) / 10 : 0;
  const elongMax = targets.elongationValue ? Math.round((targets.elongationValue + 1) * 10) / 10 : 99999;

  setYieldRp02Value(targetYieldValue);
  setTensileStrengthValue(targetTensileValue);
  setElongationValue(targetElongValue);
  setIsCompositionMode(true);
  setIncludeProduction(true);

  if (message.trim()) {
    setAgentMessages(prev => [...(baseMessages ?? prev), userMessage]);
  }

  const body: QueryRequest = {
    client_request_id: createClientRequestId(),
    query_text: "",
    slab_width_min: slabWidthMin,
    slab_width_max: slabWidthMax,
    slab_thickness_min: slabThicknessMin,
    slab_thickness_max: slabThicknessMax,
    yield_rp02_min: yieldMin,
    yield_rp02_max: yieldMax,
    tensile_strength_min: tensileMin,
    tensile_strength_max: tensileMax,
    elongation_min: elongMin,
    elongation_max: elongMax,
    top_k: topK,
    include_production: true,
    steel_mark: steelMark,
    steel_grade: steelGrade,
    advice_mode: "composition",
  };

  try {
    const resp = await fetch(`${API_BASE}/api/search`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    const json = await readSearchResponse(resp);
    if (!resp.ok) throw new Error(json.error || `search request failed: ${resp.status}`);
    setLoading(false);

    if (!json.success || !json.production_records.length || !(json.advice_standard_records ?? []).length) {
      const failureText = "我已经按检索模式尝试匹配目标性能，但没有找到可用于工艺寻优的生产数据或成分标准记录。你可以放宽目标范围，或补充钢级/出钢记号后再试。";
      setAgentMessages(prev => {
        const workingMessages = baseMessages ? [...baseMessages, userMessage] : prev;
        const nextMessages = [...workingMessages, { role: "agent" as const, content: failureText }];
        persistAgentConversation(flowSessionId, nextMessages, null);
        return nextMessages;
      });
      setAgentProgress(prev => ({ ...prev, active: false, statusText: "未匹配到可寻优数据" }));
      return;
    }

    prepareAgentRetrievalOptimizationFlow(json, targets);
    setAgentProgress(buildAgentProgress("answer", "正在生成成分推荐与优化入口...", "workflow"));

    const contexts = json.advice_contexts ?? [];
    const prompt = json.advice_prompt?.trim() || [
      "请基于已匹配的成分标准，给出成分推荐方案。",
      "回答需要包含推荐理由、可选方案比较、冶炼注意事项，并在最后提示用户可点击工艺寻优继续优化。",
    ].join("\n");

    const workingMessages = baseMessages ? [...baseMessages, userMessage] : undefined;
    setAgentMessages(prev => {
      const base = workingMessages ?? prev;
      return [...base, {
        role: "agent" as const,
        content: "",
        action: { type: "process_optimization" as const, label: "工艺寻优" },
      }];
    });

    setAgentStreaming(true);
    const controller = new AbortController();
    abortControllerRef.current = controller;
    let accumulated = "";
    try {
      const answerResp = await fetch(`${API_BASE}/api/ask`, {
        signal: controller.signal,
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ query: prompt, contexts, mode: "advice" }),
      });
      if (!answerResp.ok || !answerResp.body) {
        throw new Error("AI 服务请求失败，请检查后端 LLM 配置。");
      }
      const reader = answerResp.body.getReader();
      const decoder = new TextDecoder();
      while (true) {
        if (controller.signal.aborted) {
          await reader.cancel().catch(() => {});
          break;
        }
        const { done, value } = await reader.read();
        if (done) break;
        const text = decoder.decode(value, { stream: true });
        const lines = text.split("\n");
        for (const line of lines) {
          if (!line.startsWith("data: ")) continue;
          const payload = line.slice(6).trim();
          if (payload === "[DONE]") {
            break;
          }
          if (payload.startsWith("[ERROR]")) {
            accumulated += payload.replace("[ERROR] ", "");
          } else {
            try {
              const parsed = JSON.parse(payload);
              if (parsed.content) accumulated += parsed.content;
            } catch {
              // Ignore incomplete stream chunks.
            }
          }
          setAgentMessages(prev => {
            const next = [...prev];
            const last = next[next.length - 1];
            if (last?.role === "agent") {
              next[next.length - 1] = {
                ...last,
                content: accumulated,
                action: { type: "process_optimization" as const, label: "工艺寻优" },
              };
            }
            return next;
          });
        }
      }
    } catch (streamErr: any) {
      if (streamErr?.name !== "AbortError" && !String(streamErr).includes("AbortError")) {
        accumulated = accumulated || `生成回答失败：${streamErr.message || String(streamErr)}`;
        setAgentMessages(prev => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last?.role === "agent") {
            next[next.length - 1] = {
              ...last,
              content: accumulated,
              action: { type: "process_optimization" as const, label: "工艺寻优" },
            };
          }
          return next;
        });
      }
    } finally {
      setAgentStreaming(false);
      setAgentMessages(prev => {
        persistAgentConversation(flowSessionId, prev, null);
        return prev;
      });
    }
    setAgentProgress(prev => ({ ...prev, active: false, completed: AGENT_PROGRESS_ORDER, current: "answer", statusText: "已生成推荐方案" }));
  } catch (err: any) {
    setLoading(false);
    setAgentError(err.message || String(err));
    setAgentProgress(prev => ({ ...prev, active: false }));
  } finally {
    setAgentLoading(false);
  }
}, [
  agentSessionId,
  elongationMin,
  elongationMax,
  prepareAgentRetrievalOptimizationFlow,
  persistAgentConversation,
  slabThicknessMax,
  slabThicknessMin,
  slabWidthMax,
  slabWidthMin,
  steelGrade,
  steelMark,
  tensileStrengthMax,
  tensileStrengthMin,
  topK,
  yieldRp02Max,
  yieldRp02Min,
]);

const runAgent = useCallback(async (message: string, confirmedActionIds: string[] = [], baseMessages?: AgentMessage[], options?: { resetSessionMemory?: boolean }) => {
  const trimmed = message.trim();
  const outgoingAttachments = agentAttachments;
  if (!trimmed && confirmedActionIds.length === 0 && outgoingAttachments.length === 0) return;
  if (trimmed && confirmedActionIds.length === 0) {
    const targets = parseAgentRetrievalFlowTargets(trimmed);
    if (shouldRunAgentRetrievalOptimizationFlow(trimmed, targets)) {
      await runAgentRetrievalOptimizationFlow(trimmed, targets, baseMessages);
      return;
    }
  }
  const controller = new AbortController();
  abortControllerRef.current = controller;
  setAgentLoading(true);
  setAgentStreaming(true);
  setAgentError("");
  setAgentResponse(null);
  setAgentProgress(buildAgentProgress("analysis", "正在分析问题...", "direct"));
  const userMessage = trimmed ? { role: "user" as const, content: trimmed } : null;
  const historyMessages = baseMessages ?? agentMessages;
  const requestBody = {
    client_request_id: createClientRequestId(),
    session_id: agentSessionId || undefined,
    message: trimmed || "继续执行已确认动作",
    context: {
      chat_history: buildAgentChatHistory(historyMessages),
      composition: optimizerComposition,
      standard_records: data?.advice_standard_records ?? [],
      maxiter: optimizeMaxiter,
      popsize: optimizePopsize,
      algorithm: optimizeAlgorithm,
      llm_config: {
        provider: currentChatProvider,
        base_url: currentChatBaseUrl,
        model_name: currentChatModelName,
        has_api_key: Boolean(activeLlmConfig?.api_key_configured ?? profileInfo?.llm?.api_key_configured),
      },
      web_search: webSearchEnabled,
      reset_session_memory: options?.resetSessionMemory ?? false,
    },
    confirmed_action_ids: confirmedActionIds,
    attachments: outgoingAttachments,
  };
  if (outgoingAttachments.length > 0) {
    setAgentAttachments([]);
  }
  if (trimmed) {
    setAgentMessages(prev => [...(baseMessages ?? prev), userMessage!]);
  }

  const appendFinalResponse = (json: AgentResponse) => {
    setAgentResponse(json);
    setAgentSessionId(json.session_id);
    setAgentMessages(prev => {
      // 从当前正在流式的 agent 消息里取出思考过程与联网来源，避免编辑重发/收尾合并时把它们丢掉
      const streamingLast = prev[prev.length - 1];
      const carriedReasoning = streamingLast?.role === "agent" ? streamingLast.reasoning : undefined;
      const carriedReasoningMs = streamingLast?.role === "agent" ? streamingLast.reasoningMs : undefined;
      const carriedWebSources = streamingLast?.role === "agent" ? streamingLast.webSources : undefined;
      const workingMessages = baseMessages && userMessage ? [...baseMessages, userMessage] : prev;
      const nextMessages = [...workingMessages];
      const finalMessage = { role: "agent" as const, content: json.answer || json.follow_up_questions.join("\n"), response: json, reasoning: carriedReasoning, reasoningMs: carriedReasoningMs, webSources: carriedWebSources };
      if (nextMessages[nextMessages.length - 1]?.role === "agent") {
        nextMessages[nextMessages.length - 1] = finalMessage;
      } else {
        nextMessages.push(finalMessage);
      }
      persistAgentConversation(json.session_id, nextMessages, json);
      return nextMessages;
    });
  };

  try {
    const streamResp = await fetch(`${API_BASE}/api/agent/stream`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(requestBody),
      signal: controller.signal,
    });
    if (!streamResp.ok || !streamResp.body) throw new Error(await streamResp.text());
    const reader = streamResp.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    let streamedAnswer = "";
    let hasAgentMessage = false;
    let finalResponse: AgentResponse | null = null;
    let pendingAnswerText = "";
    let streamEvidence: AgentEvidence[] = [];
    let streamWebSources: AgentWebSource[] = [];
    let answerFrame: number | null = null;
    let streamReasoning = "";
    let reasoningFrame: number | null = null;
    let reasoningStartTs = 0;
    let streamReasoningMs: number | undefined = undefined;

    const flushPendingAnswer = () => {
      answerFrame = null;
      if (!pendingAnswerText) return;
      const nextAnswer = pendingAnswerText;
      setAgentMessages(prev => {
        const nextMessages = [...prev];
        if (!hasAgentMessage || nextMessages[nextMessages.length - 1]?.role !== "agent") {
          nextMessages.push({ role: "agent" as const, content: nextAnswer, streamEvidence, webSources: streamWebSources.length ? streamWebSources : undefined, reasoning: streamReasoning || undefined, reasoningMs: streamReasoningMs });
          hasAgentMessage = true;
        } else {
          nextMessages[nextMessages.length - 1] = {
            ...nextMessages[nextMessages.length - 1],
            content: nextAnswer,
            streamEvidence,
            webSources: streamWebSources.length ? streamWebSources : nextMessages[nextMessages.length - 1].webSources,
            reasoning: streamReasoning || nextMessages[nextMessages.length - 1].reasoning,
            reasoningMs: streamReasoningMs,
          };
        }
        return nextMessages;
      });
    };

    const appendDelta = (delta: string) => {
      if (!delta) return;
      if (reasoningStartTs && streamReasoningMs === undefined) {
        streamReasoningMs = Date.now() - reasoningStartTs;
      }
      streamedAnswer += delta;
      pendingAnswerText = streamedAnswer;
      setAgentStreaming(true);
      setAgentProgress(prev => buildAgentProgress("answer", "正在流式生成回答...", prev.mode));
      if (answerFrame === null) {
        answerFrame = window.requestAnimationFrame(flushPendingAnswer);
      }
    };

    const flushReasoning = () => {
      reasoningFrame = null;
      if (!streamReasoning) return;
      const nextReasoning = streamReasoning;
      setAgentMessages(prev => {
        const nextMessages = [...prev];
        if (nextMessages[nextMessages.length - 1]?.role === "agent") {
          nextMessages[nextMessages.length - 1] = {
            ...nextMessages[nextMessages.length - 1],
            reasoning: nextReasoning,
            reasoningMs: streamReasoningMs,
          };
        } else {
          nextMessages.push({ role: "agent" as const, content: "", reasoning: nextReasoning, reasoningMs: streamReasoningMs, streamEvidence, webSources: streamWebSources.length ? streamWebSources : undefined });
          hasAgentMessage = true;
        }
        return nextMessages;
      });
    };

    const appendReasoning = (delta: string) => {
      if (!delta) return;
      if (reasoningStartTs === 0) reasoningStartTs = Date.now();
      streamReasoning += delta;
      setAgentStreaming(true);
      setAgentProgress(prev => buildAgentProgress("answer", "正在思考...", prev.mode));
      if (reasoningFrame === null) {
        reasoningFrame = window.requestAnimationFrame(flushReasoning);
      }
    };

    const handlePayload = (payload: any) => {
      if (payload.type === "started") {
        setAgentProgress(prev => ({ ...prev, active: false, mode: "direct", statusText: "" }));
      }
      if (payload.type === "heartbeat") {
        // SSE keep-alive, ignore
        return;
      }
      if (payload.type === "tool_progress") {
        setAgentProgress(prev => buildAgentProgress("retrieval", payload.message || "工具执行中...", prev.mode));
        return;
      }
      if (payload.type === "direct_stream") {
        setAgentProgress(prev => ({ ...prev, active: false, mode: "direct", statusText: "" }));
      }
      if (payload.type === "workflow" && payload.workflow) {
        setAgentResponse(prev => prev ? { ...prev, workflow: payload.workflow } : prev);
        setAgentProgress(buildAgentProgress("intent", "已进入智能体工作流...", "workflow"));
      }
      if (payload.type === "workflow_progress") {
        const step = AGENT_PROGRESS_ORDER.includes(payload.step) ? payload.step as AgentProgressStepKey : "organize";
        setAgentProgress(buildAgentProgress(step, payload.message || "正在推进智能体工作流...", "workflow"));
      }
      if (payload.type === "node_completed" && payload.node) {
        const next = progressFromWorkflowNode(String(payload.node.type || ""));
        setAgentProgress(buildAgentProgress(next.step, payload.node.outputs_summary || next.text, "workflow"));
      }
      if (payload.type === "plan") {
        setAgentProgress(buildAgentProgress("intent", "已生成执行计划...", "workflow"));
      }
      if (payload.type === "tool_call") {
        setAgentProgress(buildAgentProgress("retrieval", payload.tool_call?.result_summary || "正在调用工具并汇总结果...", "workflow"));
      }
      if (payload.type === "evidence") {
        streamEvidence = Array.isArray(payload.evidence) ? payload.evidence : [];
        const count = streamEvidence.length;
        if (hasAgentMessage) {
          setAgentMessages(prev => {
            const nextMessages = [...prev];
            if (nextMessages[nextMessages.length - 1]?.role === "agent") {
              nextMessages[nextMessages.length - 1] = {
                ...nextMessages[nextMessages.length - 1],
                streamEvidence,
              };
            }
            return nextMessages;
          });
        }
        setAgentProgress(buildAgentProgress("organize", count ? `已整理 ${count} 条证据，正在生成回答...` : "未检索到证据，正在整理回答边界...", "workflow"));
      }
      if (payload.type === "needs_confirmation" && payload.confirmations) {
        setAgentResponse(prev => prev ? { ...prev, pending_confirmations: payload.confirmations, status: "needs_confirmation" } : prev);
      }
      if (payload.type === "web_sources") {
        streamWebSources = Array.isArray(payload.sources) ? payload.sources : [];
        if (hasAgentMessage) {
          setAgentMessages(prev => {
            const nextMessages = [...prev];
            if (nextMessages[nextMessages.length - 1]?.role === "agent") {
              nextMessages[nextMessages.length - 1] = {
                ...nextMessages[nextMessages.length - 1],
                webSources: streamWebSources.length ? streamWebSources : undefined,
              };
            }
            return nextMessages;
          });
        }
        setAgentProgress(prev => buildAgentProgress("retrieval", `已搜索 ${streamWebSources.length} 个网页，正在生成回答...`, prev.mode));
        return;
      }
      if (payload.type === "reasoning_delta") {
        appendReasoning(String(payload.delta || ""));
      }
      if (payload.type === "answer_delta") {
        appendDelta(String(payload.delta || ""));
      }
      if (payload.type === "answer_done") {
        if (reasoningStartTs && streamReasoningMs === undefined) {
          streamReasoningMs = Date.now() - reasoningStartTs;
        }
        if (reasoningFrame !== null) {
          window.cancelAnimationFrame(reasoningFrame);
          reasoningFrame = null;
        }
        flushReasoning();
        if (answerFrame !== null) {
          window.cancelAnimationFrame(answerFrame);
          flushPendingAnswer();
        }
        setAgentProgress(prev => buildAgentProgress("answer", "回答生成完成，正在收尾...", prev.mode));
      }
      if (payload.type === "final" && payload.response) {
        finalResponse = payload.response as AgentResponse;
        appendFinalResponse(finalResponse);
        setAgentProgress(prev => ({ ...prev, active: false, completed: AGENT_PROGRESS_ORDER, current: "answer", statusText: "已完成" }));
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
        handlePayload(JSON.parse(raw));
      }
    }
    if (!finalResponse) throw new Error("智能体流式响应没有返回最终结果。");
  } catch (err: any) {
    if (err?.name === "AbortError") {
      setAgentProgress(prev => ({ ...prev, active: false }));
      setAgentLoading(false);
      setAgentStreaming(false);
      return;
    }
    try {
      const resp = await fetch(`${API_BASE}/api/agent/chat`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(requestBody),
      });
      if (!resp.ok) throw new Error(await resp.text());
      appendFinalResponse(await resp.json() as AgentResponse);
    } catch (fallbackErr: any) {
      setAgentError(fallbackErr.message || err.message || String(fallbackErr));
      setAgentProgress(prev => ({ ...prev, active: false }));
    }
  } finally {
    setAgentLoading(false);
    setAgentStreaming(false);
    if (abortControllerRef.current === controller) abortControllerRef.current = null;
  }
}, [agentSessionId, agentMessages, optimizerComposition, data?.advice_standard_records, optimizeMaxiter, optimizePopsize, optimizeAlgorithm, activeLlmConfig?.api_key_configured, profileInfo?.llm?.api_key_configured, currentChatProvider, currentChatBaseUrl, currentChatModelName, webSearchEnabled, agentAttachments, persistAgentConversation, runAgentRetrievalOptimizationFlow]);

const handleAgentSubmit = useCallback(async () => {
  const message = query.trim();
  if ((!message && agentAttachments.length === 0) || agentLoading || agentStreaming) return;
  setAgentModelMenuOpen(false);
  setQuery("");
  await runAgent(message);
}, [agentLoading, agentStreaming, query, agentAttachments, runAgent]);

const copyAgentMessage = useCallback(async (content: string, index: number) => {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(content);
    } else {
      const textarea = document.createElement("textarea");
      textarea.value = content;
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand("copy");
      document.body.removeChild(textarea);
    }
    setCopiedAgentMessageIndex(index);
    window.setTimeout(() => {
      setCopiedAgentMessageIndex(current => (current === index ? null : current));
    }, 1400);
  } catch (err) {
    console.warn("复制消息失败", err);
  }
}, []);

const submitEditedAgentMessage = useCallback(async () => {
  if (editingAgentMessageIndex === null || agentLoading) return;
  const message = editingAgentMessageText.trim();
  if (!message) return;
  const baseMessages = agentMessages.slice(0, editingAgentMessageIndex);
  cancelEditAgentMessage();
  setAgentResponse(null);
  setAgentProgress(initialAgentProgress);
  await runAgent(message, [], baseMessages, { resetSessionMemory: true });
}, [agentLoading, agentMessages, cancelEditAgentMessage, editingAgentMessageIndex, editingAgentMessageText, runAgent]);

const confirmAgentAction = useCallback(async (item: AgentPendingConfirmation, approved: boolean) => {
  setAgentLoading(true);
  setAgentError("");
  try {
    const resp = await fetch(`${API_BASE}/api/agent/confirm`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ session_id: agentSessionId, action_id: item.action_id, approved }),
    });
    if (!resp.ok) throw new Error(await resp.text());
    const json = await resp.json();
    if (json.status === "cancelled") {
      setAgentMessages(prev => {
        const nextMessages = [...prev, { role: "agent" as const, content: `已取消：${item.title}` }];
        persistAgentConversation(agentSessionId, nextMessages, agentResponse);
        return nextMessages;
      });
    } else {
      const next = json as AgentResponse;
      setAgentResponse(next);
      setAgentMessages(prev => {
        const nextMessages = [...prev, { role: "agent" as const, content: next.answer, response: next }];
        persistAgentConversation(next.session_id || agentSessionId, nextMessages, next);
        return nextMessages;
      });
    }
  } catch (err: any) {
    setAgentError(err.message || String(err));
  } finally {
    setAgentLoading(false);
  }
}, [agentResponse, agentSessionId, persistAgentConversation]);

// P3-4: 发送用户反馈（正面/负反馈+原因）
const handleAgentFeedback = useCallback(async (messageIndex: number, rating: "up" | "down", reason?: string) => {
  const message = agentMessages[messageIndex];
  try {
    await fetch(`${API_BASE}/api/agent/feedback`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        session_id: agentSessionId,
        message_id: `${agentSessionId || "session"}-${messageIndex}`,
        rating,
        reason,
        content: message?.content?.slice(0, 500) ?? "",
      }),
    });
  } catch {
    // 反馈失败不影响用户使用，静默忽略
  }
}, [agentMessages, agentSessionId]);

const clearAgentMemory = useCallback(async () => {
  if (!agentSessionId) return;
  await fetch(`${API_BASE}/api/agent/sessions/${agentSessionId}/memory`, { method: "DELETE" }).catch(() => {});
  removeConversationBySessionId(agentSessionId);
  setAgentSessionId("");
  setAgentResponse(null);
  setAgentMessages([]);
  setAgentProgress(initialAgentProgress);
}, [agentSessionId, removeConversationBySessionId, setAgentMessages, setAgentProgress, setAgentResponse, setAgentSessionId]);

  return {
    runAgent,
    handleAgentSubmit,
    copyAgentMessage,
    submitEditedAgentMessage,
    confirmAgentAction,
    handleAgentFeedback,
    clearAgentMemory,
    webSearchEnabled,
    toggleWebSearch,
    agentAttachments,
    setAgentAttachments,
  };
}
