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
import type { MessageKey } from "../i18n/locale";

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
  labelKey: MessageKey;
  descriptionKey: MessageKey;
  icon: LucideIcon;
}

export const navigationSections: readonly NavigationSection[] = [
  { id: "workbench", labelKey: "navWorkbench", descriptionKey: "navWorkbenchDescription", icon: LayoutDashboard },
  { id: "chat", labelKey: "navChat", descriptionKey: "navChatDescription", icon: MessageSquareText },
  { id: "knowledge", labelKey: "navKnowledge", descriptionKey: "navKnowledgeDescription", icon: BookOpen },
  { id: "analysis", labelKey: "navAnalysis", descriptionKey: "navAnalysisDescription", icon: BarChart3 },
  { id: "extensions", labelKey: "navExtensions", descriptionKey: "navExtensionsDescription", icon: Puzzle },
  { id: "settings", labelKey: "navSettings", descriptionKey: "navSettingsDescription", icon: Settings },
  { id: "diagnostics", labelKey: "navDiagnostics", descriptionKey: "navDiagnosticsDescription", icon: Activity },
];

export function getNavigationSection(id: SectionId) {
  return navigationSections.find((section) => section.id === id) ?? navigationSections[0];
}
