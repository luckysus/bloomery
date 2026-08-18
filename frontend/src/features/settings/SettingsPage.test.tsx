import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SettingsPage from "./SettingsPage";
import { desktop, type PermissionRuleRecord, type ProviderProfileResponse } from "../../bridge/desktop";
import { ThemeProvider } from "../../theme/theme";

vi.mock("../../i18n/locale", () => ({
  useLocale: () => ({
    preference: "zh-CN",
    setPreference: vi.fn(),
    t: (key: string, params?: Record<string, string | number>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}));

vi.mock("../../bridge/desktop", () => ({
  isDesktopRuntime: vi.fn().mockReturnValue(false),
  desktop: {
    setNativeTheme: vi.fn().mockResolvedValue(undefined),
    listProviderProfiles: vi.fn(),
    getSetting: vi.fn(),
    setSetting: vi.fn(),
    saveProviderProfile: vi.fn(),
    setProviderSecret: vi.fn(),
    setDefaultProvider: vi.fn(),
    deleteProviderSecret: vi.fn(),
    deleteProviderProfile: vi.fn(),
    testProviderProfile: vi.fn(),
    listPermissionRules: vi.fn(),
    revokePermissionRule: vi.fn(),
    listDatabaseConnections: vi.fn(),
    saveDatabaseConnection: vi.fn(),
    deleteDatabaseConnection: vi.fn(),
    testDatabaseConnection: vi.fn(),
    listDatabaseTables: vi.fn(),
  },
}));

const chatProfile: ProviderProfileResponse = {
  id: "chat-1",
  kind: "open_ai_compatible",
  display_name: "Steel LLM",
  base_url: "https://api.example.com/v1",
  model_id: "steel-model",
  enabled: true,
  revision: 1,
  secret_generation: 1,
  secret_configured: true,
};

const embeddingProfile: ProviderProfileResponse = {
  id: "embedding-1",
  kind: "siliconflow",
  display_name: "SiliconFlow Embedding (free)",
  base_url: "https://api.siliconflow.cn/v1",
  model_id: "BAAI/bge-m3",
  enabled: true,
  revision: 1,
  secret_generation: 1,
  secret_configured: true,
};

const permissionRule: PermissionRuleRecord = {
  id: "rule-1",
  tool_id: "builtin.write_file",
  tool_version: { major: 1, minor: 0, patch: 0 },
  source: { kind: "builtin" },
  action: "execute",
  scope: { kind: "exact", value: { path: "draft.txt" } },
  effect: "allow",
};

describe("SettingsPage", () => {
  const renderSettings = () => render(
    <ThemeProvider>
      <SettingsPage />
    </ThemeProvider>,
  );

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([]);
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([chatProfile, embeddingProfile]);
    vi.mocked(desktop.getSetting).mockImplementation(async (key) => {
      if (key === "onboarding.completed") return JSON.stringify({ llm_profile_id: chatProfile.id });
      if (key === "onboarding.retrieval") {
        return JSON.stringify({ plan: "free", embedding_profile_id: embeddingProfile.id });
      }
      return null;
    });
    vi.mocked(desktop.setSetting).mockResolvedValue(undefined);
    vi.mocked(desktop.setProviderSecret).mockResolvedValue({ configured: true });
    vi.mocked(desktop.deleteProviderProfile).mockResolvedValue(undefined);
    vi.mocked(desktop.testProviderProfile).mockResolvedValue({
      ok: true,
      status_code: 200,
      error_code: null,
      elapsed_ms: 12,
    });
    vi.mocked(desktop.listPermissionRules).mockResolvedValue([permissionRule]);
    vi.mocked(desktop.revokePermissionRule).mockResolvedValue(undefined);
    vi.mocked(desktop.saveProviderProfile).mockImplementation(async (input) => ({
      ...chatProfile,
      id: input.id ?? "new-profile",
      kind: input.kind,
      display_name: input.display_name,
      base_url: input.base_url,
      model_id: input.model_id ?? null,
      enabled: input.enabled,
    }));
  });

  it("loads provider cards and never displays a configured secret", async () => {
    const { container } = renderSettings();

    expect(await screen.findByRole("heading", { name: "settingsTitle" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "languageLabel" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "themeTitle" })).toBeInTheDocument();
    for (const label of ["themeSystem", "themeLight", "themeDark"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(screen.getByDisplayValue("https://api.example.com/v1")).toBeInTheDocument();
    expect(screen.getAllByText("settingsSecretConfigured").length).toBeGreaterThan(0);
    expect(screen.getByText("settingsSecretCopy")).toBeInTheDocument();
    expect(container.querySelectorAll(".bloomery-eyebrow")).toHaveLength(0);
    expect(screen.queryByText("settingsLede")).not.toBeInTheDocument();
    expect(screen.queryByText("settingsPlanCopy")).not.toBeInTheDocument();
    expect(screen.queryByText("settingsChatDescription")).not.toBeInTheDocument();
    expect(screen.queryByText("settingsEmbeddingDescription")).not.toBeInTheDocument();
    expect(screen.queryByText("settingsRerankerDescription")).not.toBeInTheDocument();
    expect(screen.queryByText("settingsMineruDescription")).not.toBeInTheDocument();
    expect(screen.queryByText("permissionRulesCopy")).not.toBeInTheDocument();
    expect(screen.queryByText("secret-token")).not.toBeInTheDocument();
  });

  it("saves an edited provider and writes a replacement key through the secret bridge", async () => {
    renderSettings();

    const name = await screen.findByDisplayValue("Steel LLM");
    fireEvent.change(name, { target: { value: "Updated Steel LLM" } });
    const chatForm = name.closest("form");
    if (!chatForm) throw new Error("chat provider form is missing");
    fireEvent.change(within(chatForm).getByLabelText("provider.chat.apiKey"), {
      target: { value: "replacement-key" },
    });
    fireEvent.click(within(chatForm).getByRole("button", { name: "settingsSave" }));

    await waitFor(() =>
      expect(desktop.saveProviderProfile).toHaveBeenCalledWith({
        id: chatProfile.id,
        kind: chatProfile.kind,
        display_name: "Updated Steel LLM",
        base_url: chatProfile.base_url,
        model_id: chatProfile.model_id,
        credential_name: "api_key",
        enabled: true,
      }),
    );
    expect(desktop.setProviderSecret).toHaveBeenCalledWith(
      chatProfile.id,
      "api_key",
      "replacement-key",
    );
    expect(desktop.setDefaultProvider).toHaveBeenCalledWith("chat", chatProfile.id);
  });

  it("persists the SiliconFlow free or Pro selection without exposing credentials", async () => {
    renderSettings();

    const pro = await screen.findByLabelText("settingsPlanPro");
    fireEvent.click(pro);

    await waitFor(() =>
      expect(desktop.setSetting).toHaveBeenCalledWith(
        "onboarding.retrieval",
        expect.stringContaining('"plan":"pro"'),
      ),
    );
  });

  it("restores the previous SiliconFlow plan when persistence fails", async () => {
    vi.mocked(desktop.setSetting).mockRejectedValueOnce(new Error("settings unavailable"));
    renderSettings();

    const free = await screen.findByLabelText("settingsPlanFree");
    fireEvent.click(screen.getByLabelText("settingsPlanPro"));

    await waitFor(() => expect(free).toBeChecked());
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("reloads provider state when a secret write fails after profile save", async () => {
    vi.mocked(desktop.setProviderSecret).mockRejectedValueOnce(new Error("keyring unavailable"));
    renderSettings();

    const name = await screen.findByDisplayValue("Steel LLM");
    const chatForm = name.closest("form");
    if (!chatForm) throw new Error("chat provider form is missing");
    fireEvent.change(name, { target: { value: "Unsaved Steel LLM" } });
    fireEvent.change(within(chatForm).getByLabelText("provider.chat.apiKey"), {
      target: { value: "replacement-key" },
    });
    fireEvent.click(within(chatForm).getByRole("button", { name: "settingsSave" }));

    await waitFor(() => expect(desktop.listProviderProfiles).toHaveBeenCalledTimes(2));
    expect(screen.getByDisplayValue("Steel LLM")).toBeInTheDocument();
    expect(screen.queryByDisplayValue("Unsaved Steel LLM")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("lists persistent permission rules and revokes the selected rule", async () => {
    renderSettings();

    expect(await screen.findByText("builtin.write_file")).toBeInTheDocument();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    fireEvent.click(screen.getByRole("button", { name: /revoke permission|permissionRevoke/i }));

    await waitFor(() => expect(desktop.revokePermissionRule).toHaveBeenCalledWith("rule-1"));
  });
});
