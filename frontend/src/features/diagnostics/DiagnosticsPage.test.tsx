import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DiagnosticsPage from "./DiagnosticsPage";
import { desktop, type BackgroundTask } from "../../bridge/desktop";

vi.mock("../../i18n/locale", () => ({
  useLocale: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}));

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    getStorageHealth: vi.fn(),
    getIndexHealth: vi.fn(),
    getSetting: vi.fn(),
    setSetting: vi.fn(),
    installBundledSteelPackage: vi.fn(),
    listProviderProfiles: vi.fn(),
    listBackgroundTasks: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    retryBackgroundTask: vi.fn(),
    exportDiagnostics: vi.fn(),
    writeDiagnosticsExport: vi.fn(),
    openFileDialog: vi.fn(),
    saveFileDialog: vi.fn(),
    createBackup: vi.fn(),
    previewBackup: vi.fn(),
    restoreBackup: vi.fn(),
  },
}));

const task: BackgroundTask = {
  id: "task-1",
  kind: "mineru_parse",
  state: "failed",
  progress: 58,
  attempt: 2,
  error_code: "provider_timeout",
  cancel_requested: false,
  can_cancel: false,
  can_retry: true,
  created_at: "2026-08-06T10:00:00Z",
  updated_at: "2026-08-06T10:01:00Z",
};

describe("DiagnosticsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.getStorageHealth).mockResolvedValue({
      database_ok: true,
      current_migration_version: 7,
      latest_migration_version: 7,
      database_size_bytes: 2_048,
      reclaimable_bytes: 512,
      available_disk_bytes: 50 * 1024 * 1024 * 1024,
    });
    vi.mocked(desktop.getSetting).mockResolvedValue(JSON.stringify({
      embedding_profile_id: "embedding-1",
      reranker_profile_id: "reranker-1",
    }));
    vi.mocked(desktop.listProviderProfiles).mockResolvedValue([{
      id: "embedding-1",
      kind: "siliconflow",
      display_name: "BGE-M3",
      base_url: "https://api.siliconflow.cn/v1",
      model_id: "BAAI/bge-m3",
      enabled: true,
      revision: 1,
      secret_generation: 1,
      secret_configured: true,
    }]);
    vi.mocked(desktop.getIndexHealth).mockResolvedValue({
      state: "healthy",
      reason: null,
      serving_mode: "hnsw",
      chunk_count: 42,
      required_rebuild_bytes: 1024,
      available_disk_bytes: 50 * 1024 * 1024 * 1024,
      stale_temporary_count: 0,
      rebuild_task_id: null,
    });
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([task]);
    vi.mocked(desktop.retryBackgroundTask).mockResolvedValue({ ...task, state: "queued", can_retry: false });
    vi.mocked(desktop.exportDiagnostics).mockResolvedValue({
      privacy: { contains_provider_secret: false, contains_message_content: false },
    });
    vi.mocked(desktop.writeDiagnosticsExport).mockResolvedValue(undefined);
    vi.mocked(desktop.createBackup).mockResolvedValue({
      format_version: 1,
      archive_path: "C:\\Backups\\steel.bloomery-backup",
      database_bytes: 1024,
      content_file_count: 2,
      content_bytes: 2048,
    });
    vi.mocked(desktop.previewBackup).mockResolvedValue({
      format_version: 1,
      archive_path: "C:\\Backups\\steel.bloomery-backup",
      database_bytes: 1024,
      content_file_count: 2,
      content_bytes: 2048,
    });
    vi.mocked(desktop.restoreBackup).mockResolvedValue({
      format_version: 1,
      archive_path: "C:\\Backups\\steel.bloomery-backup",
      database_bytes: 1024,
      content_file_count: 2,
      content_bytes: 2048,
    });
    vi.mocked(desktop.saveFileDialog).mockResolvedValue(null);
    vi.mocked(desktop.openFileDialog).mockResolvedValue(null);
  });

  it("shows database, index, and task health from local diagnostics", async () => {
    render(<DiagnosticsPage />);

    expect(await screen.findByRole("heading", { name: "diagnosticsTitle" })).toBeInTheDocument();
    expect(screen.getByText("diagnosticsDatabaseHealthy")).toBeInTheDocument();
    expect(screen.getByText("diagnosticsIndexHealthy")).toBeInTheDocument();
    expect(screen.getByText("42")).toBeInTheDocument();
    expect(screen.getByText("provider_timeout")).toBeInTheDocument();
  });

  it("shows and repairs a bundled steel package initialization failure", async () => {
    vi.mocked(desktop.getSetting).mockImplementation(async (key) => {
      if (key === "onboarding.completed") {
        return JSON.stringify({
          version: 1,
          completed: true,
          steel_package_status: "error",
          steel_package_error: "bundled resource is missing",
        });
      }
      return JSON.stringify({
        embedding_profile_id: "embedding-1",
        reranker_profile_id: "reranker-1",
      });
    });
    vi.mocked(desktop.installBundledSteelPackage).mockResolvedValue({} as never);

    render(<DiagnosticsPage />);

    expect(await screen.findByText("bundled resource is missing")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "diagnosticsRetrySteelPackage" }));

    await waitFor(() => expect(desktop.installBundledSteelPackage).toHaveBeenCalledOnce());
    expect(desktop.setSetting).toHaveBeenCalledWith(
      "onboarding.completed",
      expect.stringContaining('"steel_package_status":"ready"'),
    );
    expect(await screen.findByText("diagnosticsSteelPackageRepaired")).toBeInTheDocument();
  });

  it("does not report a repair when the follow-up diagnostics refresh fails", async () => {
    vi.mocked(desktop.getSetting).mockImplementation(async (key) => {
      if (key === "onboarding.completed") {
        return JSON.stringify({
          version: 1,
          completed: true,
          steel_package_status: "error",
          steel_package_error: "bundled resource is missing",
        });
      }
      return JSON.stringify({
        embedding_profile_id: "embedding-1",
        reranker_profile_id: "reranker-1",
      });
    });
    vi.mocked(desktop.installBundledSteelPackage).mockResolvedValue({} as never);
    vi.mocked(desktop.getStorageHealth)
      .mockResolvedValueOnce({
        database_ok: true,
        current_migration_version: 7,
        latest_migration_version: 7,
        database_size_bytes: 2_048,
        reclaimable_bytes: 512,
        available_disk_bytes: 50 * 1024 * 1024 * 1024,
      })
      .mockRejectedValueOnce(new Error("diagnostics unavailable"));

    render(<DiagnosticsPage />);

    fireEvent.click(await screen.findByRole("button", { name: "diagnosticsRetrySteelPackage" }));

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("diagnostics unavailable"));
    expect(screen.queryByText("diagnosticsSteelPackageRepaired")).not.toBeInTheDocument();
  });

  it("preserves completion when repairing a package from an incomplete setting", async () => {
    vi.mocked(desktop.getSetting).mockImplementation(async (key) => {
      if (key === "onboarding.completed") {
        return JSON.stringify({ steel_package_status: "error", steel_package_error: "bundled resource is missing" });
      }
      return JSON.stringify({
        embedding_profile_id: "embedding-1",
        reranker_profile_id: "reranker-1",
      });
    });
    vi.mocked(desktop.installBundledSteelPackage).mockResolvedValue({} as never);

    render(<DiagnosticsPage />);

    fireEvent.click(await screen.findByRole("button", { name: "diagnosticsRetrySteelPackage" }));

    await waitFor(() => expect(desktop.setSetting).toHaveBeenCalledWith(
      "onboarding.completed",
      expect.stringContaining('"completed":true'),
    ));
  });

  it("retries a failed task through the task bridge", async () => {
    render(<DiagnosticsPage />);

    const retry = await screen.findByRole("button", { name: "diagnosticsRetryTask" });
    fireEvent.click(retry);

    await waitFor(() => expect(desktop.retryBackgroundTask).toHaveBeenCalledWith(task.id));
  });

  it("exports diagnostic metadata without including provider secrets", async () => {
    vi.mocked(desktop.saveFileDialog).mockResolvedValue("C:\\Exports\\bloomery-diagnostics.json");
    render(<DiagnosticsPage />);

    fireEvent.click(await screen.findByRole("button", { name: "diagnosticsExport" }));

    await waitFor(() => expect(desktop.writeDiagnosticsExport).toHaveBeenCalledWith("C:\\Exports\\bloomery-diagnostics.json"));
    expect(desktop.exportDiagnostics).not.toHaveBeenCalled();
    expect(await screen.findByText("diagnosticsExported")).toBeInTheDocument();
  });

  it("creates a local backup at the path selected by the user", async () => {
    vi.mocked(desktop.saveFileDialog).mockResolvedValue("C:\\Backups\\steel.bloomery-backup");
    render(<DiagnosticsPage />);

    fireEvent.click(await screen.findByRole("button", { name: "diagnosticsBackupExport" }));

    await waitFor(() => expect(desktop.createBackup).toHaveBeenCalledWith("C:\\Backups\\steel.bloomery-backup"));
    expect(await screen.findByText("diagnosticsBackupCreated")).toBeInTheDocument();
  });

  it("restores a selected backup only after explicit confirmation", async () => {
    vi.mocked(desktop.openFileDialog).mockResolvedValue("C:\\Backups\\steel.bloomery-backup");
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<DiagnosticsPage />);

    fireEvent.click(await screen.findByRole("button", { name: "diagnosticsBackupRestore" }));

    await waitFor(() => expect(desktop.restoreBackup).toHaveBeenCalledWith("C:\\Backups\\steel.bloomery-backup"));
    expect(desktop.previewBackup).toHaveBeenCalledWith("C:\\Backups\\steel.bloomery-backup");
    expect(confirm).toHaveBeenCalled();
    expect(await screen.findByText("diagnosticsBackupRestored")).toBeInTheDocument();
  });

  it("does not restore when the user rejects the validated backup preview", async () => {
    vi.mocked(desktop.openFileDialog).mockResolvedValue("C:\\Backups\\steel.bloomery-backup");
    vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<DiagnosticsPage />);

    fireEvent.click(await screen.findByRole("button", { name: "diagnosticsBackupRestore" }));

    await waitFor(() => expect(desktop.previewBackup).toHaveBeenCalledWith("C:\\Backups\\steel.bloomery-backup"));
    expect(desktop.restoreBackup).not.toHaveBeenCalled();
  });
});
