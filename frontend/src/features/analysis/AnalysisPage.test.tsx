import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AnalysisPage from "./AnalysisPage";
import { desktop } from "../../bridge/desktop";
import { open } from "@tauri-apps/plugin-dialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    calculateSteelCarbonEquivalent: vi.fn(),
    previewSteelDataset: vi.fn(),
  },
}));

describe("AnalysisPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(open).mockResolvedValue(null);
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
    vi.mocked(open).mockResolvedValue("F:\\data\\heats.csv");
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

    await waitFor(() => expect(open).toHaveBeenCalled());
    await waitFor(() => expect(desktop.previewSteelDataset).toHaveBeenCalledWith({ sourcePath: "F:\\data\\heats.csv" }));
    expect(await screen.findByText("heats.csv")).toBeInTheDocument();
    expect(screen.getByText("yield_strength")).toBeInTheDocument();
    expect(screen.getByText("355 - 355")).toBeInTheDocument();
  });
});
