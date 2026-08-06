import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ExtensionsPage from "./ExtensionsPage";
import { desktop } from "../../bridge/desktop";

vi.mock("../../i18n/locale", () => ({
  useLocale: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}));

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    listSkills: vi.fn(),
    setSkillEnabled: vi.fn(),
  },
}));

describe("ExtensionsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listSkills).mockResolvedValue({
      skills: [{
        name: "steel-review",
        description: "Review steel evidence",
        version: "1.0.0",
        compatibility: ["bloomery>=0.1.0"],
        source: { scope: "workspace", path: "C:/workspace/.claude/skills/steel-review/SKILL.md" },
        content_sha256: "abc123",
        enabled: false,
      }],
      errors: [],
    });
    vi.mocked(desktop.setSkillEnabled).mockResolvedValue({
      skills: [{
        name: "steel-review",
        description: "Review steel evidence",
        version: "1.0.0",
        compatibility: ["bloomery>=0.1.0"],
        source: { scope: "workspace", path: "C:/workspace/.claude/skills/steel-review/SKILL.md" },
        content_sha256: "abc123",
        enabled: true,
      }],
      errors: [],
    });
  });

  it("loads Skills and exposes their source and version", async () => {
    render(<ExtensionsPage />);

    expect(await screen.findByRole("heading", { name: "extensionsTitle" })).toBeInTheDocument();
    expect(screen.getByText("steel-review")).toBeInTheDocument();
    expect(screen.getByText("1.0.0")).toBeInTheDocument();
    expect(screen.getByText("extensionsScopeWorkspace")).toBeInTheDocument();
    expect(screen.getByText(/C:\/workspace/)).toBeInTheDocument();
  });

  it("enables a Skill through the desktop bridge", async () => {
    render(<ExtensionsPage />);

    const toggle = await screen.findByRole("checkbox", { name: "extensionsEnable steel-review" });
    fireEvent.click(toggle);

    await waitFor(() => expect(desktop.setSkillEnabled).toHaveBeenCalledWith("steel-review", true));
    expect(await screen.findByText("extensionsSaved")).toBeInTheDocument();
  });
});
