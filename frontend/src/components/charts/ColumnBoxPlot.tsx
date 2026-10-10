import type { DatasetColumnAnalysis } from "../../bridge/desktop";

/**
 * 箱线图（第 32 章）。
 *
 * 直接使用后端已算好的 min / P25 / median / P75 / max 与离群点数量，
 * 不做二次统计。多个数值字段横向并列，便于比较分布形态。
 */
export default function ColumnBoxPlot({ columns }: { columns: DatasetColumnAnalysis[] }) {
  const usable = columns.filter((column) => column.inferredType === "number"
    && column.min !== null && column.max !== null && column.median !== null);
  if (usable.length === 0) {
    return (
      <div className="suna-data-empty">
        <strong>暂无可绘制的数值字段</strong>
        <span>箱线图需要至少一个含有效数值的字段。</span>
      </div>
    );
  }

  // 所有字段共用一套量纲映射，避免各字段尺度差异导致图形失真。
  const lows = usable.map((column) => column.min ?? 0);
  const highs = usable.map((column) => column.max ?? 0);
  const domainMin = Math.min(...lows);
  const domainMax = Math.max(...highs);
  const span = domainMax - domainMin || 1;
  const toY = (value: number) => 100 - ((value - domainMin) / span) * 100;

  const height = 180;
  const rowHeight = 34;

  return (
    <div className="suna-boxplot">
      <svg
        viewBox={`0 0 600 ${usable.length * rowHeight + 30}`}
        width="100%"
        role="img"
        aria-label="数值字段分布箱线图"
        style={{ fontFamily: "var(--font-sans)" }}
      >
        {[0, 25, 50, 75, 100].map((tick) => (
          <g key={tick}>
            <line x1="140" x2="580" y1={20 + (tick / 100) * height} y2={20 + (tick / 100) * height} stroke="var(--suna-line)" strokeWidth="0.5" />
            <text x="132" y={20 + (tick / 100) * height} textAnchor="end" dominantBaseline="central" fontSize="10" fill="var(--suna-text-muted)">
              {(domainMax - (tick / 100) * span).toFixed(1)}
            </text>
          </g>
        ))}
        {usable.map((column, index) => {
          const top = 20 + index * rowHeight;
          const y = (value: number) => top + (toY(value) / 100) * height;
          const low = column.min ?? domainMin;
          const high = column.max ?? domainMax;
          const q1 = column.percentile25 ?? low;
          const q3 = column.percentile75 ?? high;
          const mid = column.median ?? (q1 + q3) / 2;
          return (
            <g key={column.ordinal}>
              <text x="0" y={top + height / 2} fontSize="11" fill="var(--suna-text)" dominantBaseline="central">
                {column.name}
              </text>
              <line x1="300" x2="300" y1={y(low)} y2={y(high)} stroke="var(--suna-text-muted)" strokeWidth="1" />
              <line x1="284" x2="316" y1={y(low)} y2={y(low)} stroke="var(--suna-text-muted)" strokeWidth="1" />
              <line x1="284" x2="316" y1={y(high)} y2={y(high)} stroke="var(--suna-text-muted)" strokeWidth="1" />
              <rect
                x="284"
                y={Math.min(y(q1), y(q3))}
                width="32"
                height={Math.max(1, Math.abs(y(q3) - y(q1)))}
                fill="rgba(37, 99, 235, 0.16)"
                stroke="var(--suna-accent)"
                strokeWidth="0.8"
              />
              <line x1="284" x2="316" y1={y(mid)} y2={y(mid)} stroke="var(--suna-accent)" strokeWidth="1.6" />
              <text x="330" y={top + height / 2} fontSize="10" fill="var(--suna-text-muted)" dominantBaseline="central">
                n={column.sampleCount} · 离群 {column.outlierCount}
              </text>
            </g>
          );
        })}
      </svg>
      <p className="suna-correlation-legend">箱体为 P25–P75，中线为中位数，须线为最小/最大值；离群点数量来自 IQR 规则。</p>
    </div>
  );
}
