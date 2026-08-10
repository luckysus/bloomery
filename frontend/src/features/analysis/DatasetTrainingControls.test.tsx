import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DatasetTrainingControls from "./DatasetTrainingControls";
import { desktop, type SteelDatasetRecord } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    trainSteelDataset: vi.fn(),
    listBackgroundTasks: vi.fn(),
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
    }));
    expect(await screen.findByTestId("training-task-dataset-1")).toHaveTextContent("task-1");
  });
});
