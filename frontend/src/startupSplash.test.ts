import { describe, expect, it } from "vitest";
import indexHtml from "../index.html?raw";

describe("startup shell", () => {
  it("keeps the Tauri window non-blank before React mounts", () => {
    expect(indexHtml).toContain('id="bloomery-boot-screen"');
    expect(indexHtml).toContain("BLOOMERY");
  });
});
