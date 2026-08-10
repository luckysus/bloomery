import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import OnnxInferencePanel from "./OnnxInferencePanel";
import { desktop, type BackgroundTask, type ComputeOnnxPredictionResult } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    openFileDialog: vi.fn(),
    hashOnnxModelFile: vi.fn(),
    predictOnnxModel: vi.fn(),
    listBackgroundTasks: vi.fn(),
    getComputeOnnxPredictionResult: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    retryBackgroundTask: vi.fn(),
  },
}));

const queuedTask = {
  id: "onnx-task-1",
  kind: "compute_predict_onnx",
  state: "running",
  progress: 10,
  attempt: 0,
  error_code: null,
  cancel_requested: false,
  can_cancel: true,
  can_retry: false,
  created_at: "2026-08-10T00:00:00Z",
  updated_at: "2026-08-10T00:00:00Z",
} satisfies BackgroundTask;

const completedTask = { ...queuedTask, state: "completed", progress: 100, can_cancel: false } satisfies BackgroundTask;

const onnxResult = {
  task_id: "onnx-task-1",
  state: "completed",
  model_id: "mul-model",
  model_version: "1.0.0",
  model_sha256: "a".repeat(64),
  opset_version: 7,
  operators: ["Mul"],
  input_schema: [{ name: "X", type: "tensor(float)", shape: [null, 2] }],
  output_schema: [{ name: "Y", type: "tensor(float)", shape: [null, 2] }],
  preprocessing: { feature_names: ["temperature", "carbon"], means: [0, 0], scales: [1, 1] },
  normalized_inputs: [[1, 2]],
  predictions: [[1, 4]],
  outputs: { Y: [[1, 4]] },
  applicability_warnings: [
    { row: 0, feature: "carbon", index: 1, value: 2, min: 0, max: 1, code: "outside_applicability_range" },
  ],
  confidence: [0.5],
  constraints: [],
} satisfies ComputeOnnxPredictionResult;

const fillManifest = (value: string) => {
  fireEvent.change(screen.getByTestId("onnx-manifest"), { target: { value } });
};

const minimalManifest = JSON.stringify({
  model_id: "mul-model",
  model_version: "1.0.0",
  inputs: [{ name: "X", dtype: "float32", shape: [-1, 2] }],
  outputs: [{ name: "Y", dtype: "float32", shape: [-1, 2] }],
  preprocessing: { feature_names: ["temperature", "carbon"], means: [0, 0], scales: [1, 1] },
});

async function selectModel() {
  vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\models\\mul.onnx");
  vi.mocked(desktop.hashOnnxModelFile).mockResolvedValue("a".repeat(64));
  fireEvent.click(screen.getByTestId("onnx-pick-model"));
  await waitFor(() => expect(desktop.hashOnnxModelFile).toHaveBeenCalledWith("F:\\models\\mul.onnx"));
  await screen.findByTestId("onnx-model-path");
}

describe("OnnxInferencePanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
    vi.mocked(desktop.predictOnnxModel).mockResolvedValue(queuedTask);
  });

  it("pins the selected model file with its sha256 before inference", async () => {
    render(<OnnxInferencePanel />);

    await selectModel();

    expect(screen.getByTestId("onnx-model-path")).toHaveTextContent("mul.onnx");
    expect(screen.getByTestId("onnx-model-hash")).toHaveTextContent("aaaaaaaaaaaa");
  });

  it("submits the manifest and feature matrix through the desktop bridge", async () => {
    render(<OnnxInferencePanel />);
    await selectModel();
    fillManifest(minimalManifest);
    fireEvent.change(screen.getByTestId("onnx-features"), { target: { value: "1, 2" } });

    fireEvent.click(screen.getByTestId("onnx-start"));

    await waitFor(() => expect(desktop.predictOnnxModel).toHaveBeenCalledWith({
      modelPath: "F:\\models\\mul.onnx",
      modelSha256: "a".repeat(64),
      manifest: JSON.parse(minimalManifest),
      features: [[1, 2]],
    }));
    expect(await screen.findByTestId("onnx-task")).toBeInTheDocument();
  });

  it("renders predictions with confidence and applicability warnings", async () => {
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([completedTask]);
    vi.mocked(desktop.getComputeOnnxPredictionResult).mockResolvedValue(onnxResult);
    render(<OnnxInferencePanel />);

    const result = await screen.findByTestId("onnx-result");
    expect(result).toHaveTextContent("mul-model / 1.0.0");
    expect(result).toHaveTextContent("Mul");
    expect(screen.getByTestId("onnx-predictions")).toHaveTextContent("1, 4");
    expect(result).toHaveTextContent("0.500");
    expect(screen.getByTestId("onnx-warning-0")).toBeInTheDocument();
  });

  it("rejects malformed feature rows without calling the bridge", async () => {
    render(<OnnxInferencePanel />);
    await selectModel();
    fillManifest(minimalManifest);
    fireEvent.change(screen.getByTestId("onnx-features"), { target: { value: "1, x" } });

    fireEvent.click(screen.getByTestId("onnx-start"));

    await waitFor(() => expect(desktop.predictOnnxModel).not.toHaveBeenCalled());
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("rejects an invalid manifest before submitting", async () => {
    render(<OnnxInferencePanel />);
    await selectModel();
    fillManifest("{not-json");
    fireEvent.change(screen.getByTestId("onnx-features"), { target: { value: "1, 2" } });

    fireEvent.click(screen.getByTestId("onnx-start"));

    await waitFor(() => expect(desktop.predictOnnxModel).not.toHaveBeenCalled());
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
