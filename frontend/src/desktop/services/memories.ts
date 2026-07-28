import { invokeDesktop } from "./tauri";

export type DesktopMemory = {
  id?: string;
  scope: "global" | "project" | "domain";
  type: "user" | "project" | "domain" | "feedback" | "reference";
  title: string;
  description: string;
  body: string;
  tags_json: string;
  enabled: boolean;
  archived_at?: string | null;
  created_at?: string;
  updated_at?: string;
};

export type DesktopMemorySuggestion = {
  id: string;
  scope: DesktopMemory["scope"];
  type: DesktopMemory["type"];
  title: string;
  description: string;
  body: string;
  tags_json: string;
  reason: string;
  evidence: string;
};

export function listMemories() {
  return invokeDesktop<DesktopMemory[]>("list_memories");
}

export function listArchivedMemories() {
  return invokeDesktop<DesktopMemory[]>("list_archived_memories");
}

export function searchMemories(query: string) {
  return invokeDesktop<DesktopMemory[]>("search_memories", { query });
}

export function suggestMemories(limit = 6) {
  return invokeDesktop<DesktopMemorySuggestion[]>("suggest_memories", { limit });
}

export function saveMemory(memory: DesktopMemory) {
  return invokeDesktop<DesktopMemory>("save_memory", { memory });
}

export function archiveMemory(id: string) {
  return invokeDesktop<void>("archive_memory", { id });
}

export function restoreMemory(id: string) {
  return invokeDesktop<void>("restore_memory", { id });
}
