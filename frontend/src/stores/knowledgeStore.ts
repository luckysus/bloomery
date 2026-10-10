/**
 * knowledgeStore —— 第 77 章要求的 Zustand store 之一。
 *
 * 承载知识库列表、本轮选中的知识库与检索过滤器。迁移前这些状态由
 * `features/chat/chatController.ts` 的 `useState` 持有。
 */
import { create } from "zustand";
import type { KnowledgeBaseRecord, PostgresKnowledgeSearchFilters } from "../bridge/desktop";
import { resolveUpdater, type Updater } from "./storeUtils";

export interface KnowledgeStoreState {
  knowledgeBases: KnowledgeBaseRecord[];
  selectedKnowledgeBaseIds: string[];
  knowledgeFilters: PostgresKnowledgeSearchFilters;

  setKnowledgeBases: (next: Updater<KnowledgeBaseRecord[]>) => void;
  setSelectedKnowledgeBaseIds: (next: Updater<string[]>) => void;
  setKnowledgeFilters: (next: Updater<PostgresKnowledgeSearchFilters>) => void;
  reset: () => void;
}

function createInitialState() {
  return {
    knowledgeBases: [] as KnowledgeBaseRecord[],
    selectedKnowledgeBaseIds: [] as string[],
    knowledgeFilters: {} as PostgresKnowledgeSearchFilters,
  };
}

export const useKnowledgeStore = create<KnowledgeStoreState>((set) => ({
  ...createInitialState(),

  setKnowledgeBases: (next) => set((state) => ({ knowledgeBases: resolveUpdater(next, state.knowledgeBases) })),
  setSelectedKnowledgeBaseIds: (next) => set((state) => ({ selectedKnowledgeBaseIds: resolveUpdater(next, state.selectedKnowledgeBaseIds) })),
  setKnowledgeFilters: (next) => set((state) => ({ knowledgeFilters: resolveUpdater(next, state.knowledgeFilters) })),

  reset: () => set(createInitialState()),
}));

/** 仅供测试：把知识库状态复位到初始值。 */
export function resetKnowledgeStoreForTests(): void {
  useKnowledgeStore.getState().reset();
}
