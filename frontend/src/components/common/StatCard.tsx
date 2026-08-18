import type { LucideIcon } from "lucide-react";

type StatCardProps = {
  icon: LucideIcon;
  label: string;
  value: string | number;
  unit: string;
  color: string;
};

export default function StatCard({ icon: Icon, label, value, unit, color }: StatCardProps) {
  const colorMap: Record<string, { dot: string; iconBg: string; iconText: string }> = {
    indigo: { dot: "bg-indigo-500", iconBg: "bg-indigo-50", iconText: "text-indigo-600" },
    blue: { dot: "bg-blue-500", iconBg: "bg-blue-50", iconText: "text-blue-600" },
    cyan: { dot: "bg-cyan-500", iconBg: "bg-cyan-50", iconText: "text-cyan-600" },
    emerald: { dot: "bg-emerald-500", iconBg: "bg-emerald-50", iconText: "text-emerald-600" },
    amber: { dot: "bg-amber-500", iconBg: "bg-amber-50", iconText: "text-amber-600" },
    rose: { dot: "bg-rose-500", iconBg: "bg-rose-50", iconText: "text-rose-600" },
  };
  const c = colorMap[color] ?? colorMap.indigo;

  return (
    <div className="bloomery-stat-card group relative rounded-xl border border-slate-200 bg-white p-4 shadow-sm transition-all duration-300 hover:-translate-y-0.5 hover:shadow-lg hover:border-slate-300">
      <div
        className={`absolute -top-1 -right-1 h-2 w-2 rounded-full ${c.dot} opacity-60 blur-[2px] group-hover:opacity-100 transition-opacity`}
      />
      <div className="flex items-center gap-2 mb-2">
        <div className={`flex h-7 w-7 items-center justify-center rounded-md ${c.iconBg}`}>
          <Icon size={14} className={c.iconText} />
        </div>
        <span className="bloomery-stat-card-label text-sm font-medium text-slate-500">{label}</span>
      </div>
      <p className="bloomery-stat-card-value text-xl font-semibold tracking-tight text-slate-900">
        {value}
        <span className="bloomery-stat-card-unit ml-1 text-xs font-normal text-slate-400">{unit}</span>
      </p>
    </div>
  );
}
