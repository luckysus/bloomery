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
    listProviderProfiles: vi.fn(),
    listBackgroundTasks: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    retryBackgroundTask: vi.fn(),
    exportDiagnostics: vi.fn(),
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
  });

  it("shows database, index, and task health from local diagnostics", async () => {
    render(<DiagnosticsPage />);

    expect(await screen.findByRole("heading", { name: "diagnosticsTitle" })).toBeInTheDocument();
    expect(screen.getByText("diagnosticsDatabaseHealthy")).toBeInTheDocument();
    expect(screen.getByText("diagnosticsIndexHealthy")).toBeInTheDocument();
    expect(screen.getByText("42")).toBeInTheDocument();
    expect(screen.getByText("provider_timeout")).toBeInTheDocument();
  });

  it("retries a failed task through the task bridge", async () => {
    render(<DiagnosticsPage />);

    const retry = await screen.findByRole("button", { name: "diagnosticsRetryTask" });
    fireEvent.click(retry);

    await waitFor(() => expect(desktop.retryBackgroundTask).toHaveBeenCalledWith(task.id));
  });

  it("exports diagnostic metadata without including provider secrets", async () => {
    render(<DiagnosticsPage />);

    fireEvent.click(await screen.findByRole("button", { name: "diagnosticsExport" }));

    await waitFor(() => expect(desktop.exportDiagnostics).toHaveBeenCalledWith(undefined));
    expect(await screen.findByText("diagnosticsExported")).toBeInTheDocument();
  });
});
