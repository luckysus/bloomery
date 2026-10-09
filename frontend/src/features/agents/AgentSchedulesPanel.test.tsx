import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop, type AgentSchedule, type Conversation } from "../../bridge/desktop";
import AgentSchedulesPanel from "./AgentSchedulesPanel";

vi.mock("../../bridge/desktop", () => ({ desktop: {
  listAgentSchedules: vi.fn(), listConversations: vi.fn(), saveAgentSchedule: vi.fn(),
  setAgentScheduleEnabled: vi.fn(), deleteAgentSchedule: vi.fn(),
} }));

const conversations: Conversation[] = [
  { id: "conversation-1", title: "材料研究", created_at: "2026-10-09T01:00:00Z", updated_at: "2026-10-09T01:00:00Z", pinned: false, archived: false },
  { id: "conversation-2", title: "实验结果", created_at: "2026-10-09T01:00:00Z", updated_at: "2026-10-09T01:00:00Z", pinned: false, archived: false },
];
const job: AgentSchedule = {
  id: "schedule-1", workspaceId: "local", agentId: "knowledge", conversationId: "conversation-1",
  expression: "0 9 * * *", timezone: "Asia/Shanghai", prompt: "整理材料标准中的证据缺口", identity: "local",
  recurring: true, durable: true, enabled: true, nextRunAtUtc: "2026-10-10T01:00:00Z", lastSlotAtUtc: null,
  lastError: null, lastRunId: null, lastRunState: null,
};
let schedules: AgentSchedule[];

async function ready() {
  await waitFor(() => expect(screen.getByRole("combobox", { name: "结果会话" })).toBeEnabled());
}

describe("AgentSchedulesPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks(); schedules = [];
    vi.mocked(desktop.listConversations).mockResolvedValue(conversations);
    vi.mocked(desktop.listAgentSchedules).mockImplementation(async () => schedules.map((entry) => ({ ...entry })));
    vi.mocked(desktop.saveAgentSchedule).mockImplementation(async (request) => {
      const saved = { ...job, ...request, id: request.id ?? job.id, recurring: request.recurring ?? true };
      schedules = [...schedules.filter((entry) => entry.id !== saved.id), saved];
      return saved;
    });
    vi.mocked(desktop.setAgentScheduleEnabled).mockImplementation(async (id, enabled) => {
      schedules = schedules.map((entry) => entry.id === id ? { ...entry, enabled } : entry);
    });
    vi.mocked(desktop.deleteAgentSchedule).mockImplementation(async (id) => { schedules = schedules.filter((entry) => entry.id !== id); });
  });

  it("creates a schedule for the selected expert and chosen result conversation", async () => {
    render(<AgentSchedulesPanel agentId="knowledge" />);
    await ready();
    fireEvent.change(screen.getByRole("combobox", { name: "结果会话" }), { target: { value: "conversation-2" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Cron 表达式" }), { target: { value: "30 10 * * 1" } });
    fireEvent.change(screen.getByRole("textbox", { name: "时区" }), { target: { value: "Asia/Tokyo" } });
    fireEvent.change(screen.getByRole("textbox", { name: "执行指令" }), { target: { value: "核验实验结果与标准的差异" } });
    fireEvent.change(screen.getByRole("combobox", { name: "重复执行" }), { target: { value: "false" } });
    fireEvent.click(screen.getByRole("button", { name: "创建计划" }));
    await waitFor(() => expect(desktop.saveAgentSchedule).toHaveBeenCalledWith({
      agentId: "knowledge", conversationId: "conversation-2", expression: "30 10 * * 1", timezone: "Asia/Tokyo",
      prompt: "核验实验结果与标准的差异", recurring: false, enabled: true,
    }));
    expect(await screen.findByText("核验实验结果与标准的差异")).toBeInTheDocument();
    expect(await screen.findByText("实验结果 · 30 10 * * 1 · Asia/Tokyo")).toBeInTheDocument();
  });

  it("persists pause and enable changes on the existing schedule", async () => {
    schedules = [job];
    render(<AgentSchedulesPanel agentId="knowledge" />);
    await ready();
    fireEvent.click(screen.getByRole("checkbox", { name: "启用" }));
    await waitFor(() => expect(desktop.setAgentScheduleEnabled).toHaveBeenCalledWith(job.id, false));
    expect(await screen.findByText("已暂停")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "启用" })).toBeEnabled());
    fireEvent.click(screen.getByRole("checkbox", { name: "启用" }));
    await waitFor(() => expect(desktop.setAgentScheduleEnabled).toHaveBeenLastCalledWith(job.id, true));
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "启用" })).toBeChecked());
  });

  it("deletes the visible expert schedule and leaves other experts' schedules intact", async () => {
    schedules = [job, { ...job, id: "report-schedule", agentId: "report", prompt: "报告整理计划" }];
    render(<AgentSchedulesPanel agentId="knowledge" />);
    await ready();
    expect(screen.queryByText("报告整理计划")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "删除计划" }));
    await waitFor(() => expect(desktop.deleteAgentSchedule).toHaveBeenCalledWith(job.id));
    expect(await screen.findByText("这个 Agent 还没有定时计划。")).toBeInTheDocument();
    expect(schedules.map((entry) => entry.id)).toEqual(["report-schedule"]);
  });

  it("keeps the instruction available for retry when saving fails", async () => {
    vi.mocked(desktop.saveAgentSchedule).mockRejectedValueOnce(new Error("计划保存服务不可用"));
    render(<AgentSchedulesPanel agentId="knowledge" conversationId="conversation-2" />);
    await ready();
    fireEvent.change(screen.getByRole("textbox", { name: "执行指令" }), { target: { value: "保留待保存指令" } });
    fireEvent.click(screen.getByRole("button", { name: "创建计划" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("计划保存服务不可用");
    expect(screen.getByRole("textbox", { name: "执行指令" })).toHaveValue("保留待保存指令");
    expect(screen.getByRole("button", { name: "创建计划" })).toBeEnabled();
    expect(schedules).toEqual([]);
  });

  it("shows a loading service error and recovers when refreshed", async () => {
    vi.mocked(desktop.listAgentSchedules).mockRejectedValueOnce(new Error("无法读取计划"));
    render(<AgentSchedulesPanel agentId="knowledge" />);
    expect(await screen.findByRole("alert")).toHaveTextContent("无法读取计划");
    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    await ready();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(desktop.listAgentSchedules).toHaveBeenCalledTimes(2);
  });
});
