import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DatabaseConnectionsPanel from "./DatabaseConnectionsPanel";
import { desktop } from "../../bridge/desktop";

vi.mock("../../i18n/locale", () => ({
  useLocale: () => ({
    t: (key: string) => key,
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
});
