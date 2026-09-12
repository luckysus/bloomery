import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { desktop } from "../../bridge/desktop";
import { useWorkbenchOverview } from "./useWorkbenchOverview";

vi.mock("../../bridge/desktop", () => ({ desktop: {
  listConversations: vi.fn().mockResolvedValue([]),
  listKnowledgeBases: vi.fn().mockResolvedValue([]),
  listBackgroundTasks: vi.fn().mockResolvedValue([]),
  getKnowledgeHealth: vi.fn().mockResolvedValue(null),
  listenSchedulerProgress: vi.fn(),
} }));

it("refreshes durable task and health snapshots after events and disposes the listener", async () => {
  const dispose = vi.fn();
  let notify!: Parameters<typeof desktop.listenSchedulerProgress>[0];
  vi.mocked(desktop.listenSchedulerProgress).mockImplementation(async (handler) => {
    notify = handler;
    return dispose;
  });
  const { result, unmount } = renderHook(() => useWorkbenchOverview(true));
  await waitFor(() => expect(result.current.loading).toBe(false));
  vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([{
    id: "task-1", kind: "parse", state: "completed", progress: 100, attempt: 1,
    error_code: null, cancel_requested: false, can_cancel: false, can_retry: false,
    created_at: "2026-09-12T00:00:00Z", updated_at: "2026-09-12T00:01:00Z",
  }]);
  act(() => notify({
    id: "task-1", kind: "parse", state: "running", progress: 10, attempt: 1,
    error_code: null, cancel_requested: false,
    created_at: "2026-09-12T00:00:00Z", updated_at: "2026-09-12T00:00:00Z",
  }));
  await waitFor(() => expect(result.current.backgroundTasks[0]?.state).toBe("completed"));
  expect(desktop.getKnowledgeHealth).toHaveBeenCalledTimes(2);
  unmount();
  expect(dispose).toHaveBeenCalledOnce();
});
