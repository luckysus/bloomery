/**
 * agentStore —— 第 77 章要求的 Zustand store 之一。
 *
 * 承载 Agent 运行视图、子任务恢复记录与 Agent 选择状态。迁移前这些状态由
 * `features/chat/chatController.ts` 的 `useState` 持有。
 *
 * 注意：`agentViews` 这类纯缓存（不参与渲染）仍留在控制器里，不作为 store 状态。
 */
import { create } from "zustand";
import type { AgentProfileSummary, RecoveredRun } from "../bridge/desktop";
import type { AgentRunView } from "../features/chat/agentEvents";
import { resolveUpdater, type Updater } from "./storeUtils";

export interface AgentStoreState {
  agentRun: AgentRunView | null;
  recoveredRuns: RecoveredRun[];
  activeRunId: string | null;
  agentProfiles: AgentProfileSummary[];
  activeAgentId: string | null;

  setAgentRun: (next: Updater<AgentRunView | null>) => void;
  setRecoveredRuns: (next: Updater<RecoveredRun[]>) => void;
  setActiveRunId: (next: Updater<string | null>) => void;
  setAgentProfiles: (next: Updater<AgentProfileSummary[]>) => void;
  setActiveAgentId: (next: Updater<string | null>) => void;
  reset: () => void;
}

function createInitialState() {
  return {
    agentRun: null as AgentRunView | null,
    recoveredRuns: [] as RecoveredRun[],
    activeRunId: null as string | null,
    agentProfiles: [] as AgentProfileSummary[],
    activeAgentId: null as string | null,
  };
}

export const useAgentStore = create<AgentStoreState>((set) => ({
  ...createInitialState(),

  setAgentRun: (next) => set((state) => ({ agentRun: resolveUpdater(next, state.agentRun) })),
  setRecoveredRuns: (next) => set((state) => ({ recoveredRuns: resolveUpdater(next, state.recoveredRuns) })),
  setActiveRunId: (next) => set((state) => ({ activeRunId: resolveUpdater(next, state.activeRunId) })),
  setAgentProfiles: (next) => set((state) => ({ agentProfiles: resolveUpdater(next, state.agentProfiles) })),
  setActiveAgentId: (next) => set((state) => ({ activeAgentId: resolveUpdater(next, state.activeAgentId) })),

  reset: () => set(createInitialState()),
}));

/** 仅供测试：把 Agent 状态复位到初始值。 */
export function resetAgentStoreForTests(): void {
  useAgentStore.getState().reset();
}
