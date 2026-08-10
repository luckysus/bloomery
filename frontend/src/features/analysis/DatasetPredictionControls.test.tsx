import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DatasetPredictionControls from "./DatasetPredictionControls";
import { desktop, type ComputeTrainingResult } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    predictSteelModel: vi.fn(),
    listBackgroundTasks: vi.fn(),
    getComputePredictionResult: vi.fn(),
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

describe("DatasetPredictionControls", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
    vi.mocked(desktop.predictSteelModel).mockResolvedValue({
      id: "prediction-1",
      kind: "compute_predict_linear_regression",
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
    vi.mocked(desktop.cancelBackgroundTask).mockResolvedValue({
      id: "prediction-1",
      kind: "compute_predict_linear_regression",
      dataset_id: "dataset-1",
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
      id: "prediction-1",
      kind: "compute_predict_linear_regression",
      dataset_id: "dataset-1",
      state: "queued",
      progress: 32,
      attempt: 2,
      error_code: null,
      cancel_requested: false,
      can_cancel: true,
      can_retry: false,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:02Z",
    });
  });

  it("submits numeric feature values for a trained model", async () => {
    render(<DatasetPredictionControls datasetId="dataset-1" trainingResult={trainingResult} />);

    fireEvent.change(screen.getByTestId("prediction-input-0"), { target: { value: "125" } });
    fireEvent.change(screen.getByTestId("prediction-input-1"), { target: { value: "0.2" } });
    fireEvent.click(screen.getByRole("button", { name: /predict|推理/i }));

    await waitFor(() => expect(desktop.predictSteelModel).toHaveBeenCalledWith({
      datasetId: "dataset-1",
      trainingTaskId: "training-1",
      featureValues: [125, 0.2],
    }));
    expect(await screen.findByTestId("prediction-task")).toHaveTextContent("prediction-1");
  });

  it("restores a completed prediction and shows applicability warnings", async () => {
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([{
      id: "prediction-1",
      kind: "compute_predict_linear_regression",
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
    vi.mocked(desktop.getComputePredictionResult).mockResolvedValue({
      task_id: "prediction-1",
      state: "completed",
      model_id: "model-1",
      model_type: "linear_regression",
      feature_names: ["temperature", "carbon"],
      input_values: [125, 0.2],
      predictions: [91.5],
      applicability_range: trainingResult.artifact.applicability_range,
      applicability_warnings: [{
        code: "outside_applicability_range",
        feature: "temperature",
        index: 0,
        value: 125,
        min: 10,
        max: 40,
      }],
      confidence: null,
      constraints: [],
    });

    render(<DatasetPredictionControls datasetId="dataset-1" trainingResult={trainingResult} />);

    expect(await screen.findByTestId("prediction-result")).toHaveTextContent("91.5");
    expect(screen.getByTestId("prediction-warning-0")).toHaveTextContent("temperature");
    expect(desktop.getComputePredictionResult).toHaveBeenCalledWith("prediction-1");
  });

  it("shows a validation error instead of submitting incomplete features", async () => {
    render(<DatasetPredictionControls datasetId="dataset-1" trainingResult={trainingResult} />);

    fireEvent.click(screen.getByRole("button", { name: /推理/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("请输入有效的特征值");
    expect(desktop.predictSteelModel).not.toHaveBeenCalled();
  });

  it("cancels an active prediction task", async () => {
    const running = {
      id: "prediction-1",
      kind: "compute_predict_linear_regression",
      dataset_id: "dataset-1",
      state: "running" as const,
      progress: 32,
      attempt: 1,
      error_code: null,
      cancel_requested: false,
      can_cancel: true,
      can_retry: false,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:00Z",
    };
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([running]);

    render(<DatasetPredictionControls datasetId="dataset-1" trainingResult={trainingResult} />);

    fireEvent.click(await screen.findByTestId("prediction-cancel"));

    await waitFor(() => expect(desktop.cancelBackgroundTask).toHaveBeenCalledWith("prediction-1"));
    expect(await screen.findByTestId("prediction-task")).toHaveTextContent(/cancelled|已取消/i);
  });

  it("retries a failed prediction task", async () => {
    const failed = {
      id: "prediction-1",
      kind: "compute_predict_linear_regression",
      dataset_id: "dataset-1",
      state: "failed" as const,
      progress: 32,
      attempt: 1,
      error_code: "worker_failed",
      cancel_requested: false,
      can_cancel: false,
      can_retry: true,
      created_at: "2026-08-10T00:00:00Z",
      updated_at: "2026-08-10T00:00:01Z",
    };
    const queued = { ...failed, state: "queued" as const, attempt: 2, can_cancel: true, can_retry: false };
    vi.mocked(desktop.listBackgroundTasks)
      .mockResolvedValueOnce([failed])
      .mockResolvedValue([queued]);

    render(<DatasetPredictionControls datasetId="dataset-1" trainingResult={trainingResult} />);

    fireEvent.click(await screen.findByTestId("prediction-retry"));

    await waitFor(() => expect(desktop.retryBackgroundTask).toHaveBeenCalledWith("prediction-1"));
    expect(await screen.findByTestId("prediction-task")).toHaveTextContent(/queued|排队/i);
  });
});
