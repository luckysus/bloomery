/**
 * skillStore —— 第 77 章要求的 Zustand store 之一。
 *
 * 承载 Skill 目录（含加载错误）。迁移前这些领域数据由设置页 Skill 面板与
 * 能力管理页各自持有；现在统一由本 store 承载。
 */
import { create } from "zustand";
import type { SkillCatalog } from "../bridge/desktop";
import { resolveUpdater, type Updater } from "./storeUtils";

const emptyCatalog: SkillCatalog = { skills: [], errors: [] };

export interface SkillStoreState {
  catalog: SkillCatalog;
  setCatalog: (next: Updater<SkillCatalog>) => void;
  reset: () => void;
}

export const useSkillStore = create<SkillStoreState>((set) => ({
  catalog: emptyCatalog,

  setCatalog: (next) => set((state) => ({ catalog: resolveUpdater(next, state.catalog) })),

  reset: () => set({ catalog: emptyCatalog }),
}));

/** 仅供测试：把 Skill 状态复位到初始值。 */
export function resetSkillStoreForTests(): void {
  useSkillStore.getState().reset();
}
