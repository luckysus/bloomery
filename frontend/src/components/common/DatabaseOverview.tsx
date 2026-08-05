import { Database, FileText, Image as ImageIcon, Microscope } from "lucide-react";
import { useLocale } from "../../i18n/locale";

type FieldRange = {
  min_val: number;
  max_val: number;
};

export type DatabaseOverviewData = {
  literature_papers_count: number;
  literature_images_count: number;
  experimental_images_count: number;
  production_count: number;
  slab_width_range?: FieldRange | null;
  slab_thickness_range?: FieldRange | null;
  yield_rp02_range?: FieldRange | null;
  tensile_strength_range?: FieldRange | null;
  elongation_range?: FieldRange | null;
};

export default function DatabaseOverview({ data }: { data: DatabaseOverviewData | null }) {
  const { locale, t } = useLocale();
  const stats = [
    { icon: FileText, label: t("literatureCount"), value: data?.literature_papers_count ?? 0, unit: "", bgColor: "bg-indigo-50", textColor: "text-indigo-600" },
    { icon: ImageIcon, label: t("literatureImages"), value: data?.literature_images_count ?? 0, unit: "", bgColor: "bg-cyan-50", textColor: "text-cyan-600" },
    { icon: Microscope, label: t("metallographyCount"), value: data?.experimental_images_count ?? 0, unit: "", bgColor: "bg-emerald-50", textColor: "text-emerald-600" },
    { icon: Database, label: t("productionData"), value: data?.production_count ?? 0, unit: "", bgColor: "bg-amber-50", textColor: "text-amber-600" },
  ];

  const formatNumber = (num: number) => num.toLocaleString(locale);

  return (
    <div className="rounded-xl border border-slate-200 bg-slate-50/50 p-3 mb-4 max-md:p-2 max-md:mb-2">
      <div className="flex items-center gap-2 text-base font-semibold text-slate-500 mb-3 max-md:mb-1.5 max-md:text-sm">
        <Database size={18} />
        {t("databaseOverview")}
      </div>
      <div className="flex flex-col gap-2 max-md:grid max-md:grid-cols-2 max-md:gap-1.5">
        {stats.map((stat) => {
          const Icon = stat.icon;
          return (
            <div
              key={stat.label}
              className="flex items-center justify-between rounded-lg bg-white border border-slate-200 px-3 py-2.5 transition-all duration-200 hover:shadow-sm hover:border-slate-300 max-md:flex-col max-md:items-start max-md:gap-0.5 max-md:px-2 max-md:py-1.5"
            >
              <div className="flex items-center gap-2 max-md:gap-1">
                <div className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-md ${stat.bgColor} max-md:h-5 max-md:w-5`}>
                  <Icon size={14} className={stat.textColor} />
                </div>
                <span className="text-base text-slate-600 max-md:text-xs">{stat.label}</span>
              </div>
              <span className="text-base font-semibold text-slate-900 max-md:text-sm">
                {formatNumber(stat.value)}
                {stat.unit && <span className="text-xs font-normal text-slate-400 ml-0.5">{stat.unit}</span>}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
