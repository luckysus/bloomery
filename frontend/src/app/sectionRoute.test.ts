import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { SECTION_PATHS, isRootPath, pathForSection, sectionFromPath, useSectionRoute } from "./sectionRoute";

const chapter81Routes = ["/chat", "/knowledge", "/literature", "/data", "/prediction", "/optimization", "/experiment", "/agents", "/tools", "/settings"];

describe("sectionRoute", () => {
  it("把根路径和已知路径映射到功能区", () => {
    expect(sectionFromPath("/")).toBe("chat");
    expect(sectionFromPath("/chat")).toBe("chat");
    expect(sectionFromPath("/knowledge")).toBe("knowledge");
    expect(sectionFromPath("/tools")).toBe("tools");
  });

  it("归一化末尾斜杠并拒绝未知路径", () => {
    expect(sectionFromPath("/data/")).toBe("data");
    expect(sectionFromPath("/unknown")).toBeNull();
  });

  it("覆盖第 81 章要求的全部路由", () => {
    for (const path of chapter81Routes) expect(Object.values(SECTION_PATHS)).toContain(path);
    expect(pathForSection("settings")).toBe("/settings");
  });

  it("识别根路径", () => {
    expect(isRootPath("/")).toBe(true);
    expect(isRootPath("")).toBe(true);
    expect(isRootPath("/chat")).toBe(false);
  });
});

describe("useSectionRoute", () => {
  afterEach(() => window.history.replaceState(null, "", "/"));

  it("导航时更新状态并同步地址栏", () => {
    const { result } = renderHook(() => useSectionRoute());
    expect(result.current[0]).toBe("chat");
    act(() => result.current[1]("knowledge"));
    expect(result.current[0]).toBe("knowledge");
    expect(window.location.pathname).toBe("/knowledge");
  });

  it("响应浏览器前进/后退", () => {
    const { result } = renderHook(() => useSectionRoute());
    act(() => result.current[1]("data"));
    expect(result.current[0]).toBe("data");
    act(() => {
      window.history.replaceState(null, "", "/experiment");
      window.dispatchEvent(new PopStateEvent("popstate"));
    });
    expect(result.current[0]).toBe("experiment");
  });
});
