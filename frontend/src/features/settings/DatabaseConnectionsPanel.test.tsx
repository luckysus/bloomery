import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DatabaseConnectionsPanel from "./DatabaseConnectionsPanel";
import { desktop } from "../../bridge/desktop";

vi.mock("../../i18n/locale", () => ({
  useLocale: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}));

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    listDatabaseConnections: vi.fn(),
    saveDatabaseConnection: vi.fn(),
    deleteDatabaseConnection: vi.fn(),
    testDatabaseConnection: vi.fn(),
    listDatabaseTables: vi.fn(),
  },
}));

const connection = {
  id: "db-1",
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

describe("DatabaseConnectionsPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([]);
  });

  it("lists saved connections with host and database", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);

    render(<DatabaseConnectionsPanel />);

    expect(await screen.findByText("3 号高炉")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.10:1433 · sa")).toBeInTheDocument();
  });

  it("sends a new connection without the password field when empty", async () => {
    vi.mocked(desktop.saveDatabaseConnection).mockResolvedValue(connection);

    render(<DatabaseConnectionsPanel />);

    fireEvent.change(await screen.findByRole("textbox", { name: "settingsDatabaseDisplayName" }), { target: { value: "3 号高炉" } });
    fireEvent.change(screen.getByRole("textbox", { name: "settingsDatabaseHost" }), { target: { value: "192.168.1.10" } });
    fireEvent.change(screen.getByRole("spinbutton", { name: "settingsDatabasePort" }), { target: { value: "1433" } });
    fireEvent.change(screen.getByRole("textbox", { name: "settingsDatabaseUsername" }), { target: { value: "sa" } });
    fireEvent.click(screen.getByRole("button", { name: "settingsDatabaseSave" }));

    await waitFor(() =>
      expect(desktop.saveDatabaseConnection).toHaveBeenCalledWith(
        expect.objectContaining({
          host: "192.168.1.10",
          port: 1433,
          username: "sa",
          password: undefined,
        }),
      ),
    );
    expect(await screen.findByText("settingsDatabaseSaved")).toBeInTheDocument();
  });

  it("edits without replacing the stored password", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
    vi.mocked(desktop.saveDatabaseConnection).mockResolvedValue(connection);

    render(<DatabaseConnectionsPanel />);

    fireEvent.click(await screen.findByRole("button", { name: "settingsDatabaseEdit" }));
    expect(screen.getByLabelText("settingsDatabasePassword")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "settingsDatabaseSave" }));

    await waitFor(() =>
      expect(desktop.saveDatabaseConnection).toHaveBeenCalledWith(
        expect.objectContaining({ id: "db-1", password: undefined }),
      ),
    );
  });

  it("shows the server version after a successful test", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
    vi.mocked(desktop.testDatabaseConnection).mockResolvedValue(
      "Microsoft SQL Server 2022 (RTP)\nCopyright",
    );

    render(<DatabaseConnectionsPanel />);

    fireEvent.click(await screen.findByRole("button", { name: "settingsDatabaseTest" }));

    expect(await screen.findByText(/Microsoft SQL Server 2022/)).toBeInTheDocument();
    expect(desktop.testDatabaseConnection).toHaveBeenCalledWith("db-1");
  });

  it("shows the error when a test fails", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
    vi.mocked(desktop.testDatabaseConnection).mockRejectedValue(
      new Error("cannot reach 192.168.1.10:1433"),
    );

    render(<DatabaseConnectionsPanel />);

    fireEvent.click(await screen.findByRole("button", { name: "settingsDatabaseTest" }));

    expect(await screen.findByText(/cannot reach 192\.168\.1\.10/)).toBeInTheDocument();
  });

  it("loads the table list on demand", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
    vi.mocked(desktop.listDatabaseTables).mockResolvedValue(["dbo.HeatRecords", "dbo.Chemistry"]);

    render(<DatabaseConnectionsPanel />);

    fireEvent.click(await screen.findByRole("button", { name: "settingsDatabaseTables" }));

    expect(await screen.findByText("dbo.HeatRecords")).toBeInTheDocument();
    expect(screen.getByText("dbo.Chemistry")).toBeInTheDocument();
  });

  it("deletes a connection after confirmation", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
    vi.mocked(desktop.deleteDatabaseConnection).mockResolvedValue(undefined);
    window.confirm = vi.fn(() => true);

    render(<DatabaseConnectionsPanel />);

    fireEvent.click(await screen.findByRole("button", { name: "settingsDatabaseDelete" }));

    await waitFor(() => expect(desktop.deleteDatabaseConnection).toHaveBeenCalledWith("db-1"));
    expect(await screen.findByText("settingsDatabaseDeleted")).toBeInTheDocument();
  });

  it("shows a health badge from persisted connection health", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([
      {
        ...connection,
        last_checked_at: "2026-08-18T09:00:00+08:00",
        last_latency_ms: 120,
        last_version: "Microsoft SQL Server 2022",
        last_error: null,
      },
    ]);

    render(<DatabaseConnectionsPanel />);

    expect(await screen.findByText(/Microsoft SQL Server 2022/)).toBeInTheDocument();
    expect(screen.getByText(/settingsDatabaseLatency/)).toBeInTheDocument();
  });

  it("shows the last failure reason from persisted health", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([
      {
        ...connection,
        last_checked_at: "2026-08-18T09:00:00+08:00",
        last_latency_ms: null,
        last_version: null,
        last_error: "SQL Server login failed",
      },
    ]);

    render(<DatabaseConnectionsPanel />);

    expect(await screen.findByText(/SQL Server login failed/)).toBeInTheDocument();
  });

  it("toggles a connection enabled state", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
    vi.mocked(desktop.saveDatabaseConnection).mockResolvedValue(connection);

    render(<DatabaseConnectionsPanel />);

    fireEvent.click(await screen.findByRole("switch", { name: "settingsDatabaseEnabled" }));

    await waitFor(() =>
      expect(desktop.saveDatabaseConnection).toHaveBeenCalledWith(
        expect.objectContaining({ id: "db-1", enabled: false }),
      ),
    );
  });

  it("edits the timeout and warns about duplicates", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);

    render(<DatabaseConnectionsPanel />);

    fireEvent.change(await screen.findByLabelText("settingsDatabaseTimeout"), {
      target: { value: "20000" },
    });
    fireEvent.change(screen.getByLabelText("settingsDatabaseHost"), {
      target: { value: connection.host },
    });
    fireEvent.change(screen.getByLabelText("settingsDatabasePort"), {
      target: { value: String(connection.port) },
    });
    fireEvent.change(screen.getByLabelText("settingsDatabaseUsername"), {
      target: { value: connection.username },
    });
    fireEvent.change(screen.getByLabelText("settingsDatabaseDisplayName"), {
      target: { value: "重复检查" },
    });

    expect(await screen.findByText("settingsDatabaseDuplicate")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "settingsDatabaseSave" }));

    await waitFor(() =>
      expect(desktop.saveDatabaseConnection).toHaveBeenCalledWith(
        expect.objectContaining({ timeout_ms: 20000 }),
      ),
    );
  });
});
