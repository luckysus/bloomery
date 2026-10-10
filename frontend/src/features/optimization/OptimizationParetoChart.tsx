import { useMemo, useRef, useState } from "react";
import { CartesianGrid, Legend, ResponsiveContainer, Scatter, ScatterChart, Tooltip, XAxis, YAxis } from "recharts";
import type { ComputeOptimizationRecommendation } from "../../bridge/desktop";

/**
 * 第 42 章 Pareto Front 可视化：二维与三维。
 *
 * - 2 个目标：Recharts 散点 + 非支配前沿折线；
 * - 3 个目标：纯 SVG 可旋转投影散点（拖拽旋转 yaw/pitch，深度排序控制
 *   遮挡与点径），不引入 echarts-gl 等重依赖；
 * - 4 个目标：提示仅支持表格查看。
 */

type Point = {
  objectives: number[];
  prediction: number;
  onFront: boolean;
  label: string;
};

function shortName(name: string): string {
  return name.length > 14 ? `${name.slice(0, 13)}…` : name;
}

export default function OptimizationParetoChart({
  front,
  recommendations,
  objectives,
}: {
  front: ComputeOptimizationRecommendation[];
  recommendations: ComputeOptimizationRecommendation[];
  objectives: string[];
}) {
  const frontKeys = useMemo(
    () => new Set(front.map((entry) => JSON.stringify(entry.values))),
    [front],
  );
  const points: Point[] = useMemo(
    () =>
      recommendations.map((entry, index) => ({
        objectives: entry.objectives,
        prediction: entry.prediction,
        onFront: frontKeys.has(JSON.stringify(entry.values)),
        label: `方案 ${String.fromCharCode(65 + (index % 26))}`,
      })),
    [recommendations, frontKeys],
  );

  if (objectives.length === 2) {
    return <Pareto2D points={points} objectives={objectives} />;
  }
  if (objectives.length === 3) {
    return <Pareto3D points={points} objectives={objectives} />;
  }
  return <p className="suna-optimization-muted">目标超过 3 个时暂不支持图形化 Pareto 前沿，请使用候选方案表。</p>;
}

/** 二维散点数据：全部候选 + 按目标 0 排序的前沿折线点。 */
export function paretoChartData(points: Point[]): {
  data: Array<{ x: number; y: number; z: number; front: boolean }>;
  frontLine: Array<{ x: number; y: number; z: number; front: boolean }>;
} {
  const data = points.map((point, index) => ({
    x: point.objectives[0],
    y: point.objectives[1],
    z: index,
    front: point.onFront,
  }));
  const frontLine = data
    .filter((item) => item.front)
    .sort((left, right) => left.x - right.x);
  return { data, frontLine };
}

function Pareto2D({ points, objectives }: { points: Point[]; objectives: string[] }) {
  const { data, frontLine } = paretoChartData(points);
  return (
    <div className="suna-optimization-chart">
      <ResponsiveContainer width="100%" height={260}>
        <ScatterChart margin={{ top: 12, right: 16, left: -8, bottom: 8 }}>
          <CartesianGrid stroke="var(--suna-line)" />
          <XAxis type="number" dataKey="x" name={shortName(objectives[0])} tick={{ fontSize: 10, fill: "var(--suna-text-muted)" }} />
          <YAxis type="number" dataKey="y" name={shortName(objectives[1])} tick={{ fontSize: 10, fill: "var(--suna-text-muted)" }} />
          <Tooltip cursor={{ strokeDasharray: "3 3" }} />
          <Legend />
          <Scatter name="前沿解" data={frontLine} fill="var(--suna-primary)" line={{ stroke: "var(--suna-primary)", strokeWidth: 1.5 }} />
          <Scatter name="候选解" data={data.filter((item) => !item.front)} fill="var(--suna-text-muted)" fillOpacity={0.55} />
        </ScatterChart>
      </ResponsiveContainer>
    </div>
  );
}

const VIEW_WIDTH = 320;
const VIEW_HEIGHT = 260;

function Pareto3D({ points, objectives }: { points: Point[]; objectives: string[] }) {
  const [yaw, setYaw] = useState(Math.PI / 5);
  const [pitch, setPitch] = useState(0.45);
  const dragging = useRef<{ x: number; y: number; yaw: number; pitch: number } | null>(null);

  const projected = useMemo(() => {
    const all = points.flatMap((point) => point.objectives);
    const min = [Math.min(...all.filter((_, index) => index % 3 === 0)), Math.min(...all.filter((_, index) => index % 3 === 1)), Math.min(...all.filter((_, index) => index % 3 === 2))];
    const max = [Math.max(...all.filter((_, index) => index % 3 === 0)), Math.max(...all.filter((_, index) => index % 3 === 1)), Math.max(...all.filter((_, index) => index % 3 === 2))];
    const span = [0, 1, 2].map((axis) => (max[axis] - min[axis]) || 1);
    const scale = 0.8;
    const cosYaw = Math.cos(yaw);
    const sinYaw = Math.sin(yaw);
    const cosPitch = Math.cos(pitch);
    const sinPitch = Math.sin(pitch);
    const rendered = points.map((point) => {
      const normalized = [0, 1, 2].map((axis) => ((point.objectives[axis] - min[axis]) / span[axis] - 0.5) * scale);
      // 绕 z 轴 yaw，再绕 x 轴 pitch 的右手系投影。
      const x1 = normalized[0] * cosYaw - normalized[1] * sinYaw;
      const y1 = normalized[0] * sinYaw + normalized[1] * cosYaw;
      const y2 = y1 * cosPitch - normalized[2] * sinPitch;
      const depth = y1 * sinPitch + normalized[2] * cosPitch;
      return {
        ...point,
        cx: VIEW_WIDTH / 2 + x1 * 120,
        cy: VIEW_HEIGHT / 2 - y2 * 100,
        depth,
      };
    });
    return rendered.sort((left, right) => right.depth - left.depth);
  }, [points, yaw, pitch]);

  const startDrag = (event: React.PointerEvent<SVGSVGElement>) => {
    dragging.current = { x: event.clientX, y: event.clientY, yaw, pitch };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const moveDrag = (event: React.PointerEvent<SVGSVGElement>) => {
    const state = dragging.current;
    if (!state) return;
    setYaw(state.yaw + (event.clientX - state.x) * 0.01);
    setPitch(Math.max(-1.4, Math.min(1.4, state.pitch + (event.clientY - state.y) * 0.01)));
  };
  const endDrag = () => { dragging.current = null; };

  return (
    <div className="suna-optimization-chart">
      <svg
        width="100%"
        viewBox={`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`}
        role="img"
        aria-label="三维 Pareto 前沿（拖拽旋转）"
        style={{ touchAction: "none", cursor: dragging.current ? "grabbing" : "grab" }}
        onPointerDown={startDrag}
        onPointerMove={moveDrag}
        onPointerUp={endDrag}
        onPointerLeave={endDrag}
      >
        {[0, 1, 2].map((axis) => (
          <text
            key={axis}
            x={axis === 0 ? 14 : VIEW_WIDTH - 14}
            y={axis === 2 ? 16 : VIEW_HEIGHT - 8}
            textAnchor={axis === 0 ? "start" : "end"}
            fontSize="10"
            fill="var(--suna-text-muted)"
          >
            {axis === 2 ? `↑ ${shortName(objectives[2])}` : shortName(objectives[axis])}
          </text>
        ))}
        {projected.map((point) => (
          <g key={point.label}>
            <circle
              cx={point.cx}
              cy={point.cy}
              r={point.onFront ? 5 : 3.5}
              fill={point.onFront ? "var(--suna-primary)" : "var(--suna-text-muted)"}
              fillOpacity={point.onFront ? 0.95 : 0.5}
            >
              <title>{`${point.label}：${point.objectives.map((value) => value.toFixed(2)).join(" / ")}`}</title>
            </circle>
            {point.onFront && (
              <text x={point.cx + 7} y={point.cy + 3} fontSize="9" fill="var(--suna-text-soft)">
                {point.label}
              </text>
            )}
          </g>
        ))}
      </svg>
      <p className="suna-optimization-muted">拖拽旋转视角；蓝色点为非支配前沿解，灰色点为其余候选。</p>
    </div>
  );
}
