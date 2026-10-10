import { useCallback, useEffect, useState } from "react";
import type { SectionId } from "./navigation";

/** 第 81 章页面路由：每个功能区一个稳定路径。 */
export const SECTION_PATHS: Record<SectionId, string> = {
  chat: "/chat",
  knowledge: "/knowledge",
  literature: "/literature",
  data: "/data",
  prediction: "/prediction",
  optimization: "/optimization",
  experiment: "/experiment",
  agents: "/agents",
  tools: "/tools",
  mcp: "/mcp",
  skills: "/skills",
  models: "/models",
  reports: "/reports",
  settings: "/settings",
  account: "/account",
  about: "/about",
  diagnostics: "/diagnostics",
};

const PATH_SECTIONS = new Map<string, SectionId>(
  (Object.entries(SECTION_PATHS) as [SectionId, string][]).map(([id, path]) => [path, id]),
);

/** 归一化路径：去掉末尾斜杠，根路径视为 `/`。 */
function normalizePath(pathname: string): string {
  const trimmed = pathname.replace(/\/+$/, "");
  return trimmed === "" ? "/" : trimmed;
}

/** 路径 → 功能区；未知路径返回 null，由调用方决定兜底。 */
export function sectionFromPath(pathname: string): SectionId | null {
  const path = normalizePath(pathname);
  if (path === "/") return "chat";
  return PATH_SECTIONS.get(path) ?? null;
}

export function pathForSection(section: SectionId): string {
  return SECTION_PATHS[section];
}

/** 地址栏是否为根路径（无显式路由）。 */
export function isRootPath(pathname: string): boolean {
  return normalizePath(pathname) === "/";
}

function currentSection(): SectionId {
  if (typeof window === "undefined") return "chat";
  return sectionFromPath(window.location.pathname) ?? "chat";
}

/**
 * 在 `activeSection` 状态导航之上加一层 URL 路由：
 * 初始状态取自地址栏，导航时同步 `history`，并响应浏览器前进/后退。
 */
export function useSectionRoute(): [SectionId, (section: SectionId) => void] {
  const [section, setSection] = useState<SectionId>(currentSection);
  useEffect(() => {
    const onPopState = () => setSection(currentSection());
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);
  const navigate = useCallback((next: SectionId) => {
    setSection(next);
    const path = pathForSection(next);
    if (typeof window !== "undefined" && normalizePath(window.location.pathname) !== path) {
      try {
        window.history.pushState(null, "", path);
      } catch {
        /* 受限 webview 下退化为纯状态导航 */
      }
    }
  }, []);
  return [section, navigate];
}
