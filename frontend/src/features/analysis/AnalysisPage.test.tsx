import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AnalysisPage from "./AnalysisPage";
import { desktop } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    calculateSteelCarbonEquivalent: vi.fn(),
  },
}));

describe("AnalysisPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
});
