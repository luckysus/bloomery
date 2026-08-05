import {
  Activity,
  BarChart3,
  BookOpen,
  LayoutDashboard,
  MessageSquareText,
  Puzzle,
  Settings,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

export type SectionId =
  | "workbench"
  | "chat"
  | "knowledge"
  | "analysis"
  | "extensions"
  | "settings"
  | "diagnostics";

export interface NavigationSection {
  id: SectionId;
  label: string;
  description: string;
  icon: LucideIcon;
}

export const navigationSections: readonly NavigationSection[] = [
  { id: "workbench", label: "工作台", description: "最近工作与系统状态", icon: LayoutDashboard },
  { id: "chat", label: "对话", description: "智能体会话与运行记录", icon: MessageSquareText },
  { id: "knowledge", label: "知识库", description: "文档、索引与证据", icon: BookOpen },
  { id: "analysis", label: "数据分析", description: "数据集、预测与优化", icon: BarChart3 },
  { id: "extensions", label: "扩展", description: "MCP、Skills 与领域包", icon: Puzzle },
  { id: "settings", label: "设置", description: "Provider、数据目录与权限", icon: Settings },
  { id: "diagnostics", label: "诊断", description: "存储、任务与运行健康", icon: Activity },
];

export function getNavigationSection(id: SectionId) {
  return navigationSections.find((section) => section.id === id) ?? navigationSections[0];
}
