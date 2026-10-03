import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import KnowledgeRetrievalSettings from "./KnowledgeRetrievalSettings";
import { desktop, isDesktopRuntime, type ProviderProfileResponse } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  isDesktopRuntime: vi.fn().mockReturnValue(true),
  desktop: {
    getSetting: vi.fn(),
    setSetting: vi.fn(),
    listKnowledgeBases: vi.fn(),
    listProviderProfiles: vi.fn(),
    setDefaultProvider: vi.fn(),
  },
}));

const embeddingProfile: ProviderProfileResponse = {
  id: "embedding-1",
  kind: "siliconflow",
  display_name: "SiliconFlow Embedding",
  base_url: "https://api.siliconflow.cn/v1",
  model_id: "BAAI/bge-m3",
  enabled: true,
  revision: 1,
  secret_generation: 1,
  secret_configured: true,
};

describe("KnowledgeRetrievalSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(isDesktopRuntime).mockReturnValue(true);
    vi.mocked(desktop.getSetting).mockImplementation(async (key) => {
      if (key === "onboarding.retrieval") return JSON.stringify({ plan: "free" });
      return null;
    });
    vi.mocked(desktop.setSetting).mockResolvedValue(undefined);
    vi.mocked(desktop.listKnowledgeBases).mockResolvedValue([]);
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([embeddingProfile]);
    vi.mocked(desktop.setDefaultProvider).mockResolvedValue(undefined);
  });

  it("lists configured embedding providers and persists the selected profile", async () => {
    render(<KnowledgeRetrievalSettings />);

    const provider = await screen.findByRole("combobox", { name: "Embedding Provider" });
    expect(screen.queryByRole("option", { name: "自定义 Provider 模型" })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "text-embedding-3-small" })).not.toBeInTheDocument();

    fireEvent.change(provider, { target: { value: embeddingProfile.id } });

    await waitFor(() => expect(desktop.setDefaultProvider).toHaveBeenCalledWith("embedding", embeddingProfile.id));
    expect(desktop.setSetting).toHaveBeenCalledWith(
      "onboarding.retrieval",
      expect.stringContaining(`"embedding_profile_id":"${embeddingProfile.id}"`),
    );
    expect(desktop.setSetting).toHaveBeenCalledWith(
      "knowledge.preferences",
      expect.stringContaining(`"embedding_profile_id":"${embeddingProfile.id}"`),
    );
  });

  it("shows an empty state when no usable embedding provider is configured", async () => {
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([]);
    render(<KnowledgeRetrievalSettings />);

    expect(await screen.findByText(/尚未配置可用的 Embedding Provider/)).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Embedding Provider" })).not.toBeInTheDocument();
  });
});
