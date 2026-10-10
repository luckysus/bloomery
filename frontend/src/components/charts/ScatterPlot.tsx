import { useState } from "react";
import { CartesianGrid, ResponsiveContainer, Scatter, ScatterChart, Tooltip, XAxis, YAxis, ZAxis } from "recharts";
import type { DatasetSeries } from "../../bridge/desktop";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";

/**
 * 散点图（第 32 章）。
 *
 * 数据来自 `read_steel_dataset_series`：后端按行返回所选列的原始数值，
 * 前端只做配对与绘制，不重新计算统计量。任一侧缺失的点会被跳过，
 * 避免用 0 顶替产生虚假相关。
 */
export default function ScatterPlot({ series }: { series: DatasetSeries }) {
  const [xOrdinal, setXOrdinal] = useState<number | null>(series.columns[0]?.ordinal ?? null);
  const [yOrdinal, setYOrdinal] = useState<number | null>(series.columns[1]?.ordinal ?? null);

  const xIndex = series.columns.findIndex((column) => column.ordinal === xOrdinal);
  const yIndex = series.columns.findIndex((column) => column.ordinal === yOrdinal);
  const xColumn = series.columns[xIndex];
  const yColumn = series.columns[yIndex];

  const points = xIndex >= 0 && yIndex >= 0 && xIndex !== yIndex
    ? series.rows
      .map((row) => ({ x: row[xIndex], y: row[yIndex] }))
      .filter((point): point is { x: number; y: number } => point.x !== null && point.y !== null)
    : [];

  return (
    <div className="suna-scatter">
      <div className="suna-distribution-picker">
        <label>
          X 轴
          <Select value={String(xOrdinal ?? "")} onValueChange={(value) => setXOrdinal(Number(value))}>
            <SelectTrigger aria-label="散点图 X 轴"><SelectValue /></SelectTrigger>
            <SelectContent>
              {series.columns.map((column) => (
                <SelectItem key={column.ordinal} value={String(column.ordinal)}>{column.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
        <label>
          Y 轴
          <Select value={String(yOrdinal ?? "")} onValueChange={(value) => setYOrdinal(Number(value))}>
            <SelectTrigger aria-label="散点图 Y 轴"><SelectValue /></SelectTrigger>
            <SelectContent>
              {series.columns.map((column) => (
                <SelectItem key={column.ordinal} value={String(column.ordinal)}>{column.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
      </div>
      {points.length === 0 ? (
        <div className="suna-data-empty">
          <strong>暂无可绘制的点</strong>
          <span>请选择两个不同的数值字段；缺失值不会用 0 代替。</span>
        </div>
      ) : (
        <ResponsiveContainer width="100%" height={240}>
          <ScatterChart margin={{ top: 12, right: 12, left: -14, bottom: 4 }}>
            <CartesianGrid stroke="var(--suna-line)" />
            <XAxis
              type="number"
              dataKey="x"
              name={xColumn?.name}
              tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }}
            />
            <YAxis
              type="number"
              dataKey="y"
              name={yColumn?.name}
              tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }}
            />
            <ZAxis range={[24, 24]} />
            <Tooltip cursor={{ strokeDasharray: "3 3" }} />
            <Scatter data={points} fill="var(--suna-accent)" fillOpacity={0.55} />
          </ScatterChart>
        </ResponsiveContainer>
      )}
      <p className="suna-correlation-legend">
        共 {points.length} 个有效点{series.sampled ? `（从 ${series.totalRows} 行等距抽样）` : ""}。
      </p>
    </div>
  );
}
