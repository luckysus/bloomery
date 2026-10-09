import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop, type AgentProfileSummary } from "../../bridge/desktop";
import AgentManagementPage from "./AgentManagementPage";

vi.mock("../../bridge/desktop", () => ({ desktop: {
  listAgentProfiles: vi.fn(), listAgentProfilePresets: vi.fn(), listProviderProfiles: vi.fn(), listToolCapabilities: vi.fn(),
  listMcpServers: vi.fn(), listMcpTools: vi.fn(), saveAgentProfile: vi.fn(), resetAgentProfile: vi.fn(), deleteAgentProfile: vi.fn(),
  listAgentSchedules: vi.fn(), listConversations: vi.fn(),
} }));
vi.mock("../settings/settingsModel", () => ({ getSettingValue: vi.fn(async () => '{"allowShell":false}'), parseObject: (raw: string) => JSON.parse(raw || "{}") }));

const profile: AgentProfileSummary = {
  id: "knowledge", name: "Knowledge Agent", description: "知识检索和证据引用", enabled: true, status: "available", preset: true,
  systemPrompt: "Only use verified evidence", providerId: null, toolIds: ["steel.knowledge_search"],
  permissionRestrictions: { allowFileAccess: true, allowShell: false, allowNetwork: true, allowDatabase: true, allowMcp: true, confirmDangerous: true },
  limits: { maxTurns: 20, maxToolCalls: 64, contextBudget: 32768, retries: 2, recoveryRetries: 2, runTimeoutSeconds: 1800 },
};

describe("AgentManagementPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listAgentProfiles).mockResolvedValue([profile]);
    vi.mocked(desktop.listAgentProfilePresets).mockResolvedValue([profile]);
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([{ id: "provider-1", display_name: "研究模型", model_id: "model-x", enabled: true, kind: "open_ai_compatible", base_url: "http://localhost", revision: 1, secret_generation: 0, secret_configured: false }]);
    vi.mocked(desktop.listToolCapabilities).mockResolvedValue([{ id: "steel.knowledge_search", name: "知识检索", description: "真实检索工具", source: "builtin", enabled: true }]);
    vi.mocked(desktop.listMcpServers).mockResolvedValue([]);
    vi.mocked(desktop.listAgentSchedules).mockResolvedValue([]);
    vi.mocked(desktop.listConversations).mockResolvedValue([]);
    vi.mocked(desktop.saveAgentProfile).mockImplementation(async (next) => ({ ...next, status: next.enabled ? "available" : "disabled" }));
    vi.mocked(desktop.resetAgentProfile).mockResolvedValue(profile);
    vi.mocked(desktop.deleteAgentProfile).mockResolvedValue(undefined);
  });

  it("saves the selected expert's prompt, provider, allowlist, restrictions and enabled state", async () => {
    const editor = render(<AgentManagementPage />);
    const prompt = await screen.findByRole("textbox", { name: "独立 System Prompt" });
    fireEvent.change(prompt, { target: { value: "Return cited evidence only" } });
    fireEvent.change(screen.getByRole("combobox", { name: "Provider / 模型" }), { target: { value: "provider-1" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "启用此 Agent" }));
    fireEvent.click(screen.getByRole("button", { name: "全部取消" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "网络访问" }));
    fireEvent.change(screen.getByRole("spinbutton", { name: "网络重试次数" }), { target: { value: "4" } });
    fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
    await waitFor(() => expect(desktop.saveAgentProfile).toHaveBeenCalledWith(expect.objectContaining({
      id: "knowledge", systemPrompt: "Return cited evidence only", providerId: "provider-1", enabled: false, toolIds: [],
      permissionRestrictions: expect.objectContaining({ allowNetwork: false }), limits: expect.objectContaining({ retries: 4, recoveryRetries: 2 }),
    })));
    expect(await screen.findByText("已保存")).toBeInTheDocument();
    const saved = vi.mocked(desktop.saveAgentProfile).mock.calls[0][0];
    vi.mocked(desktop.listAgentProfiles).mockResolvedValue([saved]);
    editor.unmount();
    render(<AgentManagementPage />);
    expect(await screen.findByRole("textbox", { name: "独立 System Prompt" })).toHaveValue("Return cited evidence only");
    expect(screen.getByRole("checkbox", { name: "启用此 Agent" })).not.toBeChecked();
    expect(screen.getByRole("combobox", { name: "Provider / 模型" })).toHaveValue("provider-1");
  });

  it("copies a preset into a custom expert and keeps preset deletion unavailable", async () => {
    render(<AgentManagementPage />);
    await screen.findByRole("textbox", { name: "独立 System Prompt" });
    expect(screen.queryByRole("button", { name: "删除自定义 Agent" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "创建自定义 Agent" }));
    fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
    await waitFor(() => expect(desktop.saveAgentProfile).toHaveBeenCalledWith(expect.objectContaining({ id: expect.stringMatching(/^custom-/), preset: false, systemPrompt: profile.systemPrompt })));
    fireEvent.click(screen.getByRole("button", { name: "删除自定义 Agent" }));
    await waitFor(() => expect(desktop.deleteAgentProfile).toHaveBeenCalledWith(expect.stringMatching(/^custom-/)));
    expect(await screen.findByRole("button", { name: "恢复此预设" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "恢复此预设" }));
    await waitFor(() => expect(desktop.resetAgentProfile).toHaveBeenCalledWith("knowledge"));
  });

  it("keeps unsaved edits visible when persistence fails", async () => {
    vi.mocked(desktop.saveAgentProfile).mockRejectedValue(new Error("保存失败"));
    render(<AgentManagementPage />);
    fireEvent.change(await screen.findByRole("textbox", { name: "独立 System Prompt" }), { target: { value: "Unsaved prompt" } });
    fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("保存失败");
    expect(screen.getByRole("textbox", { name: "独立 System Prompt" })).toHaveValue("Unsaved prompt");
  });
});
