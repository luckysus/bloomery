import { SlidersHorizontal } from "lucide-react";
import DatabaseOverview from "../common/DatabaseOverview";

type RetrievalSidebarControlsProps = Record<string, any>;

export default function RetrievalSidebarControls(props: RetrievalSidebarControlsProps) {
  const {
    overviewData,
    adviceModeEnabled,
    isCoilMatchMode,
    yieldRp02Value,
    setYieldRp02Value,
    tensileStrengthValue,
    setTensileStrengthValue,
    elongationValue,
    setElongationValue,
    slabWidthMin,
    setSlabWidthMin,
    slabWidthMax,
    setSlabWidthMax,
    slabThicknessMin,
    setSlabThicknessMin,
    slabThicknessMax,
    setSlabThicknessMax,
    yieldRp02Min,
    setYieldRp02Min,
    yieldRp02Max,
    setYieldRp02Max,
    tensileStrengthMin,
    setTensileStrengthMin,
    tensileStrengthMax,
    setTensileStrengthMax,
    elongationMin,
    setElongationMin,
    elongationMax,
    setElongationMax,
    topK,
    setTopK,
    includeProduction,
    setIncludeProduction,
  } = props;

  return (
    <>
              {/* 数据库概览 */}
              <DatabaseOverview data={overviewData} />

              <div className="flex items-center gap-2 text-base max-md:text-sm font-semibold text-slate-500 mb-1">
                <SlidersHorizontal size={18} />
                查询参数
              </div>

              {/* 性能参数：建议模式或钢卷匹配模式时只显示三个性能纵向排列，否则显示完整参数 */}
              {(adviceModeEnabled || isCoilMatchMode) ? (
                <div className="space-y-3 max-md:space-y-2 fade-in">
                  {/* 屈服RP0.2 单值输入 */}
                  <div className="space-y-1 max-md:space-y-0.5">
                    <label className="block text-center text-base max-md:text-sm text-slate-600">屈服 RP0.2 (MPa)</label>
                    <input
                      type="number"
                      step={1}
                      value={yieldRp02Value}
                      onChange={(e) => setYieldRp02Value(e.target.value ? +e.target.value : "")}
                      placeholder="输入目标值"
                      className="w-full px-3 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                    />
                  </div>
                  {/* 抗拉强度 单值输入 */}
                  <div className="space-y-1 max-md:space-y-0.5">
                    <label className="block text-center text-base max-md:text-sm text-slate-600">抗拉强度 (MPa)</label>
                    <input
                      type="number"
                      step={1}
                      value={tensileStrengthValue}
                      onChange={(e) => setTensileStrengthValue(e.target.value ? +e.target.value : "")}
                      placeholder="输入目标值"
                      className="w-full px-3 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                    />
                  </div>
                  {/* 断后伸长率 单值输入 */}
                  <div className="space-y-1 max-md:space-y-0.5">
                    <label className="block text-center text-base max-md:text-sm text-slate-600">断后伸长率 A (%)</label>
                    <input
                      type="number"
                      step={0.1}
                      value={elongationValue}
                      onChange={(e) => setElongationValue(e.target.value ? +e.target.value : "")}
                      placeholder="输入目标值"
                      className="w-full px-3 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                    />
                  </div>
                </div>
              ) : (
                <>
                  {/* 板坯宽度 Range */}
                  <div className="space-y-1 max-md:space-y-0.5">
                    <label className="block text-center text-base max-md:text-sm text-slate-600">板坯宽度 (mm)</label>
                    <div className="grid w-full grid-cols-[1fr_auto_1fr] items-center gap-2">
                      <input
                        type="number"
                        step={1}
                        value={slabWidthMin}
                        onChange={(e) => setSlabWidthMin(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                      <span className="text-slate-400">-</span>
                      <input
                        type="number"
                        step={1}
                        value={slabWidthMax}
                        onChange={(e) => setSlabWidthMax(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                    </div>
                  </div>

                  {/* 板坯厚度 Range */}
                  <div className="space-y-1 max-md:space-y-0.5">
                    <label className="block text-center text-base max-md:text-sm text-slate-600">板坯厚度 (mm)</label>
                    <div className="grid w-full grid-cols-[1fr_auto_1fr] items-center gap-2">
                      <input
                        type="number"
                        step={1}
                        value={slabThicknessMin}
                        onChange={(e) => setSlabThicknessMin(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                      <span className="text-slate-400">-</span>
                      <input
                        type="number"
                        step={1}
                        value={slabThicknessMax}
                        onChange={(e) => setSlabThicknessMax(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                    </div>
                  </div>

                  {/* 屈服RP0.2 Range */}
                  <div className="space-y-1 max-md:space-y-0.5">
                    <label className="block text-center text-base max-md:text-sm text-slate-600">屈服 RP0.2 (MPa)</label>
                    <div className="grid w-full grid-cols-[1fr_auto_1fr] items-center gap-2">
                      <input
                        type="number"
                        step={1}
                        value={yieldRp02Min}
                        onChange={(e) => setYieldRp02Min(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                      <span className="text-slate-400">-</span>
                      <input
                        type="number"
                        step={1}
                        value={yieldRp02Max}
                        onChange={(e) => setYieldRp02Max(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                    </div>
                  </div>

                  {/* 抗拉强度 Range */}
                  <div className="space-y-1 max-md:space-y-0.5">
                    <label className="block text-center text-base max-md:text-sm text-slate-600">抗拉强度 (MPa)</label>
                    <div className="grid w-full grid-cols-[1fr_auto_1fr] items-center gap-2">
                      <input
                        type="number"
                        step={1}
                        value={tensileStrengthMin}
                        onChange={(e) => setTensileStrengthMin(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                      <span className="text-slate-400">-</span>
                      <input
                        type="number"
                        step={1}
                        value={tensileStrengthMax}
                        onChange={(e) => setTensileStrengthMax(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                    </div>
                  </div>

                  {/* 断后伸长率A Range */}
                  <div className="space-y-1 max-md:space-y-0.5">
                    <label className="block text-center text-base max-md:text-sm text-slate-600">断后伸长率 A (%)</label>
                    <div className="grid w-full grid-cols-[1fr_auto_1fr] items-center gap-2">
                      <input
                        type="number"
                        step={0.1}
                        value={elongationMin}
                        onChange={(e) => setElongationMin(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                      <span className="text-slate-400">-</span>
                      <input
                        type="number"
                        step={0.1}
                        value={elongationMax}
                        onChange={(e) => setElongationMax(+e.target.value)}
                        className="w-full px-2 py-1.5 max-md:py-1 text-base max-md:text-sm border border-slate-200 rounded-md text-center focus:border-indigo-300 focus:ring-2 focus:ring-indigo-50"
                      />
                    </div>
                  </div>
                </>
              )}

              {/* Top K */}
              <div className="space-y-1 max-md:space-y-0.5">
                <div className="flex items-center justify-between">
                  <label className="text-base max-md:text-sm text-slate-600">返回数量</label>
                  <span className="rounded-md bg-amber-50 px-2 py-0.5 text-base max-md:text-sm font-mono text-amber-600 border border-amber-100">
                    {topK}
                  </span>
                </div>
                <input
                  type="range"
                  min={1}
                  max={20}
                  step={1}
                  value={topK}
                  onChange={(e) => setTopK(+e.target.value)}
                  className="w-full accent-amber-600 h-1.5 bg-slate-200 rounded-full appearance-none cursor-pointer"
                />
              </div>

              <div className="rounded-xl border border-slate-200 bg-slate-50 p-3 mb-6">
                <label className="flex items-center justify-between gap-3 cursor-pointer">
                  <span className="text-base max-md:text-sm text-slate-700">检索生产数据</span>
                  <input
                    type="checkbox"
                    checked={includeProduction}
                    disabled={adviceModeEnabled}
                    onChange={(e) => setIncludeProduction(e.target.checked)}
                    className="h-4 w-4 rounded border-slate-300 text-indigo-600 focus:ring-indigo-500 disabled:cursor-not-allowed disabled:opacity-60"
                  />
                </label>
              </div>
    </>
  );
}
