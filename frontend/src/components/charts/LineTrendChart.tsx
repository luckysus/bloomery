import { useState } from "react";
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { DatasetSeries } from "../../bridge/desktop";

/**
 * 折线图（第 32 章）。
 *
 * 按行序绘制所选字段的走势，用于观察随炉次/批次的变化趋势。
 * 缺失值以断点表示（`connectNulls` 关闭），不会把缺口连成直线。
 */
export default function LineTrendChart({ series }: { series: DatasetSeries }) {
  const [ordinal, setOrdinal] = useState<number | null>(series.columns[0]?.ordinal ?? null);
  const index = series.columns.findIndex((column) => column.ordinal === ordinal);
  const column = series.columns[index];

  const data = index >= 0
    ? series.rows.map((row, position) => ({ position: position + 1, value: row[index] }))
    : [];

  return (
    <div className="suna-line">
      <div className="suna-distribution-picker">
        {series.columns.map((item) => (
          <button
            key={item.ordinal}
            type="button"
            className={item.ordinal === ordinal ? "is-active" : ""}
            aria-pressed={item.ordinal === ordinal}
            onClick={() => setOrdinal(item.ordinal)}
          >
            {item.name}
          </button>
        ))}
      </div>
      {data.length === 0 ? (
        <div className="suna-data-empty">
          <strong>暂无可绘制的数据</strong>
          <span>请选择至少一个数值字段。</span>
        </div>
      ) : (
        <ResponsiveContainer width="100%" height={220}>
          <LineChart data={data} margin={{ top: 12, right: 12, left: -14, bottom: 4 }}>
            <CartesianGrid stroke="var(--suna-line)" vertical={false} />
            <XAxis dataKey="position" tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
            <YAxis tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
            <Tooltip labelFormatter={(label) => `第 ${label} 行`} />
            <Line
              type="monotone"
              dataKey="value"
              name={column?.name}
              stroke="var(--suna-accent)"
              strokeWidth={1.6}
              dot={false}
              connectNulls={false}
            />
          </LineChart>
        </ResponsiveContainer>
      )}
      <p className="suna-correlation-legend">
        按行序绘制；缺失值处断开{series.sampled ? `，已从 ${series.totalRows} 行等距抽样` : ""}。
      </p>
    </div>
  );
}
