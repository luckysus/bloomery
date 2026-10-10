/**
 * mcpStore —— 第 77 章要求的 Zustand store 之一。
 *
 * 承载 MCP Server 列表、健康检查结果与工具目录。迁移前这些领域数据由
 * `features/mcp/McpManagementPage.tsx` 的组件内状态持有；表单草稿等瞬态状态
 * 仍留在组件内。
 */
import { create } from "zustand";
import type { McpHealth, McpServerSummary, McpToolSummary } from "../bridge/desktop";
import { resolveUpdater, type Updater } from "./storeUtils";

export interface McpStoreState {
  servers: McpServerSummary[];
  health: Record<string, McpHealth>;
  tools: Record<string, McpToolSummary[]>;

  setServers: (next: Updater<McpServerSummary[]>) => void;
  setHealth: (next: Updater<Record<string, McpHealth>>) => void;
  setTools: (next: Updater<Record<string, McpToolSummary[]>>) => void;
  reset: () => void;
}

function createInitialState() {
  return {
    servers: [] as McpServerSummary[],
    health: {} as Record<string, McpHealth>,
    tools: {} as Record<string, McpToolSummary[]>,
  };
}

export const useMcpStore = create<McpStoreState>((set) => ({
  ...createInitialState(),

  setServers: (next) => set((state) => ({ servers: resolveUpdater(next, state.servers) })),
  setHealth: (next) => set((state) => ({ health: resolveUpdater(next, state.health) })),
  setTools: (next) => set((state) => ({ tools: resolveUpdater(next, state.tools) })),

  reset: () => set(createInitialState()),
}));

/** 仅供测试：把 MCP 状态复位到初始值。 */
export function resetMcpStoreForTests(): void {
  useMcpStore.getState().reset();
}
