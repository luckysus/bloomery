/**
 * taskStore —— 第 77 章要求的 Zustand store 之一。
 *
 * 承载持久化后台任务列表（运行记录页与工作台共用）。迁移前该列表由
 * `features/diagnostics/DiagnosticsPage.tsx` 的快照对象持有；现在由本 store
 * 承载，页面其余诊断数据仍留在页面内。
 */
import { create } from "zustand";
import type { BackgroundTask } from "../bridge/desktop";
import { resolveUpdater, type Updater } from "./storeUtils";

export interface TaskStoreState {
  tasks: BackgroundTask[];
  setTasks: (next: Updater<BackgroundTask[]>) => void;
  reset: () => void;
}

export const useTaskStore = create<TaskStoreState>((set) => ({
  tasks: [],

  setTasks: (next) => set((state) => ({ tasks: resolveUpdater(next, state.tasks) })),

  reset: () => set({ tasks: [] }),
}));

/** 仅供测试：把任务状态复位到初始值。 */
export function resetTaskStoreForTests(): void {
  useTaskStore.getState().reset();
}
