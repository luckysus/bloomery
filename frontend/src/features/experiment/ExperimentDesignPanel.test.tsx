import { describe, expect, it } from "vitest";
import { buildExistingMatrix } from "./ExperimentDesignPanel";

describe("buildExistingMatrix", () => {
  const rows: Array<Array<number | null>> = [
    [800, 10, 355],
    [null, 20, 360], // 特征列含 null → 过滤
    [820, null, 365], // 特征列含 null → 过滤
    [840, 30, 370],
    [860, 40, 375],
    [880, 50, 380],
    [900, 60, 385],
    [920, 70, 390],
  ];

  it("keeps only rows where the target and every feature are present", () => {
    const built = buildExistingMatrix(rows, 2, [0, 1]);
    // 第 1、2 行特征含 null，被过滤；其余 6 行完整保留。
    expect(built).toEqual({
      features: [
        [800, 10],
        [840, 30],
        [860, 40],
        [880, 50],
        [900, 60],
        [920, 70],
      ],
      targets: [355, 370, 375, 380, 385, 390],
    });
  });

  it("returns null when fewer than five complete rows remain", () => {
    const sparse = rows.slice(0, 4);
    expect(buildExistingMatrix(sparse, 2, [0, 1])).toBeNull();
  });

  it("supports a single feature column", () => {
    const built = buildExistingMatrix(rows, 2, [1]);
    // 只看第 1 列时，第 0 列的 null 不影响保留；仅第 1 行（第 1 列为 null）被过滤。
    expect(built?.features).toEqual([[10], [20], [30], [40], [50], [60], [70]]);
    expect(built?.targets).toEqual([355, 360, 370, 375, 380, 385, 390]);
  });
});
