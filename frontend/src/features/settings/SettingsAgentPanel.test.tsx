import { describe, expect, it } from "vitest";
import { normalizeAgentPreferences } from "./SettingsAgentPanel";

describe("Agent settings import compatibility", () => {
  it("keeps legacy retries as network retries and supplies separate recovery and deadline defaults", () => {
    const legacy = normalizeAgentPreferences('{"retries":4,"allowShell":false}');
    expect(legacy).toMatchObject({ retries: 4, recoveryRetries: 2, runTimeoutSeconds: 1800, workingDirectory: "", allowShell: false });
    const imported = normalizeAgentPreferences(JSON.stringify({ retries: -2, recoveryRetries: 100, runTimeoutSeconds: 1, workingDirectory: " F:/research ", allowShell: "true" }));
    expect(imported).toMatchObject({ retries: 0, recoveryRetries: 10, runTimeoutSeconds: 30, workingDirectory: "F:/research", allowShell: false });
  });
});
