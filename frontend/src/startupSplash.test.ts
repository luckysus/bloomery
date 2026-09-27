import { describe, expect, it } from "vitest";
import indexHtml from "../index.html?raw";

describe("startup shell", () => {
  it("keeps the Tauri window non-blank before React mounts", () => {
    expect(indexHtml).toContain('id="suna-boot-screen"');
    expect(indexHtml).toContain("SUNA");
  });
});
