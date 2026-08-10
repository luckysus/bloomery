import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AnalysisPage from "./AnalysisPage";
import { desktop, type DatasetPreview, type SteelDatasetRecord } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    calculateSteelCarbonEquivalent: vi.fn(),
    openFileDialog: vi.fn(),
    previewSteelDataset: vi.fn(),
    listSteelDatasets: vi.fn(),
    saveSteelDataset: vi.fn(),
    activateSteelDataset: vi.fn(),
    analyzeSteelDataset: vi.fn(),
    listBackgroundTasks: vi.fn(),
    getComputeTrainingResult: vi.fn(),
    hashOnnxModelFile: vi.fn(),
    predictOnnxModel: vi.fn(),
    getComputeOnnxPredictionResult: vi.fn(),
    optimizeSteelProcess: vi.fn(),
    getComputeOptimizationResult: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    retryBackgroundTask: vi.fn(),
  },
}));

describe("AnalysisPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.openFileDialog).mockResolvedValue(null);
    vi.mocked(desktop.listSteelDatasets).mockResolvedValue([]);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
    vi.mocked(desktop.getComputeTrainingResult).mockResolvedValue(null);
    vi.mocked(desktop.calculateSteelCarbonEquivalent).mockResolvedValue({
      formula_id: "carbon-equivalent.iiw.v1",
      expression: "C + Mn/6 + (Cr + Mo + V)/5 + (Ni + Cu)/15",
      normalized_inputs: { C: 0.2 },
      value: 0.464,
      unit: "percent_mass",
      applicability_note: "Confirm the applicable welding procedure.",
    });
  });

  it("calculates a steel carbon equivalent through the desktop bridge", async () => {
    render(<AnalysisPage />);

    expect(screen.getByTestId("analysis-page")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("C"), { target: { value: "0.20" } });
    fireEvent.click(screen.getByRole("button", { name: /calculate|计算/i }));

    await waitFor(() => expect(desktop.calculateSteelCarbonEquivalent).toHaveBeenCalled());
    expect(await screen.findByTestId("carbon-equivalent-value")).toHaveTextContent("0.464");
  });

  it("previews a production dataset through the native file picker", async () => {
    vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\data\\heats.csv");
    vi.mocked(desktop.previewSteelDataset).mockResolvedValue({
      sourceName: "heats.csv",
      format: "csv",
      sheets: ["CSV"],
      selectedSheet: "CSV",
      rowCount: 2,
      columnCount: 2,
      truncated: false,
      columns: [
        { name: "heat_id", duplicate: false, inferredType: "text", nonEmptyCount: 2, missingCount: 0, invalidCount: 0, min: null, max: null },
        { name: "yield_strength", duplicate: false, inferredType: "number", nonEmptyCount: 1, missingCount: 1, invalidCount: 0, min: 355, max: 355 },
      ],
      sampleRows: [["H-01", "355"], ["H-02", ""]],
      warnings: [],
    });
    render(<AnalysisPage />);

    fireEvent.click(screen.getByRole("button", { name: "选择数据文件" }));

    await waitFor(() => expect(desktop.openFileDialog).toHaveBeenCalled());
    await waitFor(() => expect(desktop.previewSteelDataset).toHaveBeenCalledWith({ sourcePath: "F:\\data\\heats.csv" }));
    expect(await screen.findByText("heats.csv")).toBeInTheDocument();
    expect(screen.getByText("yield_strength")).toBeInTheDocument();
    expect(screen.getByText("355 - 355")).toBeInTheDocument();
  });

  it("saves an explicit dataset selection and keeps it in the local catalog", async () => {
    vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\data\\heats.csv");
    const preview: DatasetPreview = {
      sourceName: "heats.csv",
      format: "csv",
      sheets: ["CSV"],
      selectedSheet: "CSV",
      rowCount: 2,
      columnCount: 2,
      truncated: false,
      columns: [
        { name: "heat_id", duplicate: false, inferredType: "text" as const, nonEmptyCount: 2, missingCount: 0, invalidCount: 0, min: null, max: null },
        { name: "yield_strength", duplicate: false, inferredType: "number" as const, nonEmptyCount: 2, missingCount: 0, invalidCount: 0, min: 355, max: 360 },
      ],
      sampleRows: [["H-01", "355"]],
      warnings: [],
    };
    vi.mocked(desktop.previewSteelDataset).mockResolvedValue(preview);
    vi.mocked(desktop.saveSteelDataset).mockResolvedValue({
      id: "dataset-1",
      sourceName: "heats.csv",
      sourcePath: "F:\\data\\heats.csv",
      sourceSha256: "hash",
      format: "csv",
      selectedSheet: "CSV",
      rowCount: 2,
      columnCount: 2,
      truncated: false,
      mappingState: "draft",
      preview,
      columns: [],
      createdAt: "2026-08-07T10:00:00Z",
      updatedAt: "2026-08-07T10:00:00Z",
    });

    render(<AnalysisPage />);
    fireEvent.click(screen.getByRole("button", { name: "选择数据文件" }));
    await screen.findByText("heats.csv");
    fireEvent.click(screen.getByRole("button", { name: "保存数据集" }));

    await waitFor(() => expect(desktop.saveSteelDataset).toHaveBeenCalledWith({
      sourcePath: "F:\\data\\heats.csv",
      sheet: "CSV",
      mappings: [],
    }));
    expect(await screen.findByText("数据集已保存")).toBeInTheDocument();
  });

  it("analyzes a saved dataset and shows traceable statistics", async () => {
    vi.mocked(desktop.listSteelDatasets).mockResolvedValue([
      {
        id: "dataset-1",
        sourceName: "heats.csv",
        sourcePath: "F:\\data\\heats.csv",
        sourceSha256: "hash",
        format: "csv",
        selectedSheet: "CSV",
        rowCount: 5,
        columnCount: 2,
        truncated: false,
        mappingState: "draft",
        preview: {
          sourceName: "heats.csv",
          format: "csv",
          sheets: ["CSV"],
          selectedSheet: "CSV",
          rowCount: 5,
          columnCount: 2,
          truncated: false,
          columns: [
            { name: "heat_id", duplicate: false, inferredType: "text", nonEmptyCount: 5, missingCount: 0, invalidCount: 0, min: null, max: null },
            { name: "temperature", duplicate: false, inferredType: "number", nonEmptyCount: 5, missingCount: 0, invalidCount: 0, min: 10, max: 100 },
          ],
          sampleRows: [],
          warnings: [],
        },
        columns: [
          { ordinal: 0, originalName: "heat_id", duplicate: false, inferredType: "text", canonicalField: "heat_id", unit: null, nonEmptyCount: 5, missingCount: 0, invalidCount: 0, min: null, max: null },
          { ordinal: 1, originalName: "temperature", duplicate: false, inferredType: "number", canonicalField: "temperature", unit: "C", nonEmptyCount: 5, missingCount: 0, invalidCount: 0, min: 10, max: 100 },
        ],
        createdAt: "2026-08-07T10:00:00Z",
        updatedAt: "2026-08-07T10:00:00Z",
      },
    ]);
    vi.mocked(desktop.analyzeSteelDataset).mockResolvedValue({
      datasetId: "dataset-1",
      sourceSha256: "hash",
      selectedSheet: "CSV",
      rowCount: 5,
      analyzedRowCount: 5,
      excludedRowCount: 0,
      columns: [
        {
          ordinal: 1,
          name: "temperature",
          canonicalField: "temperature",
          unit: "C",
          inferredType: "number",
          sampleCount: 5,
          missingCount: 0,
          invalidCount: 0,
          missingRate: 0,
          distinctCount: 5,
          mean: 29.2,
          standardDeviation: 35.6,
          min: 10,
          percentile25: 11,
          median: 12,
          percentile75: 13,
          max: 100,
          outlierCount: 1,
          outlierRows: [6],
          topValues: [],
          distribution: [
            { lowerBound: 10, upperBound: 55, count: 4 },
            { lowerBound: 55, upperBound: 100, count: 1 },
          ],
        },
      ],
      groups: [
        { key: "Q355B", rowCount: 5, columns: [{ ordinal: 1, sampleCount: 5, mean: 29.2, min: 10, max: 100 }] },
      ],
      correlations: [],
      warnings: [],
    });

    render(<AnalysisPage />);
    await screen.findByText("heats.csv");
    fireEvent.change(screen.getByTestId("dataset-group-by-dataset-1"), { target: { value: "0" } });
    fireEvent.click(screen.getByTestId("analyze-dataset-dataset-1"));

    await waitFor(() => expect(desktop.analyzeSteelDataset).toHaveBeenCalledWith({
      datasetId: "dataset-1",
      groupByColumn: 0,
    }));
    expect(await screen.findByTestId("dataset-analysis-result")).toBeInTheDocument();
    expect(screen.getByText("29.2")).toBeInTheDocument();
    expect(screen.getByText("6")).toBeInTheDocument();
    expect(screen.getByText("Q355B")).toBeInTheDocument();
    expect(screen.getByTestId("dataset-distribution-1")).toBeInTheDocument();
  });

  it("persists non-empty canonical field mappings with the dataset", async () => {
    vi.mocked(desktop.openFileDialog).mockResolvedValue("F:\\data\\heats.csv");
    const preview: DatasetPreview = {
      sourceName: "heats.csv",
      format: "csv",
      sheets: ["CSV"],
      selectedSheet: "CSV",
      rowCount: 1,
      columnCount: 2,
      truncated: false,
      columns: [
        { name: "heat_id", duplicate: false, inferredType: "text", nonEmptyCount: 1, missingCount: 0, invalidCount: 0, min: null, max: null },
        { name: "yield_strength", duplicate: false, inferredType: "number", nonEmptyCount: 1, missingCount: 0, invalidCount: 0, min: 355, max: 355 },
      ],
      sampleRows: [["H-01", "355"]],
      warnings: [],
    };
    vi.mocked(desktop.previewSteelDataset).mockResolvedValue(preview);
    vi.mocked(desktop.saveSteelDataset).mockResolvedValue({
      id: "dataset-1",
      sourceName: preview.sourceName,
      selectedSheet: preview.selectedSheet,
      rowCount: preview.rowCount,
      preview,
    } as never);

    render(<AnalysisPage />);
    fireEvent.click(screen.getByRole("button", { name: "选择数据文件" }));
    await screen.findByText("heats.csv");
    fireEvent.change(screen.getByTestId("dataset-mapping-1-canonical"), { target: { value: "yield_strength" } });
    fireEvent.change(screen.getByTestId("dataset-mapping-1-unit"), { target: { value: "MPa" } });
    fireEvent.click(screen.getByTestId("save-dataset"));

    await waitFor(() => expect(desktop.saveSteelDataset).toHaveBeenCalledWith({
      sourcePath: "F:\\data\\heats.csv",
      sheet: "CSV",
      mappings: [{ ordinal: 1, canonicalField: "yield_strength", unit: "MPa" }],
    }));
  });

  it("activates a mapped draft dataset and updates its status", async () => {
    const preview: DatasetPreview = {
      sourceName: "heats.csv",
      format: "csv",
      sheets: ["CSV"],
      selectedSheet: "CSV",
      rowCount: 1,
      columnCount: 1,
      truncated: false,
      columns: [{ name: "yield_strength", duplicate: false, inferredType: "number", nonEmptyCount: 1, missingCount: 0, invalidCount: 0, min: 355, max: 355 }],
      sampleRows: [["355"]],
      warnings: [],
    };
    const draft = {
      id: "dataset-1",
      sourceName: "heats.csv",
      sourcePath: "F:\\data\\heats.csv",
      sourceSha256: "hash",
      format: "csv",
      selectedSheet: "CSV",
      rowCount: 1,
      columnCount: 1,
      truncated: false,
      mappingState: "draft",
      preview,
      columns: [{ ordinal: 0, originalName: "yield_strength", duplicate: false, inferredType: "number", canonicalField: "yield_strength", unit: "MPa", nonEmptyCount: 1, missingCount: 0, invalidCount: 0, min: 355, max: 355 }],
      createdAt: "2026-08-07T10:00:00Z",
      updatedAt: "2026-08-07T10:00:00Z",
    } satisfies SteelDatasetRecord;
    vi.mocked(desktop.listSteelDatasets).mockResolvedValue([draft]);
    vi.mocked(desktop.activateSteelDataset).mockResolvedValue({ ...draft, mappingState: "ready" });

    render(<AnalysisPage />);
    await screen.findByText("heats.csv");
    expect(screen.getByTestId("dataset-status-dataset-1")).toHaveTextContent("待激活");
    fireEvent.click(screen.getByTestId("activate-dataset-dataset-1"));

    await waitFor(() => expect(desktop.activateSteelDataset).toHaveBeenCalledWith("dataset-1"));
    expect(await screen.findByTestId("dataset-status-dataset-1")).toHaveTextContent("已激活");
  });
});
