import { PolarAngleAxis, PolarGrid, PolarRadiusAxis, Radar, RadarChart, ResponsiveContainer, Tooltip } from "recharts";
import type { DatasetColumnAnalysis } from "../../bridge/desktop";

/**
 * 雷达图（第 32 章）。
 *
 * 用于横向比较多个数值字段的相对水平。各字段量纲不同，因此按
 * (均值 − 最小值) / (最大值 − 最小值) 归一化到 0–1 后再绘制；
 * 这是纯展示用的归一化，不改变后端返回的原始统计量。
 */
export default function ColumnRadarProfile({ columns }: { columns: DatasetColumnAnalysis[] }) {
  const usable = columns.filter((column) => column.inferredType === "number"
    && column.mean !== null && column.min !== null && column.max !== null);
  if (usable.length < 3) {
    return (
      <div className="suna-data-empty">
        <strong>至少需要三个数值字段</strong>
        <span>雷达图用于比较多个字段的相对水平。</span>
      </div>
    );
  }

  const data = usable.map((column) => {
    const min = column.min ?? 0;
    const max = column.max ?? 1;
    const span = max - min;
    const normalized = span === 0 ? 0.5 : ((column.mean ?? min) - min) / span;
    return {
      field: column.name,
      归一化均值: Number(normalized.toFixed(3)),
      原始均值: column.mean ?? 0,
    };
  });

  return (
    <div className="suna-radar">
      <ResponsiveContainer width="100%" height={260}>
        <RadarChart data={data} outerRadius="72%">
          <PolarGrid stroke="var(--suna-line)" />
          <PolarAngleAxis dataKey="field" tick={{ fill: "var(--suna-text-muted)", fontSize: 10 }} />
          <PolarRadiusAxis domain={[0, 1]} tick={{ fill: "var(--suna-text-muted)", fontSize: 9 }} />
          <Tooltip />
          <Radar name="归一化均值" dataKey="归一化均值" stroke="var(--suna-accent)" fill="var(--suna-accent)" fillOpacity={0.18} />
        </RadarChart>
      </ResponsiveContainer>
      <p className="suna-correlation-legend">按各字段自身的最小/最大值归一化后比较；悬停可查看原始均值。</p>
    </div>
  );
}
