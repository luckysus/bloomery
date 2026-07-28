import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { cancelOptimize, getOptimizeLogs, getRecentOptimizeJobs, runOptimize } from "../services/optimizer";
import type { SearchResponse } from "../types/rag";
import type { AgentRetrievalFlowTargets } from "../utils/agentFlow";

type NumericInputValue = number | "";
type StandardRecord = Record<string, unknown>;
type TabTransition = Record<number, "loading" | "fade-out" | "fade-in" | "done">;
type TabViewMode = Record<number, "result" | "log" | "pareto">;

type UseOptimizationStateArgs = {
  data: SearchResponse | null;
  setData: Dispatch<SetStateAction<SearchResponse | null>>;
  yieldRp02Value: NumericInputValue;
  tensileStrengthValue: NumericInputValue;
  elongationValue: NumericInputValue;
  setYieldRp02Value: Dispatch<SetStateAction<NumericInputValue>>;
  setTensileStrengthValue: Dispatch<SetStateAction<NumericInputValue>>;
  setElongationValue: Dispatch<SetStateAction<NumericInputValue>>;
  setIsCompositionMode: Dispatch<SetStateAction<boolean>>;
  setIncludeProduction: Dispatch<SetStateAction<boolean>>;
};

const COMPOSITION_FIELD_MAP: Record<string, string> = {
  C: "碳",
  Si: "硅",
  Mn: "锰",
  P: "磷",
  S: "硫",
  Nb: "铌",
  Ti: "钛",
  N: "氮",
};

const CHINESE_COMPOSITION_TO_SYMBOL: Record<string, string> = {
  碳: "C",
  硅: "Si",
  锰: "Mn",
  磷: "P",
  硫: "S",
  铌: "Nb",
  钛: "Ti",
  氮: "N",
};

const DEFAULT_STANDARD_COLUMNS = ["出钢记号", "板坯钢种", "C", "Si", "Mn", "P", "S", "Nb", "Ti", "N"];

function recordToComposition(record: StandardRecord | undefined): Record<string, number> {
  const composition: Record<string, number> = {};
  if (!record) return composition;
  for (const [srcKey, dstKey] of Object.entries(COMPOSITION_FIELD_MAP)) {
    const value = record[srcKey];
    if (value !== null && value !== undefined && value !== "") {
      const parsed = Number(value);
      if (Number.isFinite(parsed)) composition[dstKey] = parsed;
    }
  }
  return composition;
}

function toTargetValue(value: unknown): NumericInputValue {
  return value === null || value === undefined || value === "" ? "" : Number(value);
}

export function useOptimizationState({
  data,
  setData,
  yieldRp02Value,
  tensileStrengthValue,
  elongationValue,
  setYieldRp02Value,
  setTensileStrengthValue,
  setElongationValue,
  setIsCompositionMode,
  setIncludeProduction,
}: UseOptimizationStateArgs) {
  const [showOptimizer, setShowOptimizer] = useState(false);
  const [optimizerComposition, setOptimizerComposition] = useState<Record<string, number>>({});
  const [optimizerStandardRecords, setOptimizerStandardRecords] = useState<StandardRecord[]>([]);
  const [optimizerStandardColumns, setOptimizerStandardColumns] = useState<string[]>([]);
  const [selectedStandardIdx, setSelectedStandardIdx] = useState<Set<number>>(new Set());
  const [targetYield, setTargetYield] = useState<NumericInputValue>("");
  const [targetTensile, setTargetTensile] = useState<NumericInputValue>("");
  const [targetElong, setTargetElong] = useState<NumericInputValue>("");
  const [optimizing, setOptimizing] = useState(false);
  const [optimizeResult, setOptimizeResult] = useState<any>(null);
  const [optimizeResults, setOptimizeResults] = useState<any[]>([]);
  const [activeResultTab, setActiveResultTab] = useState(0);
  const [optimizeProgress, setOptimizeProgress] = useState("");
  const [optimizeMaxiter, setOptimizeMaxiter] = useState("200");
  const [optimizePopsize, setOptimizePopsize] = useState("100");
  const [optimizeAlgorithm, setOptimizeAlgorithm] = useState("nsga2");
  const [optimizeError, setOptimizeError] = useState("");
  const [optimizeLogs, setOptimizeLogs] = useState<string[]>([]);
  const optimizeLogRef = useRef<HTMLDivElement>(null);
  const optimizeAbortRef = useRef(false);
  const optimizeAbortControllerRef = useRef<AbortController | null>(null);
  const [optimizeStopping, setOptimizeStopping] = useState(false);
  const [optimizerRestoring, setOptimizerRestoring] = useState(false);
  const [currentOptimizingIdx, setCurrentOptimizingIdx] = useState<number | null>(null);
  const [perRecordLogs, setPerRecordLogs] = useState<Record<number, string[]>>({});
  const [paretoIdx, setParetoIdx] = useState<Record<number, number>>({});
  const [tabViewMode, setTabViewMode] = useState<TabViewMode>({});
  const optimizeLogsRef = useRef<string[]>([]);
  const [tabTransition, setTabTransition] = useState<TabTransition>({});

  const buildRestoredStandardRecord = useCallback((job: any) => {
    const input = job?.input_payload ?? {};
    const standardRecord = input.standard_record && typeof input.standard_record === "object" && !Array.isArray(input.standard_record)
      ? { ...input.standard_record }
      : {};
    const composition = input.composition && typeof input.composition === "object"
      ? input.composition
      : (job?.result?.used_composition && typeof job.result.used_composition === "object" ? job.result.used_composition : {});
    for (const [rawKey, rawValue] of Object.entries(composition)) {
      const key = CHINESE_COMPOSITION_TO_SYMBOL[rawKey] ?? rawKey;
      if (["C", "Si", "Mn", "P", "S", "Nb", "Ti", "N"].includes(key) && standardRecord[key] === undefined) {
        standardRecord[key] = rawValue as unknown;
      }
    }
    if (standardRecord["出钢记号"] === undefined) {
      standardRecord["出钢记号"] = input.steel_mark || job?.job_id || "上次优化";
    }
    return standardRecord;
  }, []);

  const restoreLatestOptimizationResults = useCallback(async () => {
    setOptimizerRestoring(true);
    try {
      const resp = await getRecentOptimizeJobs(30);
      if (!resp.ok) return false;
      const jobs = await resp.json();
      if (!Array.isArray(jobs) || jobs.length === 0) return false;

      const usableJobs = jobs.filter((job: any) => {
        const result = job?.result;
        return job?.status === "completed" && result && typeof result === "object" && result.success;
      });
      if (usableJobs.length === 0) return false;

      const latestBatchId = usableJobs.find((job: any) => job?.input_payload?.batch_id)?.input_payload?.batch_id;
      const sameLegacyRun = (job: any, firstJob: any) => {
        const input = job?.input_payload ?? {};
        const firstInput = firstJob?.input_payload ?? {};
        if (input.batch_id || firstInput.batch_id) return false;
        const sameParams = input.algorithm === firstInput.algorithm
          && Number(input.maxiter ?? 0) === Number(firstInput.maxiter ?? 0)
          && Number(input.popsize ?? 0) === Number(firstInput.popsize ?? 0)
          && JSON.stringify(input.targets ?? {}) === JSON.stringify(firstInput.targets ?? {});
        if (!sameParams) return false;
        const t1 = Date.parse(String(job?.created_at || ""));
        const t0 = Date.parse(String(firstJob?.created_at || ""));
        if (!Number.isFinite(t1) || !Number.isFinite(t0)) return true;
        return Math.abs(t1 - t0) <= 10 * 60 * 1000;
      };
      const restoredJobs = latestBatchId
        ? usableJobs.filter((job: any) => job?.input_payload?.batch_id === latestBatchId)
        : usableJobs.filter((job: any) => sameLegacyRun(job, usableJobs[0]));
      restoredJobs.sort((a: any, b: any) => {
        const ai = Number.isFinite(Number(a?.input_payload?.record_index)) ? Number(a.input_payload.record_index) : 0;
        const bi = Number.isFinite(Number(b?.input_payload?.record_index)) ? Number(b.input_payload.record_index) : 0;
        if (ai !== bi) return ai - bi;
        return String(a?.created_at || "").localeCompare(String(b?.created_at || ""));
      });

      const candidateRecords = restoredJobs.find((job: any) => (
        Array.isArray(job?.input_payload?.candidate_standard_records)
        && job.input_payload.candidate_standard_records.length > 0
      ))?.input_payload?.candidate_standard_records;
      const restoredRecords = Array.isArray(candidateRecords) && candidateRecords.length > 0
        ? candidateRecords.filter((record: any) => record && typeof record === "object")
        : restoredJobs.map(buildRestoredStandardRecord);
      const restoredColumns = (() => {
        const explicit = restoredJobs.find((job: any) => Array.isArray(job?.input_payload?.standard_columns) && job.input_payload.standard_columns.length > 0)
          ?.input_payload?.standard_columns;
        if (Array.isArray(explicit) && explicit.length > 0) return explicit;
        return DEFAULT_STANDARD_COLUMNS.filter(col => col === "出钢记号" || restoredRecords.some((record: any) => record?.[col] !== undefined));
      })();

      const restoredResults = restoredJobs.map((job: any, fallbackIdx: number) => {
        const input = job?.input_payload ?? {};
        const originalIdx = Number.isFinite(Number(input.record_index)) ? Number(input.record_index) : fallbackIdx;
        const idx = Array.isArray(candidateRecords) && candidateRecords.length > 0 ? originalIdx : fallbackIdx;
        const record = restoredRecords[idx] ?? restoredRecords[fallbackIdx] ?? buildRestoredStandardRecord(job);
        const steelMark = String(input.steel_mark || record?.["出钢记号"] || `记录${idx + 1}`);
        return {
          record,
          result: { ...(job.result ?? {}), job_id: job.job_id },
          idx,
          steelMark,
        };
      });

      const firstInput = restoredJobs[0]?.input_payload ?? {};
      const restoredTargets = firstInput.targets ?? {};
      const yieldTarget = restoredTargets.yield_strength?.min ?? restoredTargets.yield_strength?.max ?? "";
      const tensileTarget = restoredTargets.tensile_strength?.min ?? restoredTargets.tensile_strength?.max ?? "";
      const elongTarget = restoredTargets.elongation?.min ?? restoredTargets.elongation?.max ?? "";

      setOptimizeResults(restoredResults);
      setOptimizeResult(restoredResults.length === 1 ? restoredResults[0].result : null);
      setOptimizerStandardRecords(restoredRecords);
      setOptimizerStandardColumns(restoredColumns);
      setSelectedStandardIdx(new Set(restoredResults.map((item: any) => item.idx)));
      setTargetYield(toTargetValue(yieldTarget));
      setTargetTensile(toTargetValue(tensileTarget));
      setTargetElong(toTargetValue(elongTarget));
      setOptimizeMaxiter(String(firstInput.maxiter ?? optimizeMaxiter));
      setOptimizePopsize(String(firstInput.popsize ?? optimizePopsize));
      setOptimizeAlgorithm(String(firstInput.algorithm ?? optimizeAlgorithm));
      setPerRecordLogs(restoredJobs.reduce((acc: Record<number, string[]>, job: any, fallbackIdx: number) => {
        const input = job?.input_payload ?? {};
        const originalIdx = Number.isFinite(Number(input.record_index)) ? Number(input.record_index) : fallbackIdx;
        const idx = Array.isArray(candidateRecords) && candidateRecords.length > 0 ? originalIdx : fallbackIdx;
        acc[idx] = Array.isArray(job.logs) ? job.logs : [];
        return acc;
      }, {}));
      setTabTransition(restoredResults.reduce((acc: TabTransition, item: any) => {
        acc[item.idx] = "done";
        return acc;
      }, {}));
      setActiveResultTab(0);
      setOptimizeError("");
      return true;
    } catch (err) {
      console.warn("恢复最近优化结果失败", err);
      return false;
    } finally {
      setOptimizerRestoring(false);
    }
  }, [buildRestoredStandardRecord, optimizeAlgorithm, optimizeMaxiter, optimizePopsize]);

  const prepareAgentRetrievalOptimizationFlow = useCallback((response: SearchResponse, targets: AgentRetrievalFlowTargets) => {
    const records = response.advice_standard_records ?? [];
    setData(response);
    setIsCompositionMode(true);
    setIncludeProduction(true);
    setYieldRp02Value(targets.yieldValue ?? "");
    setTensileStrengthValue(targets.tensileValue ?? "");
    setElongationValue(targets.elongationValue ?? "");
    setSelectedStandardIdx(new Set(records.map((_, index) => index)));

    const firstRecord = records[0] as StandardRecord | undefined;
    setOptimizerComposition(recordToComposition(firstRecord));
    setTargetYield(targets.yieldValue ?? "");
    setTargetTensile(targets.tensileValue ?? "");
    setTargetElong(targets.elongationValue ?? "");
    setOptimizeResult(null);
    setOptimizeResults([]);
    setActiveResultTab(0);
    setOptimizeProgress("");
    setOptimizeError("");
    setOptimizeLogs([]);
    optimizeLogsRef.current = [];
    setCurrentOptimizingIdx(null);
  }, [setData, setElongationValue, setIncludeProduction, setIsCompositionMode, setTensileStrengthValue, setYieldRp02Value]);

  const handleOpenOptimizer = useCallback(async () => {
    const records = data?.advice_standard_records ?? [];
    if (records.length > 0) {
      const normalizedRecords = records.filter(Boolean) as StandardRecord[];
      setOptimizerStandardRecords(normalizedRecords);
      setOptimizerStandardColumns(data?.advice_standard_columns ?? []);
      if (optimizing || optimizeResults.length > 0 || optimizeResult) {
        const optimizedSelection = new Set(
          optimizeResults
            .map((item: any) => Number(item?.idx))
            .filter((idx: number) => Number.isFinite(idx) && idx >= 0 && idx < normalizedRecords.length)
        );
        if (optimizedSelection.size > 0) {
          setSelectedStandardIdx(optimizedSelection);
        }
        setShowOptimizer(true);
        return;
      }
    }

    if (optimizing || optimizeResults.length > 0 || optimizeResult) {
      setShowOptimizer(true);
      return;
    }
    if (records.length === 0) {
      setShowOptimizer(true);
      await restoreLatestOptimizationResults();
      return;
    }
    setSelectedStandardIdx(new Set(records.map((_: any, i: number) => i)));
    setOptimizerComposition(recordToComposition(records[0] as StandardRecord | undefined));
    setTargetYield(yieldRp02Value);
    setTargetTensile(tensileStrengthValue);
    setTargetElong(elongationValue);
    setOptimizeResult(null);
    setOptimizeResults([]);
    setActiveResultTab(0);
    setOptimizeProgress("");
    setOptimizeError("");
    setShowOptimizer(true);
  }, [
    data,
    elongationValue,
    optimizeResult,
    optimizeResults,
    optimizing,
    restoreLatestOptimizationResults,
    tensileStrengthValue,
    yieldRp02Value,
  ]);

  const activeOptimizerStandardRecords = optimizerStandardRecords.length > 0
    ? optimizerStandardRecords
    : ((data?.advice_standard_records && data.advice_standard_records.length > 0)
      ? data.advice_standard_records.filter(Boolean) as StandardRecord[]
      : []);
  const activeOptimizerStandardColumns = optimizerStandardColumns.length > 0
    ? optimizerStandardColumns
    : ((data?.advice_standard_records && data.advice_standard_records.length > 0)
      ? (data?.advice_standard_columns ?? [])
      : []);
  const hasOptimizerData = activeOptimizerStandardRecords.length > 0;

  const handleSelectStandard = useCallback((idx: number) => {
    setSelectedStandardIdx(prev => {
      const next = new Set(prev);
      if (next.has(idx)) {
        next.delete(idx);
      } else {
        next.add(idx);
      }
      return next;
    });
    setOptimizerComposition(recordToComposition(activeOptimizerStandardRecords[idx]));
  }, [activeOptimizerStandardRecords]);

  const handleToggleAllStandards = useCallback(() => {
    const records = activeOptimizerStandardRecords;
    if (selectedStandardIdx.size === records.length) {
      setSelectedStandardIdx(new Set());
    } else {
      setSelectedStandardIdx(new Set(records.map((_: any, i: number) => i)));
    }
  }, [activeOptimizerStandardRecords, selectedStandardIdx.size]);

  useEffect(() => {
    if (!optimizing) return;
    const poll = async () => {
      try {
        const resp = await getOptimizeLogs();
        const d = await resp.json();
        if (d.logs) {
          setOptimizeLogs(d.logs);
          optimizeLogsRef.current = d.logs;
        }
      } catch {
        // Log polling is best-effort while a remote optimize job is running.
      }
    };
    poll();
    const timer = window.setInterval(poll, 1500);
    return () => window.clearInterval(timer);
  }, [optimizing]);

  const prevOptimizingRef = useRef(false);
  useEffect(() => {
    if (prevOptimizingRef.current && !optimizing) {
      getOptimizeLogs()
        .then(r => r.json())
        .then(d => {
          if (d.logs) {
            setOptimizeLogs(d.logs);
            optimizeLogsRef.current = d.logs;
          }
        })
        .catch(() => {});
    }
    prevOptimizingRef.current = optimizing;
  }, [optimizing]);

  useEffect(() => {
    if (optimizeLogRef.current) {
      optimizeLogRef.current.scrollTop = optimizeLogRef.current.scrollHeight;
    }
  }, [optimizeLogs]);

  const waitForOptimizeIdle = useCallback(async (timeoutMs = 20000) => {
    const startedAt = Date.now();
    while (Date.now() - startedAt < timeoutMs) {
      try {
        const resp = await getOptimizeLogs();
        const d = await resp.json();
        if (d.logs) {
          setOptimizeLogs(d.logs);
          optimizeLogsRef.current = d.logs;
        }
        if (!d.running) return true;
      } catch {
        return true;
      }
      await new Promise(r => window.setTimeout(r, 500));
    }
    return false;
  }, []);

  const requestOptimizeCancel = useCallback(async () => {
    optimizeAbortRef.current = true;
    optimizeAbortControllerRef.current?.abort();
    setOptimizeStopping(true);
    try {
      await cancelOptimize();
    } catch {
      // Local abort still updates the UI; backend cancel is best-effort.
    }
  }, []);

  const handleOptimize = useCallback(async () => {
    const selectedIndices = Array.from(selectedStandardIdx).sort();
    if (selectedIndices.length === 0) return;

    try {
      const logResp = await getOptimizeLogs();
      const logData = await logResp.json();
      if (logData.running) {
        setOptimizeLogs(prev => [...prev, "\n上一轮优化仍在停止中，正在等待后端释放..."]);
        await cancelOptimize().catch(() => {});
        const idle = await waitForOptimizeIdle();
        if (!idle) {
          setOptimizeError("上一轮优化还没有完全停止，请稍后再试。");
          setOptimizeStopping(false);
          return;
        }
      }
    } catch {
      // If the log probe fails, let the optimize request surface the real error.
    }

    setOptimizeLogs([]);
    optimizeLogsRef.current = [];
    setOptimizing(true);
    setOptimizeError("");
    setOptimizeResult(null);
    setOptimizeResults([]);
    setActiveResultTab(0);
    setOptimizeProgress("");
    optimizeAbortRef.current = false;
    setOptimizeStopping(false);
    setPerRecordLogs({});
    setTabViewMode({});
    setTabTransition({});

    const targets: Record<string, any> = {};
    targets.yield_strength = targetYield !== ""
      ? { min: targetYield, max: targetYield, weight: 1.0 }
      : { min: null, max: null, weight: 1.0 };
    targets.tensile_strength = targetTensile !== ""
      ? { min: targetTensile, max: targetTensile, weight: 1.0 }
      : { min: null, max: null, weight: 1.0 };
    targets.elongation = targetElong !== ""
      ? { min: targetElong, max: targetElong, weight: 0.8 }
      : { min: null, max: null, weight: 0.8 };

    const results: any[] = [];
    const optimizeBatchId = `opt-batch-${Date.now()}-${Math.random().toString(16).slice(2)}`;

    try {
      for (let i = 0; i < selectedIndices.length; i++) {
        if (optimizeAbortRef.current) {
          setOptimizeLogs(prev => [...prev, `\n用户已停止优化，已完成 ${i}/${selectedIndices.length} 条`]);
          break;
        }
        const idx = selectedIndices[i];
        const record = activeOptimizerStandardRecords[idx];
        const steelMark = record?.["出钢记号"] ?? `记录${idx + 1}`;

        setOptimizeProgress(`${i + 1}/${selectedIndices.length}`);
        setCurrentOptimizingIdx(idx);
        setTabTransition(prev => ({ ...prev, [idx]: "loading" }));
        setOptimizeLogs(prev => [...prev, `\n=== 正在优化第 ${i + 1}/${selectedIndices.length} 条: 出钢记号 ${steelMark} ===`]);

        const composition = recordToComposition(record);
        const controller = new AbortController();
        optimizeAbortControllerRef.current = controller;

        let newResult: any;
        let optimizeSucceeded = false;
        try {
          const resp = await runOptimize({
            composition,
            targets,
            process_constraints: {},
            maxiter: parseInt(optimizeMaxiter) || (optimizeAlgorithm === "nsga2" ? 200 : 500),
            popsize: parseInt(optimizePopsize) || (optimizeAlgorithm === "nsga2" ? 100 : 30),
            algorithm: optimizeAlgorithm,
            batch_id: optimizeBatchId,
            record_index: idx,
            steel_mark: String(steelMark),
            standard_record: record ?? {},
            standard_columns: activeOptimizerStandardColumns,
            candidate_standard_records: activeOptimizerStandardRecords,
          }, controller.signal);
          if (resp.status === 409) {
            const errData = await resp.json();
            const conflictMsg = errData.detail || "优化请求失败";
            setOptimizeError(conflictMsg);
            setOptimizeLogs(prev => [...prev, `\n✗ ${conflictMsg}`]);
            break;
          }
          if (resp.status === 499) {
            setOptimizeLogs(prev => [...prev, "\n用户已停止优化"]);
            optimizeAbortControllerRef.current = null;
            break;
          }
          if (!resp.ok) {
            let errorMessage = `优化请求失败 (${resp.status})`;
            try {
              const errData = await resp.json();
              errorMessage = errData.detail || errData.error || errorMessage;
            } catch {
              try {
                const errorText = await resp.text();
                if (errorText.trim()) errorMessage = errorText.trim();
              } catch {
                // Ignore secondary parsing failures and keep the HTTP status message.
              }
            }
            newResult = { record, error: errorMessage, idx, steelMark };
            setOptimizeError(errorMessage);
            setOptimizeLogs(prev => [...prev, `\n✗ ${errorMessage}`]);
            optimizeAbortControllerRef.current = null;
            break;
          }
          const respData = await resp.json();
          newResult = { record, result: respData, idx, steelMark };
          optimizeSucceeded = !respData.error;
        } catch (e: any) {
          if (e.name === "AbortError") {
            setOptimizeLogs(prev => [...prev, "用户已停止优化"]);
            optimizeAbortControllerRef.current = null;
            break;
          }
          newResult = { record, error: e.message, idx, steelMark };
        }
        if (optimizeSucceeded) {
          for (let retry = 0; retry < 3; retry++) {
            await new Promise(r => window.setTimeout(r, 500));
            try {
              const logResp = await getOptimizeLogs();
              const logData = await logResp.json();
              if (logData.logs) {
                setOptimizeLogs(logData.logs);
                optimizeLogsRef.current = logData.logs;
              }
            } catch {
              // Ignore transient log fetch errors after a completed optimize call.
            }
          }
        }

        const recordLogs = [...optimizeLogsRef.current];
        setPerRecordLogs(prev => ({ ...prev, [idx]: recordLogs }));

        results.push(newResult);
        setOptimizeResults(prev => [...prev, newResult]);

        setTabTransition(prev => ({ ...prev, [idx]: "fade-out" }));
        await new Promise(resolve => window.setTimeout(resolve, 300));
        setTabTransition(prev => ({ ...prev, [idx]: "fade-in" }));
        await new Promise(resolve => window.setTimeout(resolve, 500));
        setTabTransition(prev => ({ ...prev, [idx]: "done" }));

        setActiveResultTab(i);
      }

      if (results.length === 1 && results[0].result?.success) {
        setOptimizeResult(results[0].result);
      }

      if (results.length > 0) {
        setActiveResultTab(results.length - 1);
      }
      await new Promise(r => window.setTimeout(r, 50));
    } finally {
      optimizeAbortControllerRef.current = null;
      setCurrentOptimizingIdx(null);
      setOptimizeProgress("");
      setOptimizing(false);
      setOptimizeStopping(false);
    }
  }, [
    activeOptimizerStandardColumns,
    activeOptimizerStandardRecords,
    optimizeAlgorithm,
    optimizeMaxiter,
    optimizePopsize,
    selectedStandardIdx,
    targetElong,
    targetTensile,
    targetYield,
    waitForOptimizeIdle,
  ]);

  return {
    showOptimizer,
    setShowOptimizer,
    optimizerComposition,
    selectedStandardIdx,
    targetYield,
    setTargetYield,
    targetTensile,
    setTargetTensile,
    targetElong,
    setTargetElong,
    optimizing,
    optimizeResult,
    optimizeResults,
    activeResultTab,
    setActiveResultTab,
    optimizeProgress,
    optimizeMaxiter,
    setOptimizeMaxiter,
    optimizePopsize,
    setOptimizePopsize,
    optimizeAlgorithm,
    setOptimizeAlgorithm,
    optimizeError,
    optimizeLogs,
    optimizeLogRef,
    optimizeStopping,
    optimizerRestoring,
    currentOptimizingIdx,
    perRecordLogs,
    paretoIdx,
    setParetoIdx,
    tabViewMode,
    setTabViewMode,
    tabTransition,
    activeOptimizerStandardRecords,
    activeOptimizerStandardColumns,
    hasOptimizerData,
    handleOpenOptimizer,
    handleSelectStandard,
    handleToggleAllStandards,
    handleOptimize,
    requestOptimizeCancel,
    prepareAgentRetrievalOptimizationFlow,
  };
}
