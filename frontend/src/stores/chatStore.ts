/**
 * chatStore —— 第 77 章要求的 Zustand store 之一。
 *
 * 承载会话、消息、草稿与输入区状态。迁移前这些状态由
 * `features/chat/chatController.ts` 的 `useState` 持有；现在由本 store 承载，
 * setter 保持 React `useState` 的更新器语义（值或函数），因此控制器与消费方
 * 的调用方式不变。
 */
import { create } from "zustand";
import type {
  Conversation,
  EvidenceItem,
  LocalAgentAttachment,
  Message,
} from "../bridge/desktop";
import { resolveUpdater, type Updater } from "./storeUtils";

export interface ChatStoreState {
  conversations: Conversation[];
  selectedId: string | null;
  messages: Message[];
  draft: string;
  pendingQuestion: string | null;
  streamingCitations: EvidenceItem[];
  attachments: LocalAgentAttachment[];
  loading: boolean;
  loadingMessages: boolean;
  error: string | null;
  notice: string | null;
  smartSearchEnabled: boolean;
  autoSearchEnabled: boolean;

  setConversations: (next: Updater<Conversation[]>) => void;
  setSelectedId: (next: Updater<string | null>) => void;
  setMessages: (next: Updater<Message[]>) => void;
  setDraft: (next: Updater<string>) => void;
  setPendingQuestion: (next: Updater<string | null>) => void;
  setStreamingCitations: (next: Updater<EvidenceItem[]>) => void;
  setAttachments: (next: Updater<LocalAgentAttachment[]>) => void;
  setLoading: (next: Updater<boolean>) => void;
  setLoadingMessages: (next: Updater<boolean>) => void;
  setError: (next: Updater<string | null>) => void;
  setNotice: (next: Updater<string | null>) => void;
  setSmartSearchEnabled: (next: Updater<boolean>) => void;
  setAutoSearchEnabled: (next: Updater<boolean>) => void;
  reset: () => void;
}

function createInitialState() {
  return {
    conversations: [] as Conversation[],
    selectedId: null as string | null,
    messages: [] as Message[],
    draft: "",
    pendingQuestion: null as string | null,
    streamingCitations: [] as EvidenceItem[],
    attachments: [] as LocalAgentAttachment[],
    loading: true,
    loadingMessages: false,
    error: null as string | null,
    notice: null as string | null,
    smartSearchEnabled: false,
    autoSearchEnabled: false,
  };
}

export const useChatStore = create<ChatStoreState>((set) => ({
  ...createInitialState(),

  setConversations: (next) => set((state) => ({ conversations: resolveUpdater(next, state.conversations) })),
  setSelectedId: (next) => set((state) => ({ selectedId: resolveUpdater(next, state.selectedId) })),
  setMessages: (next) => set((state) => ({ messages: resolveUpdater(next, state.messages) })),
  setDraft: (next) => set((state) => ({ draft: resolveUpdater(next, state.draft) })),
  setPendingQuestion: (next) => set((state) => ({ pendingQuestion: resolveUpdater(next, state.pendingQuestion) })),
  setStreamingCitations: (next) => set((state) => ({ streamingCitations: resolveUpdater(next, state.streamingCitations) })),
  setAttachments: (next) => set((state) => ({ attachments: resolveUpdater(next, state.attachments) })),
  setLoading: (next) => set((state) => ({ loading: resolveUpdater(next, state.loading) })),
  setLoadingMessages: (next) => set((state) => ({ loadingMessages: resolveUpdater(next, state.loadingMessages) })),
  setError: (next) => set((state) => ({ error: resolveUpdater(next, state.error) })),
  setNotice: (next) => set((state) => ({ notice: resolveUpdater(next, state.notice) })),
  setSmartSearchEnabled: (next) => set((state) => ({ smartSearchEnabled: resolveUpdater(next, state.smartSearchEnabled) })),
  setAutoSearchEnabled: (next) => set((state) => ({ autoSearchEnabled: resolveUpdater(next, state.autoSearchEnabled) })),

  reset: () => set(createInitialState()),
}));

/** 仅供测试：把会话状态复位到初始值。 */
export function resetChatStoreForTests(): void {
  useChatStore.getState().reset();
}
