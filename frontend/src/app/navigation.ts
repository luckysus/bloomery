import { Activity, MessageSquareText, Settings } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { MessageKey } from "../i18n/locale";

export type SectionId =
  | "chat"
  | "settings"
  | "diagnostics";

export interface NavigationSection {
  id: SectionId;
  labelKey: MessageKey;
  descriptionKey: MessageKey;
  icon: LucideIcon;
}

export const primaryNavigationSections: readonly NavigationSection[] = [
  { id: "chat", labelKey: "navChat", descriptionKey: "navChatDescription", icon: MessageSquareText },
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
