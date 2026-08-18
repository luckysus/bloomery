import {
  Activity,
  BarChart3,
  BookOpen,
  Database,
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
  | "databases"
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

export const primaryNavigationSections: readonly NavigationSection[] = [
  { id: "workbench", labelKey: "navWorkbench", descriptionKey: "navWorkbenchDescription", icon: LayoutDashboard },
  { id: "chat", labelKey: "navChat", descriptionKey: "navChatDescription", icon: MessageSquareText },
  { id: "knowledge", labelKey: "navKnowledge", descriptionKey: "navKnowledgeDescription", icon: BookOpen },
  { id: "databases", labelKey: "navDatabases", descriptionKey: "navDatabasesDescription", icon: Database },
  { id: "analysis", labelKey: "navAnalysis", descriptionKey: "navAnalysisDescription", icon: BarChart3 },
  { id: "extensions", labelKey: "navExtensions", descriptionKey: "navExtensionsDescription", icon: Puzzle },
];

export const utilityNavigationSections: readonly NavigationSection[] = [
  { id: "settings", labelKey: "navSettings", descriptionKey: "navSettingsDescription", icon: Settings },
];

const secondarySections: readonly NavigationSection[] = [
  { id: "diagnostics", labelKey: "navDiagnostics", descriptionKey: "navDiagnosticsDescription", icon: Activity },
];

export const navigationSections: readonly NavigationSection[] = [
  ...primaryNavigationSections,
  ...utilityNavigationSections,
  ...secondarySections,
];

export function getNavigationSection(id: SectionId) {
  return navigationSections.find((section) => section.id === id) ?? navigationSections[0];
}
