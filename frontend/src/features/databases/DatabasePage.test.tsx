import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DatabasePage from "./DatabasePage";

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
    listDatabaseConnections: vi.fn(),
    listDatabases: vi.fn(),
    listDatabaseTables: vi.fn(),
    submitDatabaseQuery: vi.fn(),
    getDatabaseQueryResult: vi.fn(),
    listDatabaseQueryResults: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    listBackgroundTasks: vi.fn(),
    saveSteelDataset: vi.fn(),
    activateSteelDataset: vi.fn(),
  },
}));

import { desktop } from "../../bridge/desktop";

const connection = {
  id: "c1",
  display_name: "3 号高炉",
  host: "192.168.1.10",
  port: 1433,
  username: "sa",
  timeout_ms: 10000,
  enabled: true,
  secret_configured: true,
  last_checked_at: null,
  last_latency_ms: null,
  last_version: null,
  last_error: null,
};

const runningTask = {
  id: "task-1",
  kind: "database_query",
  state: "running" as const,
  progress: 10,
  attempt: 1,
  error_code: null,
  cancel_requested: false,
  can_cancel: true,
  can_retry: false,
  created_at: "2026-08-18T10:00:00Z",
  updated_at: "2026-08-18T10:00:01Z",
};

const completedResult = {
  task_id: "task-1",
  connection_id: "c1",
  database_name: "SteelWorks",
  query_text: "SELECT heat_id FROM dbo.heats",
  row_count: 2,
  truncated: false,
  duration_ms: 120,
  csv_path: "C:/cache/task-1.csv",
  columns: ["heat_id", "carbon_pct"],
  rows: [
    ["H1", "0.18"],
    ["H2", "0.21"],
  ],
  created_at: "2026-08-18T10:00:02Z",
};

describe("DatabasePage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
    vi.mocked(desktop.listDatabases).mockResolvedValue(["master", "SteelWorks"]);
    vi.mocked(desktop.listDatabaseTables).mockResolvedValue(["dbo.heats", "dbo.chemistry"]);
    vi.mocked(desktop.listDatabaseQueryResults).mockResolvedValue([]);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
  });

  it("renders workspace landmarks", async () => {
    render(<DatabasePage />);
    expect(await screen.findByRole("heading", { name: "dbTitle" })).toBeInTheDocument();
    expect(await screen.findByRole("combobox", { name: "dbConnectionLabel" })).toBeInTheDocument();
    expect(await screen.findByRole("combobox", { name: "dbDatabaseLabel" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "dbSqlLabel" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "dbRun" })).toBeInTheDocument();
  });

  it("shows empty guidance without connections", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([]);
    render(<DatabasePage />);
    expect(await screen.findByText("dbEmptyConnections")).toBeInTheDocument();
  });

  it("lists tables from the current connection", async () => {
    render(<DatabasePage />);
    expect(await screen.findByRole("button", { name: "dbo.heats" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "dbo.chemistry" })).toBeInTheDocument();
  });

  it("fills the editor from a table name click", async () => {
    render(<DatabasePage />);
    fireEvent.click(await screen.findByRole("button", { name: "dbo.heats" }));
    const editor = screen.getByRole("textbox", { name: "dbSqlLabel" }) as HTMLTextAreaElement;
    expect(editor.value).toBe("SELECT TOP (500) * FROM [dbo].[heats]");
  });

  it("submits a query and renders rows after completion", async () => {
    vi.mocked(desktop.submitDatabaseQuery).mockResolvedValue(runningTask);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([
      { ...runningTask, state: "completed" as const, progress: 100 },
    ]);
    vi.mocked(desktop.getDatabaseQueryResult).mockResolvedValue(completedResult);

    render(<DatabasePage />);
    fireEvent.change(await screen.findByRole("textbox", { name: "dbSqlLabel" }), {
      target: { value: "SELECT heat_id FROM dbo.heats" },
    });
    fireEvent.click(screen.getByRole("button", { name: "dbRun" }));

    expect(desktop.submitDatabaseQuery).toHaveBeenCalledWith({
      connection_id: "c1",
      database: null,
      sql: "SELECT heat_id FROM dbo.heats",
      row_limit: 500,
    });
    expect(await screen.findByRole("table")).toBeInTheDocument();
    expect(await screen.findByText("H2")).toBeInTheDocument();
    expect(screen.queryByText(/dbTruncatedNotice/)).not.toBeInTheDocument();
  });

  it("cancels a running query", async () => {
    vi.mocked(desktop.submitDatabaseQuery).mockResolvedValue(runningTask);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([runningTask]);
    vi.mocked(desktop.cancelBackgroundTask).mockResolvedValue({
      ...runningTask,
      cancel_requested: true,
    });

    render(<DatabasePage />);
    fireEvent.change(await screen.findByRole("textbox", { name: "dbSqlLabel" }), {
      target: { value: "SELECT 1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "dbRun" }));
    fireEvent.click(await screen.findByRole("button", { name: "dbCancel" }));
    expect(desktop.cancelBackgroundTask).toHaveBeenCalledWith("task-1");
  });

  it("shows truncated notice", async () => {
    vi.mocked(desktop.submitDatabaseQuery).mockResolvedValue(runningTask);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([
      { ...runningTask, state: "completed" as const, progress: 100 },
    ]);
    vi.mocked(desktop.getDatabaseQueryResult).mockResolvedValue({
      ...completedResult,
      row_count: 500,
      truncated: true,
      rows: Array.from({ length: 500 }, () => ["1"]),
      columns: ["n"],
    });

    render(<DatabasePage />);
    fireEvent.change(await screen.findByRole("textbox", { name: "dbSqlLabel" }), {
      target: { value: "SELECT 1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "dbRun" }));
    expect(await screen.findByText(/dbTruncatedNotice/)).toBeInTheDocument();
  });

  it("sends a result to data analysis", async () => {
    vi.mocked(desktop.submitDatabaseQuery).mockResolvedValue(runningTask);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([
      { ...runningTask, state: "completed" as const, progress: 100 },
    ]);
    vi.mocked(desktop.getDatabaseQueryResult).mockResolvedValue(completedResult);
    vi.mocked(desktop.saveSteelDataset).mockResolvedValue({ id: "ds-1" } as never);
    vi.mocked(desktop.activateSteelDataset).mockResolvedValue({} as never);
    const onOpenSection = vi.fn();

    render(<DatabasePage onOpenSection={onOpenSection} />);
    fireEvent.change(await screen.findByRole("textbox", { name: "dbSqlLabel" }), {
      target: { value: "SELECT 1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "dbRun" }));
    fireEvent.click(await screen.findByRole("button", { name: "dbSendToAnalysis" }));

    await waitFor(() =>
      expect(desktop.saveSteelDataset).toHaveBeenCalledWith({ sourcePath: "C:/cache/task-1.csv" }),
    );
    await waitFor(() => expect(desktop.activateSteelDataset).toHaveBeenCalledWith("ds-1"));
    expect(onOpenSection).toHaveBeenCalledWith("analysis");
  });

  it("fills the editor from history", async () => {
    vi.mocked(desktop.listDatabaseQueryResults).mockResolvedValue([
      {
        task_id: "old-1",
        database_name: "SteelWorks",
        query_text: "SELECT TOP (10) * FROM dbo.heats",
        row_count: 10,
        truncated: false,
        duration_ms: 300,
        created_at: "2026-08-18T09:00:00Z",
      },
    ]);
    render(<DatabasePage />);
    fireEvent.click(
      await screen.findByRole("button", { name: /SELECT TOP \(10\) \* FROM dbo\.heats/ }),
    );
    const editor = screen.getByRole("textbox", { name: "dbSqlLabel" }) as HTMLTextAreaElement;
    expect(editor.value).toBe("SELECT TOP (10) * FROM dbo.heats");
  });
});
