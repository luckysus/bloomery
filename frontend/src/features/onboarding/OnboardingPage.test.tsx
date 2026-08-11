import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import OnboardingPage from "./OnboardingPage";
import { desktop } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    saveProviderProfile: vi.fn().mockResolvedValue({
      id: "profile-llm",
      kind: "open_ai_compatible",
      display_name: "OpenAI Compatible",
      base_url: "https://api.example.com/v1",
      model_id: "steel-model",
      enabled: true,
      revision: 1,
      secret_generation: 0,
      secret_configured: false,
    }),
    setProviderSecret: vi.fn().mockResolvedValue({ configured: true }),
    setDefaultProvider: vi.fn().mockResolvedValue(undefined),
    testProviderProfile: vi.fn().mockResolvedValue({ ok: true, status_code: 200, error_code: null, elapsed_ms: 12 }),
    getSetting: vi.fn().mockResolvedValue(null),
    listProviderProfiles: vi.fn().mockResolvedValue([]),
    setSetting: vi.fn().mockResolvedValue(undefined),
    installBundledSteelPackage: vi.fn().mockResolvedValue({}),
  },
}));

describe("OnboardingPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("resumes at the persisted retrieval step without asking for the LLM key again", async () => {
    vi.mocked(desktop.getSetting).mockResolvedValueOnce(JSON.stringify({
      version: 1,
      step: "retrieval",
      llm_profile_id: "profile-llm",
      retrieval_state: "skipped",
      mineru_configured: false,
    }));
    vi.mocked(desktop.listProviderProfiles).mockResolvedValueOnce([{
      id: "profile-llm",
      kind: "open_ai_compatible",
      display_name: "OpenAI Compatible",
      base_url: "https://api.example.com/v1",
      model_id: "steel-model",
      enabled: true,
      revision: 1,
      secret_generation: 1,
      secret_configured: true,
    }]);

    render(<OnboardingPage onComplete={vi.fn()} />);

    await screen.findByDisplayValue("BAAI/bge-m3");
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
  });

  it("persists the LLM step when first-run setup begins", async () => {
    render(<OnboardingPage onComplete={vi.fn()} />);

    fireEvent.click(screen.getAllByRole("button")[0]);

    await waitFor(() => expect(desktop.setSetting).toHaveBeenCalledWith(
      "onboarding.progress",
      expect.stringContaining('"step":"llm"'),
    ));
  });

  it("configures an LLM without rendering the submitted API key", async () => {
    const onComplete = vi.fn();
    render(<OnboardingPage onComplete={onComplete} />);

    fireEvent.click(screen.getByRole("button", { name: "开始配置" }));
    fireEvent.change(screen.getByLabelText("模型 ID"), { target: { value: "steel-model" } });
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "sk-secret-value" } });
    fireEvent.click(screen.getByRole("button", { name: "测试 LLM 并继续" }));

    await waitFor(() => expect(desktop.testProviderProfile).toHaveBeenCalledWith("profile-llm", "chat"));
    expect(screen.getByRole("heading", { name: "检索服务" })).toBeInTheDocument();
    expect(screen.queryByText("sk-secret-value")).not.toBeInTheDocument();
    expect(desktop.setProviderSecret).toHaveBeenCalledWith("profile-llm", "api_key", "sk-secret-value");
    expect(desktop.setDefaultProvider).toHaveBeenCalledWith("chat", "profile-llm");
  });

  it("allows optional retrieval services to be skipped and persists completion", async () => {
    const onComplete = vi.fn();
    render(<OnboardingPage onComplete={onComplete} />);

    fireEvent.click(screen.getByRole("button", { name: "开始配置" }));
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "sk-test-value" } });
    fireEvent.click(screen.getByRole("button", { name: "测试 LLM 并继续" }));
    await screen.findByRole("heading", { name: "检索服务" });
    fireEvent.click(screen.getByRole("button", { name: "暂时跳过" }));
    await screen.findByRole("heading", { name: "完成配置" });
    fireEvent.click(screen.getByRole("button", { name: "进入工作台" }));

    await waitFor(() => expect(desktop.setSetting).toHaveBeenCalled());
    expect(desktop.setSetting).toHaveBeenCalledWith(
      "onboarding.completed",
      expect.stringContaining('"completed":true'),
    );
    expect(onComplete).toHaveBeenCalledOnce();
  });

  it("shows an actionable provider error and stays on the LLM step", async () => {
    vi.mocked(desktop.testProviderProfile).mockResolvedValueOnce({
      ok: false,
      status_code: 401,
      error_code: "authentication",
      elapsed_ms: 20,
    });

    render(<OnboardingPage onComplete={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "开始配置" }));
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "sk-test-value" } });
    fireEvent.click(screen.getByRole("button", { name: "测试 LLM 并继续" }));

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("凭据验证失败"));
    expect(screen.getByRole("heading", { name: "连接 LLM" })).toBeInTheDocument();
  });

  it("installs the bundled steel package before entering the workbench", async () => {
    const onComplete = vi.fn();
    render(<OnboardingPage onComplete={onComplete} />);

    fireEvent.click(screen.getAllByRole("button")[0]);
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "sk-test-value" } });
    fireEvent.click(screen.getAllByRole("button")[0]);
    await screen.findByRole("heading", { level: 2 });
    fireEvent.click(screen.getAllByRole("button")[0]);
    await screen.findByRole("heading", { level: 2 });
    fireEvent.click(screen.getAllByRole("button")[0]);

    const mockedDesktop = desktop as unknown as { installBundledSteelPackage: ReturnType<typeof vi.fn> };
    await waitFor(() => expect(mockedDesktop.installBundledSteelPackage).toHaveBeenCalledOnce());
    expect(onComplete).toHaveBeenCalledOnce();
  });

  it("keeps onboarding incomplete when the bundled steel package cannot be installed", async () => {
    const mockedDesktop = desktop as unknown as { installBundledSteelPackage: ReturnType<typeof vi.fn> };
    mockedDesktop.installBundledSteelPackage.mockRejectedValueOnce(new Error("steel package unavailable"));
    const onComplete = vi.fn();
    render(<OnboardingPage onComplete={onComplete} />);

    fireEvent.click(screen.getAllByRole("button")[0]);
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "sk-test-value" } });
    fireEvent.click(screen.getAllByRole("button")[0]);
    await screen.findByRole("heading", { level: 2 });
    fireEvent.click(screen.getAllByRole("button")[0]);
    await screen.findByRole("heading", { level: 2 });
    fireEvent.click(screen.getAllByRole("button")[0]);

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("steel package unavailable"));
    expect(desktop.setSetting).not.toHaveBeenCalledWith("onboarding.completed", expect.any(String));
    expect(onComplete).not.toHaveBeenCalled();
  });
});
