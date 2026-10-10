import { useMemo, useState } from "react";
import { CartesianGrid, Legend, ResponsiveContainer, Scatter, ScatterChart, Tooltip, XAxis, YAxis } from "recharts";
import type { DatasetSeries } from "../../bridge/desktop";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";

/**
 * Pareto Front（第 32 章）。
 *
 * 取两列作为目标，按方向（越小越好 / 越大越好）算出**非支配解集**并高亮。
 * 支配判定是 O(n²)，因此这里只对最多 1500 个点求前沿，避免大表卡住渲染。
 */
const MAX_FRONT_POINTS = 1500;

export function paretoFront(
  points: Array<{ x: number; y: number }>,
  minimizeX: boolean,
  minimizeY: boolean,
) {
  const dominated = new Array(points.length).fill(false);
  for (let i = 0; i < points.length; i += 1) {
    for (let j = 0; j < points.length; j += 1) {
      if (i === j || dominated[i]) continue;
      const candidate = points[j];
      const current = points[i];
      const notWorse =
        (minimizeX ? candidate.x <= current.x : candidate.x >= current.x) &&
        (minimizeY ? candidate.y <= current.y : candidate.y >= current.y);
      const strictlyBetter =
        (minimizeX ? candidate.x < current.x : candidate.x > current.x) ||
        (minimizeY ? candidate.y < current.y : candidate.y > current.y);
      if (notWorse && strictlyBetter) dominated[i] = true;
    }
  }
  return points.filter((_, index) => !dominated[index]);
}

export default function ParetoFrontChart({ series }: { series: DatasetSeries }) {
  const [xOrdinal, setXOrdinal] = useState<number | null>(series.columns[0]?.ordinal ?? null);
  const [yOrdinal, setYOrdinal] = useState<number | null>(series.columns[1]?.ordinal ?? null);
  const [minimizeX, setMinimizeX] = useState(false);
  const [minimizeY, setMinimizeY] = useState(false);

  const xIndex = series.columns.findIndex((column) => column.ordinal === xOrdinal);
  const yIndex = series.columns.findIndex((column) => column.ordinal === yOrdinal);
  const xColumn = series.columns[xIndex];
  const yColumn = series.columns[yIndex];

  const { points, front } = useMemo(() => {
    if (xIndex < 0 || yIndex < 0 || xIndex === yIndex) return { points: [], front: [] as Array<{ x: number; y: number }> };
    const paired = series.rows
      .map((row) => ({ x: row[xIndex], y: row[yIndex] }))
      .filter((point): point is { x: number; y: number } => point.x !== null && point.y !== null)
      .slice(0, MAX_FRONT_POINTS);
    return { points: paired, front: paretoFront(paired, minimizeX, minimizeY) };
  }, [series, xIndex, yIndex, minimizeX, minimizeY]);

  const frontKeys = useMemo(() => new Set(front.map((point) => `${point.x}|${point.y}`)), [front]);
  const frontPoints = front.map((point) => ({ ...point, front: 1 }));
  const otherPoints = points.filter((point) => !frontKeys.has(`${point.x}|${point.y}`)).map((point) => ({ ...point, front: 0 }));

  return (
    <div className="suna-scatter">
      <div className="suna-distribution-picker">
        <label>
          X 目标
          <Select value={String(xOrdinal ?? "")} onValueChange={(value) => setXOrdinal(Number(value))}>
            <SelectTrigger aria-label="Pareto X 目标"><SelectValue /></SelectTrigger>
            <SelectContent>
              {series.columns.map((column) => <SelectItem key={column.ordinal} value={String(column.ordinal)}>{column.name}</SelectItem>)}
            </SelectContent>
          </Select>
        </label>
        <label>
          Y 目标
          <Select value={String(yOrdinal ?? "")} onValueChange={(value) => setYOrdinal(Number(value))}>
            <SelectTrigger aria-label="Pareto Y 目标"><SelectValue /></SelectTrigger>
            <SelectContent>
              {series.columns.map((column) => <SelectItem key={column.ordinal} value={String(column.ordinal)}>{column.name}</SelectItem>)}
            </SelectContent>
          </Select>
        </label>
        <label>
          X 方向
          <Select value={minimizeX ? "min" : "max"} onValueChange={(value) => setMinimizeX(value === "min")}>
            <SelectTrigger aria-label="Pareto X 方向"><SelectValue /></SelectTrigger>
            <SelectContent><SelectItem value="max">越大越好</SelectItem><SelectItem value="min">越小越好</SelectItem></SelectContent>
          </Select>
        </label>
        <label>
          Y 方向
          <Select value={minimizeY ? "min" : "max"} onValueChange={(value) => setMinimizeY(value === "min")}>
            <SelectTrigger aria-label="Pareto Y 方向"><SelectValue /></SelectTrigger>
            <SelectContent><SelectItem value="max">越大越好</SelectItem><SelectItem value="min">越小越好</SelectItem></SelectContent>
          </Select>
        </label>
      </div>
      {points.length === 0 ? (
        <p>选择两个不同的数值字段以计算 Pareto 前沿。</p>
      ) : (
        <>
          <ResponsiveContainer width="100%" height={240}>
            <ScatterChart margin={{ top: 10, right: 12, left: -12, bottom: 4 }}>
              <CartesianGrid stroke="var(--suna-line)" />
              <XAxis type="number" dataKey="x" name={xColumn?.name ?? "X"} tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
              <YAxis type="number" dataKey="y" name={yColumn?.name ?? "Y"} tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
              <Tooltip />
              <Legend />
              <Scatter name="全部样本" data={otherPoints} fill="var(--suna-text-muted)" fillOpacity={0.35} />
              <Scatter name={`Pareto 前沿（${front.length}）`} data={frontPoints} fill="var(--suna-primary)" />
            </ScatterChart>
          </ResponsiveContainer>
          <p className="suna-pareto-summary">
            共 {points.length} 个样本，非支配解 {front.length} 个。
          </p>
        </>
      )}
    </div>
  );
}
