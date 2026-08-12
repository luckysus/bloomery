import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DatasetTrainingControls from "./DatasetTrainingControls";
import { desktop, type SteelDatasetRecord } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    trainSteelDataset: vi.fn(),
    listBackgroundTasks: vi.fn(),
    getComputeTrainingResult: vi.fn(),
    predictSteelModel: vi.fn(),
    getComputePredictionResult: vi.fn(),
    optimizeSteelProcess: vi.fn(),
    getComputeOptimizationResult: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    retryBackgroundTask: vi.fn(),
  },
}));

const dataset = {
  id: "dataset-1",
  sourceName: "heats.csv",
  sourcePath: "F:\\data\\heats.csv",
  sourceSha256: "hash",
  format: "csv",
  selectedSheet: "CSV",
  rowCount: 4,
  columnCount: 3,
  truncated: false,
  mappingState: "ready",
  preview: {
    sourceName: "heats.csv",
    format: "csv",
    sheets: ["CSV"],
    selectedSheet: "CSV",
    rowCount: 4,
    columnCount: 3,
    truncated: false,
    columns: [],
    sampleRows: [],
    warnings: [],
  },
  columns: [
    { ordinal: 0, originalName: "temperature", duplicate: false, inferredType: "number", canonicalField: "temperature", unit: "C", nonEmptyCount: 4, missingCount: 0, invalidCount: 0, min: 10, max: 40 },
    { ordinal: 1, originalName: "carbon", duplicate: false, inferredType: "number", canonicalField: "carbon", unit: "%", nonEmptyCount: 4, missingCount: 0, invalidCount: 0, min: 0.1, max: 0.4 },
    { ordinal: 2, originalName: "strength", duplicate: false, inferredType: "number", canonicalField: "yield_strength", unit: "MPa", nonEmptyCount: 4, missingCount: 0, invalidCount: 0, min: 355, max: 420 },
  ],
  createdAt: "2026-08-10T00:00:00Z",
  updatedAt: "2026-08-10T00:00:00Z",
} satisfies SteelDatasetRecord;

describe("DatasetTrainingControls", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.trainSteelDataset).mockResolvedValue({
      id: "task-1",
      kind: "compute_train_linear_regression",
      state: "queued",
      progress: 0,
      attempt: 0,
      error_code: null,
      cancel_requested: false,
      can_cancel: true,
      can_retry: false,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:00Z",
    });
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
    vi.mocked(desktop.getComputeTrainingResult).mockResolvedValue(null);
    vi.mocked(desktop.cancelBackgroundTask).mockResolvedValue({
      id: "task-1",
      kind: "compute_train_linear_regression",
      state: "cancelled",
      progress: 32,
      attempt: 1,
      error_code: null,
      cancel_requested: false,
      can_cancel: false,
      can_retry: true,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:01Z",
    });
    vi.mocked(desktop.retryBackgroundTask).mockResolvedValue({
      id: "task-1",
      kind: "compute_train_linear_regression",
      state: "queued",
      progress: 32,
      attempt: 2,
      error_code: null,
      cancel_requested: false,
      can_cancel: true,
      can_retry: false,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:01Z",
    });
  });

  it("queues a selected target and feature set for a ready dataset", async () => {
    render(<DatasetTrainingControls dataset={dataset} />);

    fireEvent.change(screen.getByTestId("training-target-dataset-1"), { target: { value: "2" } });
    fireEvent.click(screen.getByTestId("training-feature-dataset-1-1"));
    fireEvent.click(screen.getByRole("button", { name: /train|训练/i }));

    await waitFor(() => expect(desktop.trainSteelDataset).toHaveBeenCalledWith({
      datasetId: "dataset-1",
      targetColumn: 2,
      featureColumns: [0],
      splitPolicy: { kind: "random", validationFraction: 0.2, seed: 0 },
      algorithm: "linear_regression",
    }));
    expect(await screen.findByTestId("training-task-dataset-1")).toHaveTextContent("task-1");
  });

  it("blocks training when the source dataset was truncated", () => {
    render(<DatasetTrainingControls dataset={{ ...dataset, truncated: true }} />);

    expect(screen.getByTestId("training-truncated-dataset-1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /train|训练/i })).toBeDisabled();
    expect(desktop.trainSteelDataset).not.toHaveBeenCalled();
  });

  it("submits the selected non-linear training algorithm", async () => {
    render(<DatasetTrainingControls dataset={dataset} />);

    fireEvent.change(screen.getByTestId("training-algorithm-dataset-1"), { target: { value: "random_forest" } });
    fireEvent.click(screen.getByRole("button", { name: /train|开始训练/i }));

    await waitFor(() => expect(desktop.trainSteelDataset).toHaveBeenCalledWith({
      datasetId: "dataset-1",
      targetColumn: 2,
      featureColumns: [0, 1],
      splitPolicy: { kind: "random", validationFraction: 0.2, seed: 0 },
      algorithm: "random_forest",
    }));
  });

  it("refreshes a queued task and renders its completed model result", async () => {
    const queued = {
      id: "task-1",
      kind: "compute_train_linear_regression",
      state: "queued" as const,
      progress: 0,
      attempt: 0,
      error_code: null,
      cancel_requested: false,
      can_cancel: true,
      can_retry: false,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:00Z",
    };
    const running = { ...queued, state: "running" as const, progress: 32, attempt: 1 };
    const completed = { ...running, state: "completed" as const, progress: 100, can_cancel: false };
    vi.mocked(desktop.listBackgroundTasks)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([running])
      .mockResolvedValue([completed]);
    vi.mocked(desktop.getComputeTrainingResult).mockResolvedValue({
      task_id: "task-1",
      state: "completed",
      artifact: {
        model_id: "linear-task-1",
        model_type: "linear_regression",
        feature_names: ["temperature"],
        metrics: { rmse: 1.25, r2: 0.98 },
        applicability_range: [{ min: 10, max: 40 }],
      },
    });

    render(<DatasetTrainingControls dataset={dataset} />);
    fireEvent.click(screen.getByRole("button"));

    await screen.findByTestId("training-task-dataset-1");
    expect(await screen.findByTestId("training-result-dataset-1", {}, { timeout: 3000 })).toHaveTextContent("linear-task-1");
    expect(screen.getByTestId("training-task-dataset-1")).toHaveTextContent("100%");
    expect(screen.getByTestId("training-metric-rmse-dataset-1")).toHaveTextContent("1.25");
    expect(desktop.getComputeTrainingResult).toHaveBeenCalledWith("task-1");
  });

  it("restores a completed sklearn training task", async () => {
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([{
      id: "task-sklearn",
      kind: "compute_train_sklearn_model",
      dataset_id: "dataset-1",
      state: "completed",
      progress: 100,
      attempt: 1,
      error_code: null,
      cancel_requested: false,
      can_cancel: false,
      can_retry: false,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:01Z",
    }]);
    vi.mocked(desktop.getComputeTrainingResult).mockResolvedValue({
      task_id: "task-sklearn",
      state: "completed",
      artifact: {
        model_id: "forest-task-sklearn",
        model_type: "random_forest",
        feature_names: ["temperature", "carbon"],
        metrics: { rmse: 2.1 },
        applicability_range: [{ min: 10, max: 40 }, { min: 0.1, max: 0.4 }],
      },
    });

    render(<DatasetTrainingControls dataset={dataset} />);

    expect(await screen.findByTestId("training-result-dataset-1")).toHaveTextContent("forest-task-sklearn");
    expect(desktop.getComputeTrainingResult).toHaveBeenCalledWith("task-sklearn");
  });

  it("cancels an active training task and shows the terminal state", async () => {
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValueOnce([]).mockResolvedValue([
      {
        id: "task-1",
        kind: "compute_train_linear_regression",
        state: "running",
        progress: 32,
        attempt: 1,
        error_code: null,
        cancel_requested: false,
        can_cancel: true,
        can_retry: false,
        created_at: "2026-08-10T00:00:00Z",
        updated_at: "2026-08-10T00:00:00Z",
      },
    ]);

    render(<DatasetTrainingControls dataset={dataset} />);
    fireEvent.click(screen.getByRole("button"));
    await screen.findByTestId("training-cancel-dataset-1");
    fireEvent.click(screen.getByTestId("training-cancel-dataset-1"));

    await waitFor(() => expect(desktop.cancelBackgroundTask).toHaveBeenCalledWith("task-1"));
    expect(await screen.findByTestId("training-task-dataset-1")).toHaveTextContent(/cancelled|已取消/i);
  });
});
