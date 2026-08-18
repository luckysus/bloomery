import {
  AlertCircle,
  ArrowLeft,
  Atom,
  BarChart3,
  Brain,
  CheckSquare,
  ChevronLeft,
  ChevronRight,
  Download,
  FileText,
  Loader2,
  Settings2,
  SlidersHorizontal,
  Sparkles,
  TrendingUp,
  X,
  Zap,
} from "lucide-react";
import { CartesianGrid, Cell, Legend, ResponsiveContainer, Scatter, ScatterChart, Tooltip, XAxis, YAxis } from "recharts";
import { exportOptimizationScheme } from "./optimizationExport";

type OptimizeWorkbenchProps = Record<string, any>;
type OptimizeResultItem = {
  steelMark: string;
  idx: number;
  error?: string;
  result?: any;
};
type ViewModeByRecord = Record<number, "result" | "log" | "pareto">;
type NumericIndexMap = Record<number, number>;

export default function OptimizeWorkbench(props: OptimizeWorkbenchProps) {
  const {
    showOptimizer,
    setTrainingEntrySource,
    setShowOptimizer,
    setShowTraining,
    hasOptimizerData,
    optimizing,
    optimizeResults,
    optimizeResult,
    optimizerRestoring,
    handleToggleAllStandards,
    selectedStandardIdx,
    activeOptimizerStandardRecords,
    activeOptimizerStandardColumns,
    handleSelectStandard,
    targetYield,
    setTargetYield,
    targetTensile,
    setTargetTensile,
    targetElong,
    setTargetElong,
    optimizeAlgorithm,
    setOptimizeAlgorithm,
    optimizeMaxiter,
    setOptimizeMaxiter,
    optimizePopsize,
    setOptimizePopsize,
    handleOptimize,
    optimizeProgress,
    requestOptimizeCancel,
    optimizeStopping,
    optimizeError,
    currentOptimizingIdx,
    setActiveResultTab,
    activeResultTab,
    optimizeLogs,
    perRecordLogs,
    tabTransition,
    tabViewMode,
    setTabViewMode,
    paretoIdx,
    setParetoIdx,
    exportingScheme,
    setExportingScheme,
  } = props;

  if (!showOptimizer) return null;

  return (        <div className="fixed inset-0 z-50 bg-white flex flex-col">
          {/* 顶部栏 */}
          <div className="flex items-center justify-between px-6 py-4 border-b border-slate-200 shrink-0 max-md:px-3 max-md:py-3">
            <h2 className="text-xl font-bold text-slate-900 flex items-center gap-2 max-md:text-base">
              <Settings2 className="w-6 h-6 text-indigo-600 max-md:w-5 max-md:h-5 shrink-0" />
              <span className="whitespace-nowrap">工艺参数寻优</span>
            </h2>
            <div className="flex items-center gap-3 max-md:gap-1">
              <button
                onClick={() => { setTrainingEntrySource('optimizer'); setShowOptimizer(false); setShowTraining(true); }}
                className="flex items-center gap-2 px-3.5 py-2 text-lg text-slate-500 hover:text-indigo-600 hover:bg-indigo-50 rounded-lg transition-colors max-md:px-2 max-md:text-sm"
              >
                <Brain className="w-6 h-6 max-md:w-5 max-md:h-5 shrink-0" />
                <span className="max-md:hidden">模型训练</span>
              </button>
            <button onClick={() => setShowOptimizer(false)} className="p-2 rounded-lg hover:bg-slate-100 text-slate-500 hover:text-slate-700 transition-colors">
              <X className="w-5 h-5" />
            </button>
            </div>
          </div>

          {/* 页面内容 */}
          <div className="flex-1 overflow-auto px-6 pt-3 pb-6 max-md:px-3">
            {!hasOptimizerData && !optimizing && optimizeResults.length === 0 && !optimizeResult ? (
              /* 无数据提示 */
              <div className="flex flex-col items-center justify-center h-full text-center">
                <div className="w-16 h-16 rounded-full bg-indigo-50 flex items-center justify-center mb-4">
                  {optimizerRestoring ? (
                    <Loader2 className="w-8 h-8 animate-spin text-indigo-400" />
                  ) : (
                    <AlertCircle className="w-8 h-8 text-indigo-400" />
                  )}
                </div>
                <h3 className="text-lg font-semibold text-slate-700 mb-2">
                  {optimizerRestoring ? "正在恢复最近优化结果" : "暂无成分标准数据"}
                </h3>
                <p className="text-sm text-slate-500 max-w-md">
                  {optimizerRestoring
                    ? "如果你之前完成过优化，系统会从服务器数据库恢复最近一批结果。"
                    : <>请先在查询页面开启<span className="font-medium text-indigo-600">「成分建议」</span>模式进行检索，获取标准记录后再进行工艺优化。</>}
                </p>
                <button
                  onClick={() => setShowOptimizer(false)}
                  className="mt-6 flex items-center gap-2 px-4 py-2 text-sm font-medium text-indigo-600 bg-indigo-50 rounded-lg hover:bg-indigo-100 transition-colors"
                >
                  <ArrowLeft className="w-4 h-4" />
                  返回查询
                </button>
              </div>
            ) : (
              /* 有数据 - 左右双栏布局（窄屏单列） */
              <div className="flex gap-6 max-w-[1600px] mx-auto max-md:flex-col max-md:gap-4">
                {/* ======== 左栏：输入 + 按钮 + 日志 ======== */}
                <div className="flex-1 min-w-0 space-y-6">
                  {/* ---- 成分标准选择 ---- */}
                  <div>
                    <h3 className="text-xl font-semibold text-slate-800 mb-3 flex items-center gap-2">
                      <Atom className="w-6 h-6 text-indigo-500" />
                      成分标准
                    </h3>
                    <div className="flex items-center justify-between mb-3">
                      <p className="text-base text-slate-500">点击选择标准作为优化输入（可多选）</p>
                      <button
                        onClick={handleToggleAllStandards}
                        disabled={optimizing}
                        className={`flex items-center gap-1 text-sm font-medium text-indigo-600 hover:text-indigo-700 transition-colors ${optimizing ? 'opacity-50 cursor-not-allowed' : ''}`}
                      >
                        <CheckSquare className="w-4 h-4" />
                        {selectedStandardIdx.size === activeOptimizerStandardRecords.length ? '取消全选' : '全选'}
                      </button>
                    </div>
                    <div className="space-y-2 max-h-[400px] overflow-y-auto" style={{ scrollbarGutter: 'stable' }}>
                      {activeOptimizerStandardRecords.map((rec: any, idx: number) => {
                        const cols = activeOptimizerStandardColumns.filter((c: string) => c !== "created_at");
                        const idCols = cols.filter((c: string) => ["出钢记号", "板坯钢种"].includes(c));
                        const chemCols = cols.filter((c: string) => !["出钢记号", "板坯钢种"].includes(c));
                        const colLabel: Record<string, string> = { "C": "碳C", "Si": "硅Si", "Mn": "锰Mn", "P": "磷P", "S": "硫S", "Nb": "铌Nb", "Ti": "钛Ti", "N": "氮N" };
                        const isSelected = selectedStandardIdx.has(idx);
                        return (
                          <div
                            key={idx}
                            onClick={() => { if (!optimizing) handleSelectStandard(idx); }}
                            className={`rounded-lg border-2 px-4 py-2.5 transition-all ${
                              isSelected
                                ? 'border-indigo-500 bg-indigo-50/60 shadow-md'
                                : 'border-slate-200 bg-white'
                            } ${optimizing ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer hover:border-slate-300 hover:shadow-sm'}`}
                          >
                            {/* 单行紧凑布局：标识 + 元素值 */}
                            <div className="flex items-center gap-2 flex-nowrap overflow-x-auto">
                              {/* 出钢记号 & 板坯钢种 */}
                              {idCols.length > 0 && (
                                <div className="flex items-center gap-2 pr-2 border-r border-slate-200 shrink-0">
                                  {idCols.map((col: string) => (
                                    <div key={col} className="flex items-center gap-0.5 shrink-0">
                                      <span className="text-sm text-slate-400 whitespace-nowrap">{col}</span>
                                      <span className={`text-sm font-semibold whitespace-nowrap ${isSelected ? 'text-indigo-700' : 'text-slate-700'}`}>
                                        {rec ? String(rec[col] ?? "—") : "—"}
                                      </span>
                                    </div>
                                  ))}
                                </div>
                              )}
                              {/* 元素值横排 */}
                              {chemCols.map((col: string) => (
                                <div key={col} className="flex items-center gap-0.5 shrink-0">
                                  <span className="text-sm text-slate-400 whitespace-nowrap">{colLabel[col] || col}</span>
                                  <span className={`px-1.5 py-0.5 rounded border text-sm font-medium whitespace-nowrap ${
                                    isSelected
                                      ? 'border-indigo-300 bg-white text-indigo-700'
                                      : 'border-slate-200 bg-slate-50 text-slate-700'
                                  }`}>
                                    {rec ? String(rec[col] ?? "—") : "—"}
                                  </span>
                                </div>
                              ))}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </div>

                  {/* ---- 目标性能 ---- */}
                  <div>
                    <h3 className="text-xl font-semibold text-slate-800 mb-3 flex items-center gap-2">
                      <BarChart3 className="w-6 h-6 text-indigo-500" />
                      目标性能
                    </h3>
                    <div className="grid grid-cols-3 gap-4 max-md:grid-cols-1 max-md:gap-3">
                      <div>
                        <label className="block text-base font-medium text-slate-600 mb-1.5">屈服强度 (MPa)</label>
                        <input
                          type="number" step="any"
                          value={targetYield}
                          onChange={e => setTargetYield(e.target.value === "" ? "" : parseFloat(e.target.value))}
                          placeholder="输入目标值"
                          className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 outline-none"
                        />
                      </div>
                      <div>
                        <label className="block text-base font-medium text-slate-600 mb-1.5">抗拉强度 (MPa)</label>
                        <input
                          type="number" step="any"
                          value={targetTensile}
                          onChange={e => setTargetTensile(e.target.value === "" ? "" : parseFloat(e.target.value))}
                          placeholder="输入目标值"
                          className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 outline-none"
                        />
                      </div>
                      <div>
                        <label className="block text-base font-medium text-slate-600 mb-1.5">延伸率 (%)</label>
                        <input
                          type="number" step="any"
                          value={targetElong}
                          onChange={e => setTargetElong(e.target.value === "" ? "" : parseFloat(e.target.value))}
                          placeholder="输入目标值"
                          className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 outline-none"
                        />
                      </div>
                    </div>
                  </div>

                  {/* ---- 算法参数 ---- */}
                  <div>
                    <h3 className="text-xl font-semibold text-slate-800 mb-3 flex items-center gap-2">
                      <SlidersHorizontal className="w-6 h-6 text-indigo-500" />
                      算法参数
                    </h3>
                    <div className="grid grid-cols-3 gap-4 max-md:grid-cols-1 max-md:gap-3">
                      <div>
                        <label className="block text-base font-medium text-slate-600 mb-1.5">{optimizeAlgorithm === 'nsga2' ? '迭代代数' : '最大迭代次数'}</label>
                        <input
                          type="number" min={10} step={1}
                          value={optimizeMaxiter}
                          onChange={e => setOptimizeMaxiter(e.target.value)}
                          onBlur={() => { const v = parseInt(optimizeMaxiter); if (isNaN(v) || v < 10) setOptimizeMaxiter(optimizeAlgorithm === 'nsga2' ? '200' : '500'); }}
                          className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 outline-none"
                        />
                      </div>
                      <div>
                        <label className="block text-base font-medium text-slate-600 mb-1.5">种群大小</label>
                        <input
                          type="number" min={5} step={1}
                          value={optimizePopsize}
                          onChange={e => setOptimizePopsize(e.target.value)}
                          onBlur={() => { const v = parseInt(optimizePopsize); if (isNaN(v) || v < 5) setOptimizePopsize(optimizeAlgorithm === 'nsga2' ? '100' : '30'); }}
                          className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 outline-none"
                        />
                      </div>
                      <div>
                        <label className="block text-base font-medium text-slate-600 mb-1.5">优化算法</label>
                        <select
                          value={optimizeAlgorithm}
                          onChange={e => {
                                                    const algo = e.target.value;
                                                    setOptimizeAlgorithm(algo);
                                                    if (algo === 'nsga2') {
                                                      setOptimizeMaxiter('200');
                                                      setOptimizePopsize('100');
                                                    } else {
                                                      setOptimizeMaxiter('500');
                                                      setOptimizePopsize('30');
                                                    }
                                                  }}
                          className="w-full px-3 py-2.5 border border-slate-300 rounded-lg text-base focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 outline-none bg-white"
                        >
                          <option value="nsga2">NSGA-II 多目标</option>
                          <option value="differential_evolution">差分进化</option>
                        </select>
                      </div>
                    </div>
                  </div>

                  {/* ---- 开始优化按钮 ---- */}
                  <div className="flex justify-end gap-3">
                    <button
                      onClick={handleOptimize}
                      disabled={optimizing || selectedStandardIdx.size === 0}
                      className="flex items-center gap-2 px-6 py-3 text-base font-medium text-white bg-indigo-600 rounded-lg hover:bg-indigo-700 disabled:opacity-60 disabled:cursor-not-allowed transition-colors shadow-sm"
                    >
                      {optimizing ? (
                        <>
                          <div className="w-5 h-5 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                          正在优化 {optimizeProgress}...
                        </>
                      ) : (
                        <>
                          <Zap className="w-5 h-5" />
                          {selectedStandardIdx.size > 1 ? `批量优化 (${selectedStandardIdx.size}条)` : '开始优化'}
                        </>
                      )}
                    </button>
                    {optimizing && (
                      <button
                        onClick={requestOptimizeCancel}
                        disabled={optimizeStopping}
                        className="flex items-center gap-2 px-5 py-3 text-base font-medium border border-red-500 text-red-500 rounded-lg hover:bg-red-50 disabled:opacity-60 disabled:cursor-not-allowed transition-colors"
                      >
                        {optimizeStopping ? '正在停止...' : '停止优化'}
                      </button>
                    )}
                  </div>

                </div>

                {/* ======== 右栏：优化结果 ======== */}
                <div className="w-[42%] shrink-0 space-y-1 max-md:w-full">
                  {/* 错误信息 */}
                  {optimizeError && (
                    <div className="p-4 bg-red-50 border border-red-200 rounded-lg text-base text-red-700 flex items-center gap-2">
                      <AlertCircle className="w-5 h-5 shrink-0" />
                      {optimizeError}
                    </div>
                  )}

                  {(optimizeResults.length > 0 || currentOptimizingIdx !== null) ? (
                    <div className="space-y-1">
                      {/* Tab 栏：已完成 + 正在优化 */}
                      {(() => {
                        const hasLoadingTab = currentOptimizingIdx !== null && !optimizeResults.some((r: OptimizeResultItem) => r.idx === currentOptimizingIdx);
                        const totalTabs = optimizeResults.length + (hasLoadingTab ? 1 : 0);
                        if (totalTabs <= 1 && !hasLoadingTab) return null;
                        type TabItem = { steelMark: string; idx: number; status: 'done' | 'error' | 'loading'; resultIdx?: number };
                        const tabs: TabItem[] = optimizeResults.map((r: OptimizeResultItem, i: number) => ({
                          steelMark: r.steelMark,
                          idx: r.idx,
                          status: r.error ? 'error' : r.result?.success ? 'done' : 'error',
                          resultIdx: i,
                        }));
                        if (hasLoadingTab) {
                          tabs.push({
                            steelMark: String(activeOptimizerStandardRecords[currentOptimizingIdx!]?.['出钢记号'] ?? '优化中'),
                            idx: currentOptimizingIdx!,
                            status: 'loading',
                          });
                        }
                        return (
                          <div className="flex gap-1 border-b border-slate-200 overflow-x-auto">
                            {tabs.map((tab, i) => (
                              <button
                                key={`tab-${tab.idx}`}
                                onClick={() => {
                                  if (tab.resultIdx !== undefined) setActiveResultTab(tab.resultIdx);
                                  else if (tab.status === 'loading') setActiveResultTab(-1);
                                }}
                                className={`px-3.5 py-2.5 whitespace-nowrap border-b-2 transition-colors flex items-center gap-2 ${
                                  tab.status === 'loading'
                                    ? (activeResultTab === -1 ? 'border-indigo-600 text-indigo-600 cursor-pointer text-base font-medium' : 'border-transparent text-indigo-500 cursor-pointer text-base font-medium')
                                    : tab.resultIdx !== undefined && activeResultTab === tab.resultIdx
                                      ? 'border-indigo-600 text-indigo-600 cursor-pointer text-sm font-medium'
                                      : 'border-transparent text-slate-500 hover:text-slate-700 cursor-pointer text-sm font-medium'
                                }`}
                              >
                                {tab.status === 'loading' && (
                                  <div className="w-4 h-4 border-2 border-indigo-300 border-t-indigo-600 rounded-full animate-spin" />
                                )}
                                {tab.steelMark}
                                {tab.status === 'error' && (
                                  <span className="inline-flex h-4 w-4 items-center justify-center rounded-full bg-red-500 text-white text-[11px] leading-none">x</span>
                                )}
                                {tab.status === 'done' && (
                                  <span className="inline-flex h-4 w-4 items-center justify-center rounded-full bg-emerald-500 text-white text-[11px] leading-none">&#10003;</span>
                                )}
                              </button>
                            ))}
                          </div>
                        );
                      })()}

                      {/* 内容区：显示当前选中 Tab 的结果，或 loading 状态 */}
                      {(() => {
                        const currentResult = optimizeResults[activeResultTab];
                        // 当前选中 tab 正在 loading 中（没有结果）
                        if (!currentResult) {
                          // 显示正在优化的实时日志
                          const loadingIdx = currentOptimizingIdx;
                          const liveLogs = loadingIdx !== null ? (perRecordLogs[loadingIdx] ?? []) : [];
                          return (
                            <div className="flex flex-col items-center justify-center py-8 text-center">
                              {optimizeLogs.length > 0 && (
                                <div className="w-full bg-gray-900 text-green-400 font-mono text-sm rounded-lg p-3 h-[calc(100vh-300px)] overflow-y-auto text-left">
                                  {optimizeLogs.slice(-30).map((line: string, i: number) => (
                                    <div key={i} className="whitespace-pre-wrap leading-relaxed">{line}</div>
                                  ))}
                                </div>
                              )}
                            </div>
                          );
                        }
                        // 正在过渡中（fade-out 阶段不渲染内容，fade-in / done 使用 animate-in）
                        const transition = tabTransition[currentResult.idx];
                        if (transition === 'fade-out') {
                          return null;
                        }
                        const useAnimateIn = transition === 'fade-in';

                        if (currentResult.error) {
                          return (
                            <div className={useAnimateIn ? 'animate-in' : undefined}>
                              <div className="p-4 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700 flex items-center gap-2">
                                <AlertCircle className="w-4 h-4 shrink-0" />
                                {currentResult.steelMark}: {currentResult.error}
                              </div>
                            </div>
                          );
                        }
                        const res = currentResult.result;
                        if (!res?.success) {
                          return (
                            <div className={useAnimateIn ? 'animate-in' : undefined}>
                              <div className="p-4 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700 flex items-center gap-2">
                                <AlertCircle className="w-4 h-4 shrink-0" />
                                {currentResult.steelMark}: {res?.error || '寻优失败'}
                              </div>
                            </div>
                          );
                        }
                        const recordIdx = currentResult.idx;
                        const recordLogs = perRecordLogs[recordIdx] ?? [];
                        const viewMode = tabViewMode[recordIdx] ?? 'result';
                        const hasPareto = res.pareto_front && res.pareto_front.length > 0;
                        const toggleViewMode = () => setTabViewMode((prev: ViewModeByRecord) => {
                          const cur = prev[recordIdx] ?? 'result';
                          if (cur === 'pareto') return { ...prev, [recordIdx]: 'log' };
                          return { ...prev, [recordIdx]: cur === 'result' ? 'log' : 'result' };
                        });
                        return (
                          <div className={useAnimateIn ? 'animate-in' : undefined}>
                            {/* 右上角切换按钮 */}
                            <div className="flex justify-end mb-1.5 gap-3">
                              {hasPareto && (
                                <button
                                  onClick={() => setTabViewMode((prev: ViewModeByRecord) => ({ ...prev, [recordIdx]: prev[recordIdx] === 'pareto' ? 'result' : 'pareto' }))}
                                  className="text-base text-indigo-600 hover:text-indigo-800 flex items-center gap-1.5 transition-colors"
                                >
                                  {viewMode === 'pareto' ? (
                                    <><BarChart3 className="w-4 h-4" /> 返回</>
                                  ) : (
                                    <><TrendingUp className="w-4 h-4" /> 帕累托解集</>
                                  )}
                                </button>
                              )}
                              <button
                                onClick={toggleViewMode}
                                className="text-base text-indigo-600 hover:text-indigo-800 flex items-center gap-1.5 transition-colors"
                              >
                                {viewMode === 'result' ? (
                                  <><FileText className="w-4 h-4" /> 查看日志</>
                                ) : viewMode === 'log' ? (
                                  <><BarChart3 className="w-4 h-4" /> 查看结果</>
                                ) : (
                                  <><FileText className="w-4 h-4" /> 查看日志</>
                                )}
                              </button>
                            </div>

                            {/* 内容切换区域 */}
                            <div style={{ scrollbarGutter: 'stable' }} className="overflow-y-auto">
                              {/* 结果视图 */}
                              {viewMode === 'result' && (() => {
                                const tabIdx = activeResultTab;
                                const currentScheme = res.pareto_front && res.pareto_front.length > 0
                                  ? res.pareto_front[paretoIdx[tabIdx] ?? res.best_idx ?? 0]
                                  : null;
                                const displayPerf = currentScheme ? currentScheme.predicted_performance : res.predicted_performance;
                                const displayProcess = currentScheme ? currentScheme.optimal_process : res.optimal_process;
                                return (
                              <div>
                                <div className="space-y-2">
                                  {/* 帕累托方案导航 */}
                                  {res.pareto_front && res.pareto_front.length > 0 && (
                                    <div className="mb-2 flex items-center justify-between">
                                      <span className="text-base text-slate-500">
                                        帕累托前沿 — 共 {res.pareto_front.length} 个非支配解
                                      </span>
                                      <div className="flex items-center gap-2">
                                        <button
                                          onClick={() => {
                                            const idx = paretoIdx[tabIdx] ?? res.best_idx ?? 0;
                                            if (idx > 0) setParetoIdx((prev: NumericIndexMap) => ({...prev, [tabIdx]: idx - 1}));
                                          }}
                                          disabled={(paretoIdx[tabIdx] ?? res.best_idx ?? 0) === 0}
                                          className="p-1 rounded hover:bg-slate-100 disabled:opacity-30"
                                        >
                                          <ChevronLeft className="w-5 h-5" />
                                        </button>
                                        <span className="text-base font-medium">
                                          方案 {(paretoIdx[tabIdx] ?? res.best_idx ?? 0) + 1} / {res.pareto_front.length}
                                        </span>
                                        {(paretoIdx[tabIdx] ?? res.best_idx ?? 0) === (res.best_idx ?? 0) && (
                                          <span className="bg-green-100 text-green-700 text-sm px-2.5 py-0.5 rounded-full">推荐</span>
                                        )}
                                        <button
                                          onClick={() => {
                                            const idx = paretoIdx[tabIdx] ?? res.best_idx ?? 0;
                                            if (idx < res.pareto_front.length - 1) setParetoIdx((prev: NumericIndexMap) => ({...prev, [tabIdx]: idx + 1}));
                                          }}
                                          disabled={(paretoIdx[tabIdx] ?? res.best_idx ?? 0) === res.pareto_front.length - 1}
                                          className="p-1 rounded hover:bg-slate-100 disabled:opacity-30"
                                        >
                                          <ChevronRight className="w-5 h-5" />
                                        </button>
                                      </div>
                                    </div>
                                  )}

                                  {/* 预测性能卡片 */}
                                  <div>
                                    <h3 className="text-xl font-semibold text-slate-800 mb-1 flex items-center gap-2">
                                      <Sparkles className="w-6 h-6 text-indigo-500" />
                                      预测性能
                                      {optimizeResults.length > 1 && <span className="text-base font-normal text-slate-400">— {currentResult.steelMark}</span>}
                                    </h3>
                                    <div className="grid grid-cols-3 gap-3">
                                      {[
                                        { key: "yield_strength", label: "屈服强度", unit: "MPa", target: targetYield },
                                        { key: "tensile_strength", label: "抗拉强度", unit: "MPa", target: targetTensile },
                                        { key: "elongation", label: "延伸率", unit: "%", target: targetElong },
                                      ].map(({ key, label, unit, target }) => {
                                        const val = displayPerf?.[key];
                                        const inRange = target !== "" ? val != null && Math.abs(val - (target as number)) / ((target as number) || 1) < 0.1 : true;
                                        return (
                                          <div key={key} className={`p-3 rounded-xl border ${inRange ? 'bg-emerald-50 border-emerald-200' : 'bg-amber-50 border-amber-200'}`}>
                                            <div className="text-base text-slate-500 mb-0.5">{label}</div>
                                            <div className={`text-2xl font-bold ${inRange ? 'text-emerald-700' : 'text-amber-700'}`}>
                                              {val != null ? val.toFixed(2) : '—'}
                                            </div>
                                            <div className="text-xs text-slate-400 mt-1">{unit}</div>
                                          </div>
                                        );
                                      })}
                                    </div>
                                  </div>

                                  {/* 最优工艺参数表格 */}
                                  <div>
                                    <div className="flex items-center justify-between mb-2">
                                      <h3 className="text-xl font-semibold text-slate-800 flex items-center gap-2">
                                        <SlidersHorizontal className="w-6 h-6 text-indigo-500" />
                                        最优工艺参数方案
                                      </h3>
                                      <button
                                        onClick={() => {
                                          if (exportingScheme) return;
                                          setExportingScheme(true);
                                          setTimeout(() => {
                                            exportOptimizationScheme({
                                              steelMark: currentResult.steelMark,
                                              result: res,
                                              displayProcess,
                                              displayPerf,
                                            });
                                            setExportingScheme(false);
                                          }, 800);
                                        }}
                                        disabled={exportingScheme}
                                        className={`flex h-9 items-center gap-1.5 rounded-md border border-slate-200 bg-white px-3.5 text-sm font-medium transition-all duration-200 ${exportingScheme ? 'text-slate-400 cursor-not-allowed' : 'text-slate-600 hover:border-slate-300 hover:bg-slate-50 hover:text-slate-900'}`}
                                      >
                                        {exportingScheme ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
                                        {exportingScheme ? '导出中' : '导出方案'}
                                      </button>
                                    </div>
                                    <div className="border border-slate-200 rounded-xl overflow-hidden">
                                      <table className="w-full text-base">
                                        <thead>
                                          <tr className="bg-slate-50">
                                            <th className="px-5 py-2 text-left font-semibold text-slate-600 border-b border-slate-200">参数</th>
                                            <th className="px-5 py-2 text-right font-semibold text-slate-600 border-b border-slate-200">最优值</th>
                                          </tr>
                                        </thead>
                                        <tbody>
                                          {displayProcess && Object.entries(displayProcess).map(([param, value]: [string, any]) => (
                                            <tr key={param} className="border-t border-slate-100 hover:bg-slate-50/60">
                                              <td className="px-5 py-1.5 text-slate-700">{param}</td>
                                              <td className="px-5 py-1.5 text-right font-mono text-slate-800">{typeof value === 'number' ? value.toFixed(2) : value}</td>
                                            </tr>
                                          ))}
                                        </tbody>
                                      </table>
                                    </div>
                                  </div>

                                  {/* 补齐成分 & 收敛信息 */}
                                  <div className="text-sm text-slate-500 bg-slate-50 rounded-lg px-4 py-2 space-y-0.5">
                                    <div>
                                      使用成分: {res.used_composition ? Object.entries(res.used_composition).map(([k, v]: [string, any]) => `${k}: ${v ?? '—'}`).join(', ') : '—'}
                                    </div>
                                    <div>
                                      迭代次数: {res.optimization_info?.iterations ?? '—'} | 已收敛: {res.optimization_info?.converged ? '是' : '否'}
                                    </div>
                                  </div>
                                </div>
                              </div>
                                ); })()}

                              {/* 日志视图 */}
                              {viewMode === 'log' && (
                              <div>
                                <div className="bg-gray-900 text-green-400 font-mono text-sm rounded-lg p-4 h-[calc(100vh-300px)] overflow-y-auto">
                                  {recordLogs.map((line: string, i: number) => (
                                    <div key={i} className="whitespace-pre-wrap leading-relaxed">{line}</div>
                                  ))}
                                </div>
                              </div>
                              )}

                              {/* 帕累托解集视图 */}
                              {viewMode === 'pareto' && hasPareto && (() => {
                                const paretoData = res.pareto_front as Array<{predicted_performance: Record<string, number>; optimal_process: Record<string, number>}>;
                                const bestIndex = res.best_idx ?? 0;
                                const rawChartData = paretoData.map((p: any, i: number) => ({
                                  ys: p.predicted_performance?.yield_strength ?? 0,
                                  ts: p.predicted_performance?.tensile_strength ?? 0,
                                  el: p.predicted_performance?.elongation ?? 0,
                                  isBest: i === bestIndex,
                                  idx: i,
                                }));
                                // 对重叠点添加 jitter 微偏移，确保所有点可见
                                const chartData = (() => {
                                  const seen = new Map<string, number>();
                                  const ysVals = rawChartData.map(d => d.ys);
                                  const tsVals = rawChartData.map(d => d.ts);
                                  const elVals = rawChartData.map(d => d.el);
                                  const range = (vals: number[]) => { const mn = Math.min(...vals), mx = Math.max(...vals); return mx - mn || 1; };
                                  const ysRange = range(ysVals), tsRange = range(tsVals), elRange = range(elVals);
                                  const jitterScale = 0.008; // 0.8% of range
                                  return rawChartData.map(d => {
                                    const key = `${d.ys.toFixed(4)}_${d.ts.toFixed(4)}_${d.el.toFixed(4)}`;
                                    const count = seen.get(key) ?? 0;
                                    seen.set(key, count + 1);
                                    if (count === 0) return d;
                                    // 螺旋式偏移避免重叠
                                    const angle = count * 2.399; // golden angle
                                    const radius = Math.sqrt(count) * jitterScale;
                                    return {
                                      ...d,
                                      ys: d.ys + Math.cos(angle) * radius * ysRange,
                                      ts: d.ts + Math.sin(angle) * radius * tsRange,
                                      el: d.el + Math.cos(angle + 1) * radius * elRange,
                                    };
                                  });
                                })();
                                const charts: {title: string; xKey: string; yKey: string; xLabel: string; yLabel: string}[] = [
                                  {title: '屈服强度 vs 抗拉强度', xKey: 'ys', yKey: 'ts', xLabel: '屈服强度 (MPa)', yLabel: '抗拉强度 (MPa)'},
                                  {title: '屈服强度 vs 延伸率', xKey: 'ys', yKey: 'el', xLabel: '屈服强度 (MPa)', yLabel: '延伸率 (%)'},
                                  {title: '抗拉强度 vs 延伸率', xKey: 'ts', yKey: 'el', xLabel: '抗拉强度 (MPa)', yLabel: '延伸率 (%)'},
                                ];
                                // 计算各轴数据范围，留5%边距
                                const axisDomain = (key: string) => {
                                  const vals = chartData.map((d: any) => d[key] as number);
                                  const min = Math.min(...vals);
                                  const max = Math.max(...vals);
                                  const pad = (max - min) * 0.05 || 1;
                                  return [Math.floor((min - pad) * 100) / 100, Math.ceil((max + pad) * 100) / 100] as [number, number];
                                };
                                return (
                                  <div className="h-[calc(100vh-300px)] overflow-y-auto">
                                    <h3 className="text-xl font-semibold text-slate-800 mb-4 flex items-center gap-2">
                                      <TrendingUp className="w-6 h-6 text-indigo-500" />
                                      帕累托前沿散点图
                                      <span className="text-base font-normal text-slate-400">— 共 {paretoData.length} 个非支配解</span>
                                    </h3>
                                    <div className="grid grid-cols-1 gap-6">
                                      {charts.map((c) => (
                                        <div key={c.title} className="bg-white border border-slate-200 rounded-xl p-4">
                                          <div className="text-sm font-medium text-slate-600 mb-2 text-center">{c.title}</div>
                                          <ResponsiveContainer width="100%" height={260}>
                                            <ScatterChart margin={{top: 10, right: 20, bottom: 20, left: 20}}>
                                              <CartesianGrid strokeDasharray="3 3" stroke="#e6dfd8" />
                                              <XAxis type="number" dataKey={c.xKey} name={c.xLabel} domain={axisDomain(c.xKey)} tick={{fontSize: 11}} label={{value: c.xLabel, position: 'insideBottom', offset: -10, style: {fontSize: 12, fill: '#6c6a64'}}} />
                                              <YAxis type="number" dataKey={c.yKey} name={c.yLabel} domain={axisDomain(c.yKey)} tick={{fontSize: 11}} label={{value: c.yLabel, angle: -90, position: 'insideLeft', offset: 5, style: {fontSize: 12, fill: '#6c6a64'}}} />
                                              <Tooltip cursor={{strokeDasharray: '3 3'}} content={({active, payload}: any) => {
                                                if (!active || !payload?.length) return null;
                                                const d = payload[0].payload;
                                                return (
                                                  <div className="bg-white border border-slate-200 rounded-lg shadow-lg px-3 py-2 text-xs">
                                                    <div className="font-medium text-slate-700 mb-1">方案 {d.idx + 1}{d.isBest ? ' (推荐)' : ''}</div>
                                                    <div className="text-slate-500">{c.xLabel}: {d[c.xKey]?.toFixed(2)}</div>
                                                    <div className="text-slate-500">{c.yLabel}: {d[c.yKey]?.toFixed(2)}</div>
                                                  </div>
                                                );
                                              }} />
                                              <Legend verticalAlign="top" content={() => (
                                                <div className="flex justify-center gap-4 text-xs mb-1">
                                                  <span className="flex items-center gap-1"><span className="w-2.5 h-2.5 rounded-full bg-indigo-400 inline-block" />非支配解</span>
                                                  <span className="flex items-center gap-1"><span className="w-3 h-3 rounded-full bg-rose-500 inline-block" />推荐方案</span>
                                                </div>
                                              )} />
                                              <Scatter data={chartData}>
                                                {chartData.map((entry, index) => (
                                                  <Cell key={index} fill={entry.isBest ? '#c64545' : '#cc785c'} r={entry.isBest ? 6 : 3} fillOpacity={entry.isBest ? 0.95 : 0.55} stroke={entry.isBest ? '#8f2f2f' : '#a9583e'} strokeWidth={0.5} strokeOpacity={0.6} />
                                                ))}
                                              </Scatter>
                                            </ScatterChart>
                                          </ResponsiveContainer>
                                        </div>
                                      ))}
                                    </div>
                                  </div>
                                );
                              })()}
                            </div>
                          </div>
                        );
                      })()}
                    </div>
                  ) : (
                    /* 无结果占位 */
                    <div className="flex flex-col items-center justify-center h-full text-center py-20">
                      <div className="w-14 h-14 rounded-full bg-slate-100 flex items-center justify-center mb-4">
                        <Sparkles className="w-7 h-7 text-slate-300" />
                      </div>
                      <p className="text-base text-slate-400">优化结果将在此显示</p>
                      <p className="text-sm text-slate-300 mt-1">配置左侧参数后点击「开始优化」</p>
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
  );
}
