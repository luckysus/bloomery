import {
  AlertCircle,
  Atom,
  BarChart3,
  Beaker,
  FlaskConical,
  Layers,
  Loader2,
  Sparkles,
  TrendingUp,
} from "lucide-react";
import StatCard from "../components/common/StatCard";
import SearchResultsPanel from "../components/search/SearchResultsPanel";

type SearchPageProps = Record<string, any>;

export default function SearchPage(props: SearchPageProps) {
  const {
    isAgentMode,
    loading,
    data,
    includeProduction,
    adviceModeEnabled,
    isCoilMatchMode,
    coilMatchResults,
    coilMatchLoading,
    coilMatchError,
    resultView,
    isAIMode,
    isStreaming,
    isProductionAI,
    steelMark,
    steelGrade,
    setResultView,
    setActiveTab,
    stopAIStreaming,
    aiAnswerRef,
    aiAnswer,
    handleOpenOptimizer,
    visibleTabs,
    activeTab,
    handleTabChange,
    openAdvancedFilter,
    handleExport,
    exporting,
    resultPaneRef,
    handleResultPaneScroll,
    totalProductionCount,
    displayedProductionRecords,
    renderHighlighted,
    query,
    proxyImg,
    setLightboxSrc,
  } = props;

  return (
    <>
      {!isCoilMatchMode && data && data.success && includeProduction && data.production_stats && !adviceModeEnabled && (
        <section className="px-6 pb-3 shrink-0 max-md:px-3">
          <div className="flex items-center gap-2 text-sm font-semibold uppercase tracking-widest text-slate-500 mb-2">
            <BarChart3 size={16} />
            生产数据统计
          </div>
          <div className="grid grid-cols-2 gap-2 md:grid-cols-5">
            <StatCard icon={Layers} label="批次总数" value={data.production_stats.total_batches} unit="批" color="indigo" />
            <StatCard icon={Atom} label="平均 Nb" value={data.production_stats.avg_nb_content.toFixed(4)} unit="%" color="cyan" />
            <StatCard icon={BarChart3} label="平均屈服强度" value={data.production_stats.avg_yield_strength.toFixed(1)} unit="MPa" color="emerald" />
            <StatCard icon={Beaker} label="平均 Ti" value={data.production_stats.avg_ti_content.toFixed(4)} unit="%" color="amber" />
            <StatCard icon={FlaskConical} label="平均快冷温度" value={data.production_stats.avg_fast_cooling_temp.toFixed(1)} unit="℃" color="rose" />
          </div>
        </section>
      )}

      {data && !data.success && data.error && (
        <section className="px-6 pb-4 shrink-0 max-md:px-3">
          <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-600 animate-in">
            <span className="font-semibold">检索失败：</span> {data.error}
          </div>
        </section>
      )}

      {isCoilMatchMode && (
        <section className="flex-1 px-6 pb-4 min-h-0 overflow-hidden max-md:px-3">
          <div className="h-full rounded-xl border border-slate-200 bg-white p-4 shadow-sm flex flex-col max-md:p-3">
            <div className="flex items-center justify-between mb-3 shrink-0 max-md:flex-wrap max-md:gap-2">
              <div className="flex items-center gap-2">
                <div className="flex items-center gap-1 rounded-lg border border-[#eadccf] bg-[#fff7ef] p-1">
                  <button className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-base font-medium bg-white text-[#9c593f] shadow-sm border border-[#eadccf]">
                    <TrendingUp size={16} />
                    钢卷匹配
                    <span className="ml-1 rounded-full px-1.5 py-0.5 text-xs font-semibold bg-[#f4dfd2] text-[#a9583e]">
                      {coilMatchResults.length}
                    </span>
                  </button>
                </div>
              </div>
              <span className="text-sm text-slate-400">
                {coilMatchLoading ? "正在匹配中..." : coilMatchResults.length > 0 ? `共匹配到 ${coilMatchResults.length} 个钢卷` : "请在左侧输入性能参数后点击检索"}
              </span>
            </div>
            {coilMatchLoading && (
              <div className="flex items-center justify-center py-12">
                <Loader2 size={24} className="animate-spin text-[#cc785c]" />
                <span className="ml-2 text-slate-500">正在匹配钢卷...</span>
              </div>
            )}
            {coilMatchError && !coilMatchLoading && (
              <div className="rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-700">
                <AlertCircle size={16} className="inline mr-1" />
                {coilMatchError}
              </div>
            )}
            {!coilMatchLoading && coilMatchResults.length > 0 && (
              <div className="flex-1 min-h-0 overflow-auto">
                <table className="min-w-full text-base whitespace-nowrap">
                  <thead className="sticky top-0 bg-slate-100 text-slate-600 z-10 border-b border-slate-200">
                    <tr>
                      <th className="px-4 py-2.5 text-center font-semibold">序号</th>
                      <th className="px-4 py-2.5 text-center font-semibold">钢卷号</th>
                      <th className="px-4 py-2.5 text-center font-semibold">屈服强度 (MPa)</th>
                      <th className="px-4 py-2.5 text-center font-semibold">抗拉强度 (MPa)</th>
                      <th className="px-4 py-2.5 text-center font-semibold">延伸率 (%)</th>
                      <th className="px-4 py-2.5 text-center font-semibold">匹配度</th>
                    </tr>
                  </thead>
                  <tbody>
                    {coilMatchResults.map((item: any, idx: number) => (
                      <tr key={idx} className="border-t border-slate-200 hover:bg-slate-50">
                        <td className="px-4 py-2.5 text-center">{idx + 1}</td>
                        <td className="px-4 py-2.5 text-center">{item.coil_id}</td>
                        <td className="px-4 py-2.5 text-center">{item.yield_strength ?? "-"}</td>
                        <td className="px-4 py-2.5 text-center">{item.tensile_strength ?? "-"}</td>
                        <td className="px-4 py-2.5 text-center">{item.elongation ?? "-"}</td>
                        <td className="px-4 py-2.5 text-center">
                          <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${
                            item.distance < 5 ? "bg-[#f4dfd2] text-[#a9583e]" :
                            item.distance < 20 ? "bg-yellow-100 text-yellow-700" :
                            "bg-slate-100 text-slate-600"
                          }`}>
                            {item.distance < 5 ? "极佳" : item.distance < 20 ? "良好" : "一般"}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            {!coilMatchLoading && coilMatchResults.length === 0 && !coilMatchError && (
              <div className="flex-1 flex items-center justify-center">
                <div className="text-center text-slate-400">
                  <TrendingUp size={48} className="mx-auto mb-3 opacity-30" />
                  <p className="text-lg">输入性能参数后点击“检索”开始匹配</p>
                  <p className="text-sm mt-1">支持屈服强度、抗拉强度、延伸率单项或组合输入</p>
                </div>
              </div>
            )}
          </div>
        </section>
      )}

      {!isAgentMode && !isCoilMatchMode && (data && data.success) && (
        <SearchResultsPanel
          resultView={resultView}
          isAIMode={isAIMode}
          isStreaming={isStreaming}
          isProductionAI={isProductionAI}
          data={data}
          steelMark={steelMark}
          steelGrade={steelGrade}
          adviceModeEnabled={adviceModeEnabled}
          setResultView={setResultView}
          setActiveTab={setActiveTab}
          stopAIStreaming={stopAIStreaming}
          aiAnswerRef={aiAnswerRef}
          aiAnswer={aiAnswer}
          handleOpenOptimizer={handleOpenOptimizer}
          visibleTabs={visibleTabs}
          activeTab={activeTab}
          handleTabChange={handleTabChange}
          openAdvancedFilter={openAdvancedFilter}
          includeProduction={includeProduction}
          handleExport={handleExport}
          exporting={exporting}
          resultPaneRef={resultPaneRef}
          handleResultPaneScroll={handleResultPaneScroll}
          totalProductionCount={totalProductionCount}
          displayedProductionRecords={displayedProductionRecords}
          renderHighlighted={renderHighlighted}
          query={query}
          proxyImg={proxyImg}
          setLightboxSrc={setLightboxSrc}
        />
      )}

      {!loading && !data && !isCoilMatchMode && !isAgentMode && (
        <div className="flex-1 flex flex-col items-center justify-center text-center animate-in px-6 max-md:px-3">
          <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-indigo-50 to-cyan-50 border border-slate-200 mb-4">
            <Sparkles size={28} className="text-indigo-400" />
          </div>
          <h3 className="text-lg font-semibold text-slate-700 mb-1">开始你的检索</h3>
        </div>
      )}
    </>
  );
}
