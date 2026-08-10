import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import OptimizationPanel from "./OptimizationPanel";
import { desktop, type ComputeTrainingResult } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    optimizeSteelProcess: vi.fn(),
    listBackgroundTasks: vi.fn(),
    getComputeOptimizationResult: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    retryBackgroundTask: vi.fn(),
  },
}));

const trainingResult = {
  task_id: "training-1",
  state: "completed",
  artifact: {
    model_id: "model-1",
    model_type: "linear_regression",
    feature_names: ["temperature", "carbon"],
    metrics: { rmse: 1.2 },
    applicability_range: [
      { min: 10, max: 40 },
      { min: 0.1, max: 0.4 },
    ],
  },
} satisfies ComputeTrainingResult;

describe("OptimizationPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
    vi.mocked(desktop.optimizeSteelProcess).mockResolvedValue({
      id: "optimization-1",
      kind: "compute_optimize_constrained",
      dataset_id: "dataset-1",
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
  });

  it("submits bounds, objectives, and an enabled linear constraint", async () => {
    render(<OptimizationPanel datasetId="dataset-1" trainingResult={trainingResult} />);

    fireEvent.click(screen.getByTestId("optimization-constraint-toggle-dataset-1"));
    fireEvent.change(screen.getByTestId("optimization-constraint-coefficient-dataset-1-0"), { target: { value: "1" } });
    fireEvent.change(screen.getByTestId("optimization-constraint-value-dataset-1"), { target: { value: "25" } });
    fireEvent.click(screen.getByTestId("optimization-start-dataset-1"));

    await waitFor(() => expect(desktop.optimizeSteelProcess).toHaveBeenCalledWith({
      datasetId: "dataset-1",
      trainingTaskId: "training-1",
      direction: "minimize",
      objectiveColumns: [0],
      bounds: [{ min: 10, max: 40 }, { min: 0.1, max: 0.4 }],
      fixedValues: [null, null],
      constraints: [{
        kind: "inequality",
        coefficients: [1, 0],
        value: 25,
        tolerance: 0.000001,
      }],
      trials: 48,
      seed: 0,
    }));
    expect(await screen.findByTestId("optimization-task-dataset-1")).toHaveTextContent("optimization-1");
  });

  it("restores a completed optimization task and renders recommendations", async () => {
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([{
      id: "optimization-1",
      kind: "compute_optimize_constrained",
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
    vi.mocked(desktop.getComputeOptimizationResult).mockResolvedValue({
      task_id: "optimization-1",
      state: "completed",
      method: "tpe",
      direction: "minimize",
      objectives: ["temperature"],
      feature_names: ["temperature", "carbon"],
      model_id: "model-1",
      model_type: "linear_regression",
      trials_completed: 48,
      deterministic_seed: 7,
      recommendations: [{
        values: { temperature: 25.0001, carbon: 0.2 },
        objectives: [91.5],
        prediction: 91.5,
        feasible: true,
        constraint_residuals: {},
      }],
    });

    render(<OptimizationPanel datasetId="dataset-1" trainingResult={trainingResult} />);

    expect(await screen.findByTestId("optimization-result-dataset-1")).toHaveTextContent("tpe");
    expect(screen.getByTestId("optimization-recommendation-values-dataset-1-0")).toHaveTextContent("temperature=25.0001");
    expect(desktop.getComputeOptimizationResult).toHaveBeenCalledWith("optimization-1");
  });

  it("rejects submission without any selected objective", async () => {
    render(<OptimizationPanel datasetId="dataset-1" trainingResult={trainingResult} />);

    fireEvent.click(screen.getByTestId("optimization-objective-dataset-1-0"));
    fireEvent.click(screen.getByTestId("optimization-start-dataset-1"));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(desktop.optimizeSteelProcess).not.toHaveBeenCalled();
  });

  it("rejects inverted bounds with a validation error", async () => {
    render(<OptimizationPanel datasetId="dataset-1" trainingResult={trainingResult} />);

    fireEvent.change(screen.getByTestId("optimization-bound-min-dataset-1-0"), { target: { value: "50" } });
    fireEvent.click(screen.getByTestId("optimization-start-dataset-1"));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(desktop.optimizeSteelProcess).not.toHaveBeenCalled();
  });

  it("cancels an active optimization task", async () => {
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([{
      id: "optimization-1",
      kind: "compute_optimize_constrained",
      dataset_id: "dataset-1",
      state: "running",
      progress: 42,
      attempt: 1,
      error_code: null,
      cancel_requested: false,
      can_cancel: true,
      can_retry: false,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:00Z",
    }]);
    vi.mocked(desktop.cancelBackgroundTask).mockResolvedValue({
      id: "optimization-1",
      kind: "compute_optimize_constrained",
      dataset_id: "dataset-1",
      state: "cancelled",
      progress: 42,
      attempt: 1,
      error_code: null,
      cancel_requested: false,
      can_cancel: false,
      can_retry: true,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:01Z",
    });

    render(<OptimizationPanel datasetId="dataset-1" trainingResult={trainingResult} />);

    fireEvent.click(await screen.findByTestId("optimization-cancel-dataset-1"));

    await waitFor(() => expect(desktop.cancelBackgroundTask).toHaveBeenCalledWith("optimization-1"));
    expect(await screen.findByTestId("optimization-task-dataset-1")).toHaveTextContent(/cancelled|已取消/i);
  });
});
