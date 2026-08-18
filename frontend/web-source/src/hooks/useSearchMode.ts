import { useCallback, useEffect, useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { API_BASE } from "../services/api";
import { coilMatch, searchRetrieval } from "../services/search";
import { createClientRequestId } from "../services/requestId";
import type { AgentProgressState } from "../agent/AgentProgressBar";
import type { LitResult, ProductionRecord, QueryRequest, SearchResponse, TabId } from "../types/rag";
import { normalizeSearchResponse, readSearchResponse } from "../utils/searchResponse";

type NumericInputValue = number | "";
type CoilMatchResult = {
  coil_id: string;
  yield_strength: number | null;
  tensile_strength: number | null;
  elongation: number | null;
  distance: number;
};

type UseSearchModeArgs = {
  query: string;
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
  yieldRp02Value: NumericInputValue;
  tensileStrengthValue: NumericInputValue;
  elongationValue: NumericInputValue;
  topK: number;
  includeProduction: boolean;
  setIncludeProduction: Dispatch<SetStateAction<boolean>>;
  loading: boolean;
  setLoading: Dispatch<SetStateAction<boolean>>;
  setData: Dispatch<SetStateAction<SearchResponse | null>>;
  steelMark: string;
  steelGrade: string;
  setActiveTab: Dispatch<SetStateAction<TabId>>;
  setAgentLoading: Dispatch<SetStateAction<boolean>>;
  setAgentStreaming: Dispatch<SetStateAction<boolean>>;
  setAgentProgress: Dispatch<SetStateAction<AgentProgressState>>;
};

function buildAdviceRanges(args: {
  adviceModeEnabled: boolean;
  yieldRp02Min: number;
  yieldRp02Max: number;
  tensileStrengthMin: number;
  tensileStrengthMax: number;
  elongationMin: number;
  elongationMax: number;
  yieldRp02Value: NumericInputValue;
  tensileStrengthValue: NumericInputValue;
  elongationValue: NumericInputValue;
}) {
  let yieldMin = args.yieldRp02Min;
  let yieldMax = args.yieldRp02Max;
  let tensileMin = args.tensileStrengthMin;
  let tensileMax = args.tensileStrengthMax;
  let elongMin = args.elongationMin;
  let elongMax = args.elongationMax;

  if (args.adviceModeEnabled) {
    if (args.yieldRp02Value !== "" && args.yieldRp02Value > 0) {
      yieldMin = args.yieldRp02Value - 1;
      yieldMax = args.yieldRp02Value + 1;
    } else {
      yieldMin = 0;
      yieldMax = 99999;
    }
    if (args.tensileStrengthValue !== "" && args.tensileStrengthValue > 0) {
      tensileMin = args.tensileStrengthValue - 1;
      tensileMax = args.tensileStrengthValue + 1;
    } else {
      tensileMin = 0;
      tensileMax = 99999;
    }
    if (args.elongationValue !== "" && args.elongationValue > 0) {
      elongMin = Math.round((args.elongationValue - 1) * 10) / 10;
      elongMax = Math.round((args.elongationValue + 1) * 10) / 10;
    } else {
      elongMin = 0;
      elongMax = 99999;
    }
  }

  return { yieldMin, yieldMax, tensileMin, tensileMax, elongMin, elongMax };
}

async function streamAskResponse(
  controller: AbortController,
  payload: unknown,
  setAiAnswer: Dispatch<SetStateAction<string>>,
  setIsStreaming: Dispatch<SetStateAction<boolean>>,
) {
  const res = await fetch(`${API_BASE}/api/ask`, {
    signal: controller.signal,
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });

  if (!res.ok || !res.body) {
    setAiAnswer("AI 服务请求失败，请检查后端是否已配置 LLM_API_KEY。");
    setIsStreaming(false);
    return;
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let accumulated = "";

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
      const chunk = line.slice(6).trim();
      if (chunk === "[DONE]") {
        setIsStreaming(false);
        return;
      }
      if (chunk.startsWith("[ERROR]")) {
        accumulated += chunk.replace("[ERROR] ", "");
        setAiAnswer(accumulated);
        setIsStreaming(false);
        return;
      }
      try {
        const parsed = JSON.parse(chunk);
        if (parsed.content) {
          accumulated += parsed.content;
          setAiAnswer(accumulated);
        }
      } catch {
        // Ignore incomplete stream chunks.
      }
    }
  }

  setIsStreaming(false);
}

export function useSearchMode({
  query,
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
  yieldRp02Value,
  tensileStrengthValue,
  elongationValue,
  topK,
  includeProduction,
  setIncludeProduction,
  loading,
  setLoading,
  setData,
  steelMark,
  steelGrade,
  setActiveTab,
  setAgentLoading,
  setAgentStreaming,
  setAgentProgress,
}: UseSearchModeArgs) {
  const [isAIMode, setIsAIMode] = useState(true);
  const [isCompositionMode, setIsCompositionMode] = useState(false);
  const [isCoilMatchMode, setIsCoilMatchMode] = useState(false);
  const [coilMatchResults, setCoilMatchResults] = useState<CoilMatchResult[]>([]);
  const [coilMatchLoading, setCoilMatchLoading] = useState(false);
  const [coilMatchError, setCoilMatchError] = useState("");
  const [aiAnswer, setAiAnswer] = useState("");
  const [resultView, setResultView] = useState<"ai" | "rag">("ai");
  const [isStreaming, setIsStreaming] = useState(false);
  const [isProductionAI, setIsProductionAI] = useState(false);
  const aiAnswerRef = useRef<HTMLDivElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const pendingFilterSearchRef = useRef(false);
  const searchRequestRef = useRef<string | null>(null);

  const scrollAIAnswerToBottom = useCallback(() => {
    const pane = aiAnswerRef.current;
    if (!pane) return;

    const scroll = () => {
      pane.scrollTop = pane.scrollHeight;
    };

    scroll();
    window.requestAnimationFrame(() => {
      scroll();
      window.requestAnimationFrame(scroll);
    });
    window.setTimeout(scroll, 120);
  }, []);

  const adviceMode: "" | "composition" = isCompositionMode ? "composition" : "";
  const adviceModeEnabled = adviceMode !== "";

  const stopAIStreaming = useCallback(() => {
    abortControllerRef.current?.abort();
    setIsStreaming(false);
    setAgentLoading(false);
    setAgentStreaming(false);
    setAgentProgress(prev => ({ ...prev, active: false }));
  }, [setAgentLoading, setAgentProgress, setAgentStreaming]);

  useEffect(() => {
    if (!adviceModeEnabled) return;
    setIsAIMode(true);
    setIncludeProduction(true);
  }, [adviceModeEnabled, setIncludeProduction]);

  const handleAIModeToggle = useCallback(() => {
    if (adviceModeEnabled) return;
    setIsAIMode(prev => !prev);
  }, [adviceModeEnabled]);

  const handleCompositionModeToggle = useCallback(() => {
    const next = !isCompositionMode;
    setIsCompositionMode(next);
    if (next) {
      setIsCoilMatchMode(false);
      setIsAIMode(true);
      setIncludeProduction(true);
      setActiveTab("production");
    } else {
      setIncludeProduction(false);
    }
  }, [isCompositionMode, setActiveTab, setIncludeProduction]);

  const handleCoilMatchModeToggle = useCallback(() => {
    const next = !isCoilMatchMode;
    setIsCoilMatchMode(next);
    if (next) {
      setIsCompositionMode(false);
      setCoilMatchResults([]);
      setCoilMatchError("");
    }
  }, [isCoilMatchMode]);

  const handleCoilMatchSearch = useCallback(async () => {
    const yieldVal = yieldRp02Value !== "" ? Number(yieldRp02Value) : undefined;
    const tensileVal = tensileStrengthValue !== "" ? Number(tensileStrengthValue) : undefined;
    const elongVal = elongationValue !== "" ? Number(elongationValue) : undefined;

    if (yieldVal === undefined && tensileVal === undefined && elongVal === undefined) {
      setCoilMatchError("请至少输入一个性能参数");
      return;
    }

    setCoilMatchLoading(true);
    setCoilMatchError("");
    setCoilMatchResults([]);
    try {
      const resp = await coilMatch({
        yield_strength: yieldVal,
        tensile_strength: tensileVal,
        elongation: elongVal,
        tolerance: 0.1,
      });
      const json = await resp.json();
      if (json.success) {
        setCoilMatchResults(json.matches || []);
        if ((json.matches || []).length === 0) {
          setCoilMatchError("未找到匹配的钢卷，请尝试调整性能参数");
        }
      } else {
        setCoilMatchError(json.error || "匹配失败");
      }
    } catch (e: any) {
      setCoilMatchError(e.message || "请求服务器失败");
    } finally {
      setCoilMatchLoading(false);
    }
  }, [elongationValue, tensileStrengthValue, yieldRp02Value]);

  const streamAdviceAI = useCallback(async (queryText: string, contexts: string[]) => {
    setAiAnswer("");
    setIsStreaming(true);
    setIsProductionAI(true);
    setResultView("ai");
    const controller = new AbortController();
    abortControllerRef.current = controller;

    try {
      await streamAskResponse(controller, { query: queryText, contexts, mode: "advice" }, setAiAnswer, setIsStreaming);
    } catch (err) {
      if ((err as any)?.name === "AbortError" || String(err).includes("AbortError")) {
        setIsStreaming(false);
        return;
      }
      setAiAnswer("连接 AI 服务失败: " + String(err));
      setIsStreaming(false);
    }
  }, []);

  const streamProductionAI = useCallback(async (records: ProductionRecord[]) => {
    setAiAnswer("");
    setIsStreaming(true);
    setIsProductionAI(true);
    setResultView("ai");

    const contexts = records.map((row, i) => (
      `【第${i + 1}条生产数据】\n` +
      `出钢记号: ${row["出钢记号"] ?? "N/A"}\n` +
      `钢级代码: ${row["钢级代码"] ?? "N/A"}\n` +
      `板坯宽度: ${row["板坯宽度"] ?? "N/A"} mm\n` +
      `板坯厚度: ${row["板坯厚度"] ?? "N/A"} mm\n` +
      `出钢记号数量: ${row["出钢记号数量"] ?? "N/A"}\n` +
      `屈服RP0.2: ${row["屈服RP0.2"] ?? "N/A"} MPa\n` +
      `抗拉强度: ${row["抗拉强度"] ?? "N/A"} MPa\n` +
      `断后伸长率A: ${row["断后伸长率A"] ?? "N/A"} %`
    ));
    const queryText = `请逐条简要分析以下${records.length}条钢铁生产数据。每条数据用'**第X条数据：**'开头，一段话概括该条数据的出钢记号、钢级代码、板坯尺寸、屈服强度、抗拉强度和断后伸长率的特点。最后用一段话总结这批数据的整体特征。不需要列出原始数值，重点进行评价和分析。`;

    const controller = new AbortController();
    abortControllerRef.current = controller;
    try {
      await streamAskResponse(controller, { query: queryText, contexts }, setAiAnswer, setIsStreaming);
    } catch (err) {
      if ((err as any)?.name === "AbortError" || String(err).includes("AbortError")) {
        setIsStreaming(false);
        return;
      }
      setAiAnswer("连接 AI 服务失败: " + String(err));
      setIsStreaming(false);
    }
  }, []);

  const streamAIAnswer = useCallback(async (queryText: string, litResults: LitResult[]) => {
    setAiAnswer("");
    setIsStreaming(true);
    setIsProductionAI(false);
    setResultView("ai");

    const contexts = litResults.map((lit) => `来源: ${lit.paper_name} > ${lit.header_path}\n${lit.content}`);
    const controller = new AbortController();
    abortControllerRef.current = controller;
    try {
      await streamAskResponse(controller, { query: queryText, contexts }, setAiAnswer, setIsStreaming);
    } catch (err) {
      if ((err as any)?.name === "AbortError" || String(err).includes("AbortError")) {
        setIsStreaming(false);
        return;
      }
      setAiAnswer("连接 AI 服务失败: " + String(err));
      setIsStreaming(false);
    }
  }, []);

  const runAdviceAIIfNeeded = useCallback(async (response: SearchResponse) => {
    if (!adviceModeEnabled) return false;
    const modeLabel = "成分";

    if (!response.success || response.production_records.length === 0) {
      setAiAnswer(`未找到符合条件的生产数据，无法生成${modeLabel}建议。`);
      setIsStreaming(false);
      setIsProductionAI(true);
      setResultView("ai");
      return true;
    }

    const contexts = response.advice_contexts ?? [];
    const prompt = response.advice_prompt?.trim() ?? "";
    if (!prompt || contexts.length === 0) {
      setAiAnswer(`已检索到生产数据，但未找到可用的${modeLabel}标准，无法生成建议。`);
      setIsStreaming(false);
      setIsProductionAI(true);
      setResultView("ai");
      return true;
    }

    await streamAdviceAI(prompt, contexts);
    return true;
  }, [adviceModeEnabled, streamAdviceAI]);

  const buildQueryBody = useCallback((queryText: string, includeProductionFlag: boolean): QueryRequest => {
    const ranges = buildAdviceRanges({
      adviceModeEnabled,
      yieldRp02Min,
      yieldRp02Max,
      tensileStrengthMin,
      tensileStrengthMax,
      elongationMin,
      elongationMax,
      yieldRp02Value,
      tensileStrengthValue,
      elongationValue,
    });
    return {
      query_text: queryText,
      slab_width_min: slabWidthMin,
      slab_width_max: slabWidthMax,
      slab_thickness_min: slabThicknessMin,
      slab_thickness_max: slabThicknessMax,
      yield_rp02_min: ranges.yieldMin,
      yield_rp02_max: ranges.yieldMax,
      tensile_strength_min: ranges.tensileMin,
      tensile_strength_max: ranges.tensileMax,
      elongation_min: ranges.elongMin,
      elongation_max: ranges.elongMax,
      top_k: topK,
      include_production: includeProductionFlag,
      steel_mark: steelMark,
      steel_grade: steelGrade,
      advice_mode: adviceMode,
    };
  }, [
    adviceMode,
    adviceModeEnabled,
    elongationMax,
    elongationMin,
    elongationValue,
    slabThicknessMax,
    slabThicknessMin,
    slabWidthMax,
    slabWidthMin,
    steelGrade,
    steelMark,
    tensileStrengthMax,
    tensileStrengthMin,
    tensileStrengthValue,
    topK,
    yieldRp02Max,
    yieldRp02Min,
    yieldRp02Value,
  ]);

  useEffect(() => {
    if (!pendingFilterSearchRef.current) return;
    pendingFilterSearchRef.current = false;

    const fetchFilteredProduction = async () => {
      if (searchRequestRef.current) return;
      const clientRequestId = createClientRequestId();
      searchRequestRef.current = clientRequestId;
      setLoading(true);
      const body = { ...buildQueryBody("", true), client_request_id: clientRequestId };
      try {
        const res = await searchRetrieval(body);
        const json = await readSearchResponse(res);
        if (!json.success) throw new Error(json.error || "search failed");
        setData(prev => prev ? {
          ...prev,
          production_columns: json.production_columns,
          production_records: json.production_records,
          advice_mode: json.advice_mode,
          advice_prompt: json.advice_prompt,
          advice_contexts: json.advice_contexts,
          advice_record_count: json.advice_record_count,
          advice_standard_columns: json.advice_standard_columns,
          advice_standard_records: json.advice_standard_records,
        } : json);
        setActiveTab("production");
        setLoading(false);
        if (isAIMode) {
          const handled = await runAdviceAIIfNeeded(json);
          if (!handled && json.production_records.length > 0) {
            streamProductionAI(json.production_records);
          }
        }
      } catch (err) {
        console.error("高级筛选查询失败", err);
        setLoading(false);
      }
      if (searchRequestRef.current === clientRequestId) searchRequestRef.current = null;
    };

    void fetchFilteredProduction();
  }, [buildQueryBody, isAIMode, runAdviceAIIfNeeded, setActiveTab, setData, setLoading, steelGrade, steelMark, streamProductionAI]);

  const handleSearch = useCallback(async () => {
    if (isCoilMatchMode) {
      await handleCoilMatchSearch();
      return;
    }
    const shouldSearchProduction = includeProduction || adviceModeEnabled;
    if ((!query.trim() && !shouldSearchProduction) || loading || searchRequestRef.current) return;
    const clientRequestId = createClientRequestId();
    searchRequestRef.current = clientRequestId;
    setLoading(true);
    setData(null);
    setAiAnswer("");
    setResultView(isAIMode ? "ai" : "rag");

    const body = { ...buildQueryBody(query.trim(), shouldSearchProduction), client_request_id: clientRequestId };
    try {
      const res = await searchRetrieval(body);
      const json = await readSearchResponse(res);
      if (searchRequestRef.current !== clientRequestId) return;
      setData(json);
      if (adviceModeEnabled) {
        setActiveTab("production");
      }
      setLoading(false);
      if (adviceModeEnabled && isAIMode) {
        const handled = await runAdviceAIIfNeeded(json);
        if (handled && json.success && json.production_records.length > 0) {
          setActiveTab("production");
        }
      }
      if (!adviceModeEnabled && !query.trim() && shouldSearchProduction && json.success && json.production_records.length > 0) {
        setActiveTab("production");
        if (isAIMode) {
          streamProductionAI(json.production_records);
        }
      } else if (query.trim() && !adviceModeEnabled) {
        if (isAIMode && json.success && json.literature_results.length > 0) {
          streamAIAnswer(query.trim(), json.literature_results);
        }
      }
    } catch (err) {
      if (searchRequestRef.current !== clientRequestId) return;
      setData(normalizeSearchResponse({ error: String(err) }));
      setLoading(false);
    } finally {
      if (searchRequestRef.current === clientRequestId) searchRequestRef.current = null;
    }
  }, [
    adviceModeEnabled,
    buildQueryBody,
    handleCoilMatchSearch,
    includeProduction,
    isAIMode,
    isCoilMatchMode,
    loading,
    query,
    runAdviceAIIfNeeded,
    setActiveTab,
    setData,
    setLoading,
    streamAIAnswer,
    streamProductionAI,
  ]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
        void handleSearch();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleSearch]);

  useEffect(() => {
    if (resultView !== "ai" || !aiAnswer) return;
    scrollAIAnswerToBottom();
  }, [aiAnswer, isStreaming, resultView, scrollAIAnswerToBottom]);

  return {
    isAIMode,
    setIsAIMode,
    isCompositionMode,
    setIsCompositionMode,
    isCoilMatchMode,
    setIsCoilMatchMode,
    coilMatchResults,
    setCoilMatchResults,
    coilMatchLoading,
    coilMatchError,
    setCoilMatchError,
    aiAnswer,
    setAiAnswer,
    resultView,
    setResultView,
    isStreaming,
    isProductionAI,
    aiAnswerRef,
    abortControllerRef: abortControllerRef as MutableRefObject<AbortController | null>,
    pendingFilterSearchRef,
    adviceMode,
    adviceModeEnabled,
    stopAIStreaming,
    handleAIModeToggle,
    handleCompositionModeToggle,
    handleCoilMatchModeToggle,
    handleSearch,
  };
}
