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
    listDomainPackages: vi.fn(),
    installDomainPackage: vi.fn(),
    activateDomainPackage: vi.fn(),
    previewRemoveDomainPackage: vi.fn(),
    removeDomainPackage: vi.fn(),
  },
}));

describe("ExtensionsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listDomainPackages).mockResolvedValue([]);
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

  it("shows installed domain packages and activates an inactive version", async () => {
    vi.mocked(desktop.listDomainPackages).mockResolvedValue([
      {
        id: "steel",
        version: "1.0.0",
        path: "C:/AppData/Bloomery/domains/steel/1.0.0",
        package_sha256: "0123456789abcdef0123456789abcdef",
        trust: "OfficialSigned",
        manifest: {
          id: "steel",
          version: "1.0.0",
          author: "Bloomery",
          license: "Apache-2.0",
          builtin_tool_allowlist: ["knowledge.query"],
          mcp_recommendations: [],
          assets: [],
        },
        installed_at: "2026-08-06T00:00:00Z",
        active: false,
      },
    ]);
    vi.mocked(desktop.activateDomainPackage).mockResolvedValue({
      ...(await desktop.listDomainPackages())[0],
      active: true,
    } as never);

    render(<ExtensionsPage />);

    expect(await screen.findByRole("heading", { name: "extensionsDomainsTitle" })).toBeInTheDocument();
    expect(screen.getByText("steel")).toBeInTheDocument();
    expect(screen.getByText("extensionsDomainOfficial")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "extensionsDomainActivate" }));

    await waitFor(() => expect(desktop.activateDomainPackage).toHaveBeenCalledWith("steel", "1.0.0"));
  });
});
