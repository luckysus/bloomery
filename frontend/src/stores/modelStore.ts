/**
 * modelStore —— 第 77 章要求的 Zustand store 之一。
 *
 * 承载对话模型（Provider Profile）列表与当前选中的模型。迁移前这些状态由
 * `features/chat/chatController.ts` 的 `useState` 持有。
 */
import { create } from "zustand";
import type { ProviderProfileResponse } from "../bridge/desktop";
import { resolveUpdater, type Updater } from "./storeUtils";

export interface ModelStoreState {
  chatProfiles: ProviderProfileResponse[];
  activeChatProfileId: string | null;

  setChatProfiles: (next: Updater<ProviderProfileResponse[]>) => void;
  setActiveChatProfileId: (next: Updater<string | null>) => void;
  reset: () => void;
}

function createInitialState() {
  return {
    chatProfiles: [] as ProviderProfileResponse[],
    activeChatProfileId: null as string | null,
  };
}

export const useModelStore = create<ModelStoreState>((set) => ({
  ...createInitialState(),

  setChatProfiles: (next) => set((state) => ({ chatProfiles: resolveUpdater(next, state.chatProfiles) })),
  setActiveChatProfileId: (next) => set((state) => ({ activeChatProfileId: resolveUpdater(next, state.activeChatProfileId) })),

  reset: () => set(createInitialState()),
}));

/** 仅供测试：把模型状态复位到初始值。 */
export function resetModelStoreForTests(): void {
  useModelStore.getState().reset();
}
