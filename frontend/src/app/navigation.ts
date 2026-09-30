import { BookOpen, BrainCircuit, ChartNoAxesCombined, FileChartColumn, FlaskConical, Gauge, Library, MessageSquareText, Network, Settings2, Sparkles, Table2, Wrench, Cpu, CircleHelp, UserRound } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export type SectionId = "chat" | "knowledge" | "literature" | "data" | "prediction" | "optimization" | "experiment" | "agents" | "tools" | "mcp" | "skills" | "models" | "reports" | "settings" | "account" | "about" | "diagnostics";
export interface NavigationSection { id: SectionId; label: string; description: string; icon: LucideIcon; group: "research" | "manage"; }
export const primaryNavigationSections: readonly NavigationSection[] = [
  { id: "chat", label: "智能问答", description: "与 Suna 协作完成材料研发任务", icon: MessageSquareText, group: "research" },
  { id: "knowledge", label: "知识中心", description: "管理知识库、文档和 Wiki", icon: Library, group: "research" },
  { id: "literature", label: "文献研究", description: "检索、阅读和总结科研文献", icon: BookOpen, group: "research" },
  { id: "data", label: "数据实验室", description: "处理数据并生成科研图表", icon: Table2, group: "research" },
  { id: "prediction", label: "性能预测", description: "预测材料性能并解释模型", icon: ChartNoAxesCombined, group: "research" },
  { id: "optimization", label: "工艺优化", description: "探索多目标工艺方案", icon: Gauge, group: "research" },
  { id: "experiment", label: "实验助手", description: "设计下一组高价值实验", icon: FlaskConical, group: "research" },
];
export const utilityNavigationSections: readonly NavigationSection[] = [
  { id: "agents", label: "Agent 管理", description: "配置主 Agent 和专业 Agent", icon: BrainCircuit, group: "manage" },
  { id: "tools", label: "工具中心", description: "管理文件、计算和分析工具", icon: Wrench, group: "manage" },
  { id: "mcp", label: "MCP 管理", description: "连接外部工具和数据服务", icon: Network, group: "manage" },
  { id: "skills", label: "Skill 管理", description: "管理可复用的专业能力", icon: Sparkles, group: "manage" },
  { id: "models", label: "模型管理", description: "配置对话、Embedding 和重排模型", icon: Cpu, group: "manage" },
  { id: "reports", label: "科研报告", description: "生成报告、综述和实验记录", icon: FileChartColumn, group: "manage" },
  { id: "settings", label: "设置", description: "配置模型、外观和本地数据", icon: Settings2, group: "manage" },
  { id: "account", label: "用户中心", description: "管理本地账户和个人偏好", icon: UserRound, group: "manage" },
  { id: "about", label: "关于", description: "Suna 版本和项目信息", icon: CircleHelp, group: "manage" },
  { id: "diagnostics", label: "运行记录", description: "查看 Agent 任务和系统状态", icon: Settings2, group: "manage" },
];
export const navigationSections = [...primaryNavigationSections, ...utilityNavigationSections] as const;
export function getNavigationSection(id: SectionId) { return navigationSections.find((section) => section.id === id) ?? navigationSections[0]; }
