import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { DatasetColumnAnalysis } from "../../bridge/desktop";

/**
 * 分布直方图（第 32 章）。
 *
 * 直接渲染后端返回的分箱结果（lowerBound / upperBound / count），
 * 不做前端重采样。可在多个数值字段间切换。
 */
export default function DistributionHistogram({
  column,
  onColumnChange,
  candidates,
}: {
  column: DatasetColumnAnalysis | null;
  candidates: DatasetColumnAnalysis[];
  onColumnChange: (ordinal: number) => void;
}) {
  const bins = column?.distribution ?? [];
  const data = bins.map((bin) => ({
    label: `${bin.lowerBound.toFixed(2)}`,
    count: bin.count,
  }));

  return (
    <div className="suna-distribution">
      {candidates.length > 0 && (
        <div className="suna-distribution-picker" role="group" aria-label="选择分布字段">
          {candidates.map((item) => (
            <button
              key={item.ordinal}
              type="button"
              className={item.ordinal === column?.ordinal ? "is-active" : ""}
              aria-pressed={item.ordinal === column?.ordinal}
              onClick={() => onColumnChange(item.ordinal)}
            >
              {item.name}
            </button>
          ))}
        </div>
      )}
      {data.length === 0 ? (
        <div className="suna-data-empty">
          <strong>该字段暂无分布数据</strong>
          <span>请选择含有效数值的字段。</span>
        </div>
      ) : (
        <ResponsiveContainer width="100%" height={210}>
          <BarChart data={data} margin={{ top: 12, right: 8, left: -18, bottom: 4 }}>
            <CartesianGrid stroke="var(--suna-line)" vertical={false} />
            <XAxis dataKey="label" tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
            <YAxis tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} allowDecimals={false} />
            <Tooltip />
            <Bar dataKey="count" fill="var(--suna-accent)" radius={[3, 3, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      )}
    </div>
  );
}
