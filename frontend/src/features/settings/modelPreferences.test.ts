import { describe, expect, it } from "vitest";
import { modelRuntimeDefaults, normalizeModelRuntime, parseModelRuntime, serializeModelRuntime } from "./modelPreferences";

describe("modelPreferences", () => {
  it("缺失或非法字段回落到默认值", () => {
    expect(parseModelRuntime(null)).toEqual(modelRuntimeDefaults);
    expect(parseModelRuntime("not json")).toEqual(modelRuntimeDefaults);
    expect(parseModelRuntime(JSON.stringify({ temperature: "hot" }))).toEqual(modelRuntimeDefaults);
  });

  it("读取 snake_case 存储格式", () => {
    const parsed = parseModelRuntime(JSON.stringify({ temperature: 0.6, max_tokens: 8192, context_length: 65536, timeout_seconds: 300 }));
    expect(parsed).toEqual({ temperature: 0.6, maxTokens: 8192, contextLength: 65536, timeoutSeconds: 300 });
  });

  it("把取值收敛到支持范围", () => {
    expect(normalizeModelRuntime({ temperature: 9 }).temperature).toBe(2);
    expect(normalizeModelRuntime({ temperature: -1 }).temperature).toBe(0);
    expect(normalizeModelRuntime({ maxTokens: 1 }).maxTokens).toBe(256);
    expect(normalizeModelRuntime({ contextLength: 1 }).contextLength).toBe(1024);
  });

  it("序列化后仍可原样解析回来", () => {
    const value = { temperature: 0.7, maxTokens: 2048, contextLength: 16384, timeoutSeconds: 60 };
    expect(parseModelRuntime(serializeModelRuntime(value))).toEqual(value);
    expect(serializeModelRuntime(value)).toContain('"max_tokens":2048');
  });
});
