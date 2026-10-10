import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ComputeOptimizationRecommendation } from "../../bridge/desktop";
import OptimizationParetoChart, { paretoChartData } from "./OptimizationParetoChart";

const entryToParams = (entry: ComputeOptimizationRecommendation) => ({
  objectives: entry.objectives,
  prediction: entry.prediction,
});

const recommendation = (objectives: number[]): ComputeOptimizationRecommendation => ({
  values: Object.fromEntries(objectives.map((value, index) => [`feature_${index}`, value])),
  objectives,
  prediction: objectives[0],
  feasible: true,
  constraint_residuals: {},
});

describe("OptimizationParetoChart", () => {
  it("derives the sorted 2D front line from candidate points", () => {
    const front = [recommendation([9, 1]), recommendation([1, 9]), recommendation([4, 5])];
    const { data, frontLine } = paretoChartData([
      ...front.map((entry, index) => ({ ...entryToParams(entry), onFront: true, label: `方案 ${index}` })),
      { ...entryToParams(recommendation([5, 8])), onFront: false, label: "候选" },
    ]);
    expect(data).toHaveLength(4);
    // 前沿折线按目标 0 升序，供 Recharts 画连线。
    expect(frontLine.map((point) => point.x)).toEqual([1, 4, 9]);
    expect(frontLine.every((point) => point.front)).toBe(true);
  });

  it("renders a rotatable 3D projection for three objectives", () => {
    const front = [recommendation([1, 9, 2]), recommendation([5, 5, 5]), recommendation([9, 1, 8])];
    const { container } = render(
      <OptimizationParetoChart
        front={front}
        recommendations={[...front, recommendation([2, 8, 6])]}
        objectives={["strength", "toughness", "cost"]}
      />,
    );
    const svg = container.querySelector("svg");
    expect(svg).not.toBeNull();
    expect(svg?.getAttribute("aria-label")).toContain("三维 Pareto");
    // 前沿解带方案标签，非前沿解只有圆点。
    expect(svg?.querySelectorAll("circle").length).toBe(4);
    expect(screen.getByText("方案 A")).toBeInTheDocument();
    expect(screen.getByText(/拖拽旋转视角/)).toBeInTheDocument();
  });

  it("falls back to a table hint for four objectives", () => {
    const front = [recommendation([1, 2, 3, 4])];
    render(
      <OptimizationParetoChart
        front={front}
        recommendations={front}
        objectives={["a", "b", "c", "d"]}
      />,
    );
    expect(screen.getByText(/暂不支持图形化/)).toBeInTheDocument();
  });
});
