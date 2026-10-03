import { describe, expect, it } from "vitest";
import { settingsErrorMessage } from "./settingsError";

describe("settingsErrorMessage", () => {
  const fallback = "设置操作失败，请重试";

  it.each([
    "password=super-secret",
    "api_key: sk-live-secret",
    "postgresql://postgres:secret@127.0.0.1:5432/suna",
    "SQLSTATE 28P01: password authentication failed",
    "SELECT password FROM credentials WHERE id = 1",
  ])("hides sensitive low-level details: %s", (message) => {
    const rendered = settingsErrorMessage(new Error(message), fallback);
    expect(rendered).toBe(fallback);
    expect(rendered).not.toContain("secret");
    expect(rendered).not.toContain("password");
  });

  it("keeps a short user-facing error actionable", () => {
    expect(settingsErrorMessage(new Error("连接超时，请检查服务是否运行"), fallback)).toBe("连接超时，请检查服务是否运行");
  });

  it("uses the fallback for non-error values", () => {
    expect(settingsErrorMessage({ reason: "unknown" }, fallback)).toBe(fallback);
  });
});
