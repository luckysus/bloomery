import { describe, expect, it } from "vitest";
import { paretoFront } from "./ParetoFrontChart";

describe("paretoFront", () => {
  it("最大化两个目标时只保留非支配解", () => {
    const points = [
      { x: 1, y: 1 },
      { x: 2, y: 3 },
      { x: 3, y: 2 },
      { x: 1, y: 4 },
    ];
    const front = paretoFront(points, false, false);
    // (1,1) 被 (2,3) 与 (3,2) 支配；其余三点两两互不支配。
    expect(front).toEqual(expect.arrayContaining([{ x: 2, y: 3 }, { x: 3, y: 2 }, { x: 1, y: 4 }]));
    expect(front).toHaveLength(3);
    expect(front).not.toEqual(expect.arrayContaining([{ x: 1, y: 1 }]));
  });

  it("最小化两个目标时保留左下角前沿", () => {
    const points = [
      { x: 1, y: 5 },
      { x: 2, y: 4 },
      { x: 5, y: 1 },
      { x: 6, y: 6 },
    ];
    const front = paretoFront(points, true, true);
    expect(front).toEqual(expect.arrayContaining([{ x: 1, y: 5 }, { x: 2, y: 4 }, { x: 5, y: 1 }]));
    expect(front).toHaveLength(3);
  });

  it("混合方向时按各自方向判定支配关系", () => {
    const points = [
      { x: 1, y: 1 },
      { x: 2, y: 2 },
      { x: 3, y: 3 },
    ];
    // x 越小越好、y 越大越好 → 三个点互不支配。
    expect(paretoFront(points, true, false)).toHaveLength(3);
    // x 越大越好、y 越大越好 → 只有右上角。
    expect(paretoFront(points, false, false)).toEqual([{ x: 3, y: 3 }]);
  });

  it("完全相同的点不会互相支配", () => {
    const points = [
      { x: 1, y: 1 },
      { x: 1, y: 1 },
    ];
    expect(paretoFront(points, false, false)).toHaveLength(2);
  });
});
