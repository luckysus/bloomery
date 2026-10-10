import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop, type SteelModelRecord } from "../../bridge/desktop";
import ModelRegistryPanel, { modelCardMetrics, modelCardTitle } from "./ModelRegistryPanel";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    listAllSteelModels: vi.fn(),
    setActiveSteelModel: vi.fn(),
    deleteSteelModel: vi.fn(),
  },
}));

const model = (overrides: Partial<SteelModelRecord> = {}): SteelModelRecord => ({
  id: "model-1",
  lineage_id: "sklearn:dataset-1",
  kind: "sklearn_artifact",
  version: 2,
  source_task_id: "018f3c2a-0000-0000-0000-000000000000",
  model_sha256: "a".repeat(64),
  manifest_json: "{}",
  artifact_json: JSON.stringify({
    model_type: "xgboost",
    metrics: { validation: { sample_count: 20, mae: 1.5, rmse: 2.5, r2: 0.97 }, train: { sample_count: 80, mae: 0.5, rmse: 1.0, r2: 0.99 } },
  }),
  model_base64: null,
  is_active: false,
  created_at: "2026-10-10T08:30:00Z",
  ...overrides,
});

const datasets = [
  { id: "dataset-1", sourceName: "Q690 heats.csv" },
] as unknown as Parameters<typeof ModelRegistryPanel>[0]["datasets"];

function renderPanel() {
  return render(<ModelRegistryPanel datasets={datasets} onNotice={vi.fn()} onError={vi.fn()} />);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(desktop.listAllSteelModels).mockResolvedValue([]);
});

describe("ModelRegistryPanel", () => {
  it("renders model cards with dataset, task, version and validation metrics", async () => {
    vi.mocked(desktop.listAllSteelModels).mockResolvedValue([model()]);
    renderPanel();
    expect(await screen.findByText("XGBoost")).toBeInTheDocument();
    expect(screen.getByText("Q690 heats.csv")).toBeInTheDocument();
    expect(screen.getByText("v2 · sklearn_artifact")).toBeInTheDocument();
    const facts = screen.getByText("R²").nextElementSibling;
    expect(facts).toHaveTextContent("0.97");
    expect(screen.getByText("MAE").nextElementSibling).toHaveTextContent("1.5");
    expect(screen.getByText("RMSE").nextElementSibling).toHaveTextContent("2.5");
  });

  it("shows ONNX cards without artifact metrics", async () => {
    vi.mocked(desktop.listAllSteelModels).mockResolvedValue([
      model({
        kind: "onnx",
        artifact_json: null,
        manifest_json: JSON.stringify({ model_id: "custom-regr", model_version: "1.0.0" }),
      }),
    ]);
    renderPanel();
    expect(await screen.findByText(/ONNX 模型 custom-regr/)).toBeInTheDocument();
    const r2 = screen.getByText("R²").nextElementSibling;
    expect(r2).toHaveTextContent("-");
  });

  it("activates an inactive version through the bridge command", async () => {
    vi.mocked(desktop.listAllSteelModels).mockResolvedValue([model()]);
    vi.mocked(desktop.setActiveSteelModel).mockResolvedValue(model({ is_active: true }));
    renderPanel();
    const activate = await screen.findByRole("button", { name: /设为活动/ });
    fireEvent.click(activate);
    await waitFor(() => expect(vi.mocked(desktop.setActiveSteelModel)).toHaveBeenCalledWith("model-1"));
    await waitFor(() => expect(vi.mocked(desktop.listAllSteelModels)).toHaveBeenCalledTimes(2));
  });

  it("deletes an inactive version and reloads", async () => {
    vi.mocked(desktop.listAllSteelModels).mockResolvedValue([model()]);
    vi.mocked(desktop.deleteSteelModel).mockResolvedValue(undefined);
    renderPanel();
    fireEvent.click(await screen.findByRole("button", { name: /删除/ }));
    await waitFor(() => expect(vi.mocked(desktop.deleteSteelModel)).toHaveBeenCalledWith("model-1"));
    await waitFor(() => expect(vi.mocked(desktop.listAllSteelModels)).toHaveBeenCalledTimes(2));
  });

  it("disables mutation for the active version", async () => {
    vi.mocked(desktop.listAllSteelModels).mockResolvedValue([model({ is_active: true })]);
    renderPanel();
    expect(await screen.findByText(/活动版本/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /设为活动/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /删除/ })).toBeDisabled();
  });

  it("shows the empty state when no models are registered", async () => {
    renderPanel();
    expect(await screen.findByText("尚无已注册的模型")).toBeInTheDocument();
  });

  it("derives card titles and metrics from stored json", () => {
    expect(modelCardTitle(model())).toBe("XGBoost");
    expect(modelCardMetrics(model())).toEqual({ sample_count: 20, mae: 1.5, rmse: 2.5, r2: 0.97 });
    const onnx = model({ kind: "onnx", artifact_json: null, manifest_json: '{"model_id":"abc"}' });
    expect(modelCardTitle(onnx)).toBe("ONNX 模型 abc");
    expect(modelCardMetrics(onnx)).toBeNull();
  });
});
