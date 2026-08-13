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
    listMcpServers: vi.fn(),
    saveMcpServer: vi.fn(),
    checkMcpServer: vi.fn(),
    restartMcpServer: vi.fn(),
    listMcpTools: vi.fn(),
    deleteMcpServer: vi.fn(),
  },
}));

describe("ExtensionsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listDomainPackages).mockResolvedValue([]);
    vi.mocked(desktop.listMcpServers).mockResolvedValue([]);
    vi.mocked(desktop.listSkills).mockResolvedValue({
      skills: [{
        name: "steel-review",
        description: "Review steel evidence",
        version: "1.0.0",
        compatibility: ["bloomery>=0.1.0"],
        source: { scope: "user", path: "C:/Users/example/.bloomery/skills/steel-review/SKILL.md" },
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
        source: { scope: "user", path: "C:/Users/example/.bloomery/skills/steel-review/SKILL.md" },
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
    expect(screen.getByText("extensionsScopeUser")).toBeInTheDocument();
    expect(screen.getByText(/C:\/Users\/example/)).toBeInTheDocument();
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
        trust: "official_signed",
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

  it("checks an MCP server and exposes its discovered tools", async () => {
    vi.mocked(desktop.listMcpServers).mockResolvedValue([{
      id: "mcp-1",
      display_name: "Steel MCP",
      server_id: "steel-mcp",
      transport: "stdio",
      url: null,
      executable: "powershell.exe",
      args: ["-NoProfile"],
      working_directory: null,
      inherited_env: ["SystemRoot"],
      env_names: ["STEEL_API_KEY"],
      timeout_ms: 30000,
      enabled: true,
      secret_configured: true,
      status: "unknown",
      last_error: null,
      last_checked_at: null,
      tool_count: 0,
    }]);
    vi.mocked(desktop.checkMcpServer).mockResolvedValue({
      status: "healthy",
      server_name: "steel-mcp",
      server_version: "1.0.0",
      tool_count: 1,
      resource_count: 0,
      prompt_count: 0,
      tools: [{ id: "mcp.steel-mcp.lookup", name: "lookup", description: "Look up steel" }],
      error: null,
      checked_at: "2026-08-09T00:00:00Z",
    });

    render(<ExtensionsPage />);

    expect(await screen.findByRole("heading", { name: "extensionsMcpTitle" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "extensionsMcpCheck" }));

    await waitFor(() => expect(desktop.checkMcpServer).toHaveBeenCalledWith("mcp-1"));
    expect(await screen.findByText("lookup")).toBeInTheDocument();
    expect(screen.getByText("extensionsMcpHealthy")).toBeInTheDocument();
  });

  it("repopulates inherited environment settings when editing an MCP server", async () => {
    vi.mocked(desktop.listMcpServers).mockResolvedValue([{
      id: "mcp-2",
      display_name: "Steel MCP",
      server_id: "steel-mcp",
      transport: "stdio",
      url: null,
      executable: "powershell.exe",
      args: ["-NoProfile"],
      working_directory: null,
      inherited_env: ["SystemRoot", "windir"],
      env_names: ["STEEL_API_KEY"],
      timeout_ms: 30000,
      enabled: true,
      secret_configured: true,
      status: "unknown",
      last_error: null,
      last_checked_at: null,
      tool_count: 0,
    }]);

    render(<ExtensionsPage />);

    fireEvent.click(await screen.findByRole("button", { name: "extensionsMcpEdit" }));

    expect(screen.getByRole("textbox", { name: "extensionsMcpInheritedEnvironment" })).toHaveValue("SystemRoot\nwindir");
  });

  it("sends an explicit environment credential clear request", async () => {
    vi.mocked(desktop.listMcpServers).mockResolvedValue([{
      id: "mcp-3",
      display_name: "Steel MCP",
      server_id: "steel-mcp",
      transport: "stdio",
      url: null,
      executable: "powershell.exe",
      args: [],
      working_directory: null,
      inherited_env: [],
      env_names: ["STEEL_API_KEY"],
      timeout_ms: 30000,
      enabled: true,
      secret_configured: true,
      status: "unknown",
      last_error: null,
      last_checked_at: null,
      tool_count: 0,
    }]);
    vi.mocked(desktop.saveMcpServer).mockResolvedValue({
      id: "mcp-3",
      display_name: "Steel MCP",
      server_id: "steel-mcp",
      transport: "stdio",
      url: null,
      executable: "powershell.exe",
      args: [],
      working_directory: null,
      inherited_env: [],
      env_names: [],
      timeout_ms: 30000,
      enabled: true,
      secret_configured: false,
      status: "unknown",
      last_error: null,
      last_checked_at: null,
      tool_count: 0,
    });

    render(<ExtensionsPage />);
    fireEvent.click(await screen.findByRole("button", { name: "extensionsMcpEdit" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "extensionsMcpClearEnvironment" }));
    fireEvent.click(screen.getByRole("button", { name: "extensionsMcpSave" }));

    await waitFor(() =>
      expect(desktop.saveMcpServer).toHaveBeenCalledWith(expect.objectContaining({
        clear_environment_credentials: true,
        env_values: {},
        replace_inherited_env: true,
      })),
    );
  });
});
