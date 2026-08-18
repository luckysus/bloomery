import React, { useState, useEffect, useRef, useCallback } from "react";
import { initialAgentProgress, type AgentProgressState } from "../agent/AgentProgressBar";
import type { AgentConversation, AgentMessage, AgentRecommendation, AgentResponse } from "../agent/types";
import type { LabServiceStatusInfo } from "../services/labService";
import { useAuth } from "../context/AuthContext";
import type {
  AppMode,
  SearchResponse,
  TabId,
} from "../types/rag";
import { useLabServiceStatus } from "./useLabServiceStatus";
import { useAgentConversations } from "./useAgentConversations";
import { useAgentRuntime } from "./useAgentRuntime";
import { useAdvancedSteelFilters } from "./useAdvancedSteelFilters";
import { useLiteratureUpload } from "./useLiteratureUpload";
import { useOptimizationState } from "./useOptimizationState";
import { useProfileSettings } from "./useProfileSettings";
import { useRagEntryGateProps } from "./useRagEntryGateProps";
import { useResultWorkspace } from "./useResultWorkspace";
import { useSearchMode } from "./useSearchMode";
import { useTrainingJobs } from "./useTrainingJobs";

export function useRagAppController() {
  /* -------- state -------- */
  const {
    isAuthenticated,
    authChecked,
    authUser,
    markLoggedIn,
    logoutAuth,
  } = useAuth();
  const authScopeKey = isAuthenticated ? authUser?.username ?? "" : "";
  const [appMode, setAppMode] = useState<AppMode>("agent");
  const [query, setQuery] = useState("");
  // 高级筛选参数
  const [slabWidthMin, setSlabWidthMin] = useState(0);
  const [slabWidthMax, setSlabWidthMax] = useState(99999);
  const [slabThicknessMin, setSlabThicknessMin] = useState(0);
  const [slabThicknessMax, setSlabThicknessMax] = useState(99999);
  const [yieldRp02Min, setYieldRp02Min] = useState(0);
  const [yieldRp02Max, setYieldRp02Max] = useState(99999);
  const [tensileStrengthMin, setTensileStrengthMin] = useState(0);
  const [tensileStrengthMax, setTensileStrengthMax] = useState(99999);
  const [elongationMin, setElongationMin] = useState(0);
  const [elongationMax, setElongationMax] = useState(99999);
  // 单值性能输入（建议模式使用）
  const [yieldRp02Value, setYieldRp02Value] = useState<number | "">("");
  const [tensileStrengthValue, setTensileStrengthValue] = useState<number | "">("");
  const [elongationValue, setElongationValue] = useState<number | "">("");
  const [topK, setTopK] = useState(10);
  const [includeProduction, setIncludeProduction] = useState(false);
  const [loading, setLoading] = useState(false);
  const [data, setData] = useState<SearchResponse | null>(null);
  const advancedFilters = useAdvancedSteelFilters({
    data,
    onApply: () => {
      pendingFilterSearchRef.current = true;
    },
  });
  const { steelMark, steelGrade } = advancedFilters;
  const [activeTab, setActiveTab] = useState<TabId>("literature");
  const resultPaneRef = useRef<HTMLDivElement>(null);
  const [showFilters, setShowFilters] = useState(false);
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null);
  // 侧边栏始终展开，移除收起功能
  const [exportingScheme, setExportingScheme] = useState(false);
  const [isAgentMode, setIsAgentMode] = useState(true);
  const agentMessagesRef = useRef<HTMLDivElement>(null);
  const agentMessagesEndRef = useRef<HTMLDivElement>(null);
  const agentScrollFrameRef = useRef<number | null>(null);
  const agentWasStreamingRef = useRef(false);
  const [agentScrollToBottomKey, setAgentScrollToBottomKey] = useState(0);
  const [agentLoading, setAgentLoading] = useState(false);
  const [agentStreaming, setAgentStreaming] = useState(false);
  const [agentError, setAgentError] = useState("");
  const [agentProgress, setAgentProgress] = useState<AgentProgressState>(initialAgentProgress);
  useEffect(() => {
    setData(null);
    setQuery("");
    setIncludeProduction(false);
    setActiveTab("literature");
    setShowFilters(false);
    setLightboxSrc(null);
    setYieldRp02Value("");
    setTensileStrengthValue("");
    setElongationValue("");
    setAgentError("");
    setAgentProgress(initialAgentProgress);
  }, [authScopeKey]);
  const {
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
    abortControllerRef,
    pendingFilterSearchRef,
    adviceMode,
    adviceModeEnabled,
    stopAIStreaming,
    handleAIModeToggle,
    handleCompositionModeToggle,
    handleCoilMatchModeToggle,
    handleSearch,
  } = useSearchMode({
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
  });
  const resultWorkspace = useResultWorkspace({
    authScopeKey,
    data,
    query,
    includeProduction,
    adviceMode,
    adviceModeEnabled,
    resultView,
    isAIMode,
    activeTab,
    setActiveTab,
    resultPaneRef,
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
    steelMark,
    steelGrade,
    setSlabWidthMin,
    setSlabWidthMax,
    setSlabThicknessMin,
    setSlabThicknessMax,
    setYieldRp02Min,
    setYieldRp02Max,
    setTensileStrengthMin,
    setTensileStrengthMax,
    setElongationMin,
    setElongationMax,
  });

  const {
    agentSessionId,
    setAgentSessionId,
    agentMessages,
    setAgentMessages,
    agentResponse,
    setAgentResponse,
    agentConversations,
    setAgentConversations,
    agentHistorySearchOpen,
    setAgentHistorySearchOpen,
    agentHistorySearch,
    setAgentHistorySearch,
    agentRecentChatsOpen,
    setAgentRecentChatsOpen,
    agentModelMenuOpen,
    setAgentModelMenuOpen,
    agentConversationSyncError,
    setAgentConversationSyncError,
    agentConversationMenuId,
    setAgentConversationMenuId,
    renamingConversationId,
    setRenamingConversationId,
    renamingConversationTitle,
    setRenamingConversationTitle,
    copiedAgentMessageIndex,
    setCopiedAgentMessageIndex,
    editingAgentMessageIndex,
    editingAgentMessageText,
    setEditingAgentMessageText,
    agentSidebarCollapsed,
    setAgentSidebarCollapsed,
    filteredAgentConversations,
    recentAgentConversations,
    persistAgentConversation,
    startNewAgentConversation,
    selectAgentConversation,
    toggleAgentConversationPin,
    deleteAgentConversation,
    removeConversationBySessionId,
    beginRenameAgentConversation,
    commitRenameAgentConversation,
    shareAgentConversation,
    beginEditAgentMessage,
    cancelEditAgentMessage,
  } = useAgentConversations({
    authChecked,
    isAuthenticated,
    authUserKey: authUser?.username ?? "",
    agentLoading,
    setAgentProgress,
    setAgentError,
    setQuery,
    setAgentScrollToBottomKey,
  });
  const profileSettings = useProfileSettings({
    authChecked,
    isAuthenticated,
    authUser,
    onBeforeOpenProfile: () => {
      setAgentRecentChatsOpen(false);
      setAgentHistorySearchOpen(false);
    },
  });
  const {
    activeLlmConfig,
    currentChatProvider,
    currentChatBaseUrl,
    currentChatModelName,
    profileInfo,
    setProfileError,
    setProfileInfo,
  } = profileSettings;
  const labService = useLabServiceStatus();
  const {
    labServiceStatus,
    labServiceLoading,
    labServiceCheckedRef,
    refreshLabServiceStatus,
  } = labService;

  // 模型训练相关状态
  const [showTraining, setShowTraining] = useState(false);
  const [trainingEntrySource, setTrainingEntrySource] = useState<'main' | 'optimizer'>('main');

  // 文献处理相关状态
  const literatureUpload = useLiteratureUpload(authScopeKey);
  const trainingJobs = useTrainingJobs(showTraining);
  const optimizationState = useOptimizationState({
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
  });
  const {
    optimizerComposition,
    optimizeMaxiter,
    optimizePopsize,
    optimizeAlgorithm,
    prepareAgentRetrievalOptimizationFlow,
  } = optimizationState;

  const handleLogout = useCallback(async () => {
    abortControllerRef.current?.abort();
    abortControllerRef.current = null;
    await logoutAuth();
    setProfileInfo(null);
    setAppMode("select");
    setAgentSessionId("");
    setAgentMessages([]);
    setAgentResponse(null);
    setAgentConversations([]);
    setAgentLoading(false);
    setAgentStreaming(false);
    setAgentError("");
    setQuery("");
    setAgentProgress(initialAgentProgress);
  }, [
    abortControllerRef,
    logoutAuth,
    setAgentConversations,
    setAgentError,
    setAgentLoading,
    setAgentMessages,
    setAgentProgress,
    setAgentResponse,
    setAgentSessionId,
    setAgentStreaming,
    setAppMode,
    setProfileInfo,
    setQuery,
  ]);

  const agentRuntime = useAgentRuntime({
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
  });

  useEffect(() => {
    if (isAgentMode && !labServiceCheckedRef.current && !labServiceLoading) {
      void refreshLabServiceStatus({ quiet: true });
    }
  }, [isAgentMode, labServiceLoading, labServiceStatus, refreshLabServiceStatus]);

  /* -------- auto-scroll agent chat during streaming -------- */
  const scrollAgentMessagesToBottom = useCallback((behavior: ScrollBehavior = "auto") => {
    if (agentScrollFrameRef.current !== null) {
      window.cancelAnimationFrame(agentScrollFrameRef.current);
    }
    agentScrollFrameRef.current = window.requestAnimationFrame(() => {
      const container = agentMessagesRef.current;
      if (container) {
        container.scrollTo({ top: container.scrollHeight, behavior });
      } else {
        agentMessagesEndRef.current?.scrollIntoView({ block: "end", behavior });
      }
      agentScrollFrameRef.current = null;
    });
  }, []);

  useEffect(() => {
    if (!isAgentMode || agentScrollToBottomKey === 0) return;
    scrollAgentMessagesToBottom("auto");
  }, [agentScrollToBottomKey, isAgentMode, scrollAgentMessagesToBottom]);

  useEffect(() => {
    if (!isAgentMode) return;
    if (!agentLoading && !agentProgress.active) return;
    scrollAgentMessagesToBottom("auto");
  }, [agentMessages, agentLoading, agentProgress.active, agentProgress.statusText, isAgentMode, scrollAgentMessagesToBottom]);

  useEffect(() => {
    if (!isAgentMode) {
      agentWasStreamingRef.current = false;
      return;
    }
    if (agentStreaming) {
      agentWasStreamingRef.current = true;
      return;
    }
    if (!agentWasStreamingRef.current) return;
    agentWasStreamingRef.current = false;

    scrollAgentMessagesToBottom("auto");
    const finalScrollTimer = window.setTimeout(() => {
      scrollAgentMessagesToBottom("auto");
    }, 80);
    return () => window.clearTimeout(finalScrollTimer);
  }, [agentStreaming, isAgentMode, scrollAgentMessagesToBottom]);

  useEffect(() => {
    return () => {
      if (agentScrollFrameRef.current !== null) {
        window.cancelAnimationFrame(agentScrollFrameRef.current);
      }
    };
  }, []);

  const { shouldShowEntryGate, entryGateProps } = useRagEntryGateProps({
    authChecked,
    isAuthenticated,
    appMode,
    markLoggedIn,
    handleLogout,
    setProfileInfo,
    setAppMode,
    setIsAgentMode,
    setQuery,
    setIsAIMode,
    setIsCompositionMode,
    setIsCoilMatchMode,
    setData,
    setCoilMatchResults,
    setCoilMatchError,
  });

  const shellProps = {
    lightboxSrc,
    setLightboxSrc,
    ...advancedFilters,
    isAgentMode,
    ...profileSettings,
    agentSidebarCollapsed,
    agentRecentChatsOpen,
    agentConversationSyncError,
    agentConversations,
    filteredAgentConversations,
    agentSessionId,
    renamingConversationId,
    renamingConversationTitle,
    agentConversationMenuId,
    setRenamingConversationTitle,
    setAgentSidebarCollapsed,
    startNewAgentConversation,
    setAgentRecentChatsOpen,
    setAgentHistorySearchOpen,
    selectAgentConversation,
    toggleAgentConversationPin,
    beginRenameAgentConversation,
    shareAgentConversation,
    deleteAgentConversation,
    setAgentConversationMenuId,
    commitRenameAgentConversation,
    agentModelMenuOpen,
    setAgentModelMenuOpen,
    isStreaming,
    agentMessagesRef,
    agentMessagesEndRef,
    agentMessages,
    agentResponse,
    agentLoading,
    agentStreaming,
    agentError,
    agentProgress,
    query,
    setQuery,
    ...agentRuntime,
    beginEditAgentMessage,
    cancelEditAgentMessage,
    editingAgentMessageIndex,
    editingAgentMessageText,
    setEditingAgentMessageText,
    copiedAgentMessageIndex,
    setAppMode,
    onSwitchRetrieval: entryGateProps.onSelectRetrieval,
    onSwitchAgent: entryGateProps.onSelectAgent,
    handleLogout,
    ...resultWorkspace,
    isAIMode,
    data,
    loading,
    aiAnswer,
    isProductionAI,
    stopAIStreaming,
    handleAIModeToggle,
    handleCompositionModeToggle,
    handleCoilMatchModeToggle,
    isCompositionMode,
    isCoilMatchMode,
    coilMatchLoading,
    coilMatchError,
    coilMatchResults,
    resultView,
    setResultView,
    includeProduction,
    setIncludeProduction,
    topK,
    setTopK,
    showFilters,
    setShowFilters,
    ...labService,
    activeTab,
    resultPaneRef,
    setActiveTab,
    ...optimizationState,
    exportingScheme,
    setExportingScheme,
    showTraining,
    setShowTraining,
    trainingEntrySource,
    setTrainingEntrySource,
    ...trainingJobs,
    ...literatureUpload,
    agentHistorySearchOpen,
    agentHistorySearch,
    setAgentHistorySearch,
    recentAgentConversations,
    adviceModeEnabled,
    yieldRp02Value,
    setYieldRp02Value,
    tensileStrengthValue,
    setTensileStrengthValue,
    elongationValue,
    setElongationValue,
    slabWidthMin,
    setSlabWidthMin,
    slabWidthMax,
    setSlabWidthMax,
    slabThicknessMin,
    setSlabThicknessMin,
    slabThicknessMax,
    setSlabThicknessMax,
    yieldRp02Min,
    setYieldRp02Min,
    yieldRp02Max,
    setYieldRp02Max,
    tensileStrengthMin,
    setTensileStrengthMin,
    tensileStrengthMax,
    setTensileStrengthMax,
    elongationMin,
    setElongationMin,
    elongationMax,
    setElongationMax,
    handleSearch,
    steelMark,
    steelGrade,
    aiAnswerRef,
    setProfileError,
    setRenamingConversationId,
  };

  return {
    shouldShowEntryGate,
    entryGateProps,
    shellProps,
  };
}
