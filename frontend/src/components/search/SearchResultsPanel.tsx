import { ArrowLeft, BotMessageSquare, ChevronRight, Download, FileText, Filter, Layers, Loader2, Settings2, Square, ZoomIn } from "lucide-react";
import AIAnswerRenderer from "../answer/AnswerRenderer";
import EmptyState from "../common/EmptyState";
import type { ImageResult, LitResult } from "../../types/rag";

type SearchResultsPanelProps = Record<string, any>;
type ResultTab = { id: string; label: string; icon: React.ComponentType<{ size?: number; className?: string }> };
type RecordRow = Record<string, any>;
type LiteratureResultItem = LitResult;
type ImageResultItem = ImageResult;

function asArray<T>(value: unknown): T[] {
  return Array.isArray(value) ? value as T[] : [];
}

function scorePercent(value: unknown) {
  const score = Number(value);
  return Number.isFinite(score) ? score * 100 : 0;
}

export default function SearchResultsPanel(props: SearchResultsPanelProps) {
  const {
    resultView,
    isAIMode,
    isStreaming,
    isProductionAI,
    data,
    steelMark,
    steelGrade,
    adviceModeEnabled,
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
    includeProduction,
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

  if (!data?.success) return null;

  const productionColumns = asArray<string>(data.production_columns);
  const productionRecords = asArray<RecordRow>(data.production_records);
  const adviceStandardColumns = asArray<string>(data.advice_standard_columns);
  const adviceStandardRecords = asArray<RecordRow | null>(data.advice_standard_records);
  const literatureResults = asArray<LiteratureResultItem>(data.literature_results);
  const literatureImages = asArray<ImageResultItem>(data.literature_images);
  const experimentalImages = asArray<ImageResultItem>(data.experimental_images);

  return (
            <section className="flex-1 px-6 pb-4 min-h-0 overflow-hidden max-md:px-3">
              <div className={`h-full rounded-xl border bg-white p-4 shadow-sm flex flex-col transition-all duration-300 max-md:p-3 ${
                resultView === "ai" && isAIMode
                  ? "border-indigo-100 ring-1 ring-indigo-50"
                  : "border-slate-200"
              }`}>

                {/* ===== AI 回答视图 ===== */}
                {resultView === "ai" && isAIMode && (
                  <>
                    {/* AI 回答顶栏 */}
                    <div className="flex items-center justify-between mb-4 shrink-0 max-md:flex-wrap max-md:gap-2">
                      <div className="flex items-center gap-3">
                        <div className="relative flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-indigo-500 via-purple-500 to-pink-500 shadow-lg shadow-indigo-200/50">
                          <BotMessageSquare size={16} className="text-white" />
                          {isStreaming && (
                            <span className="absolute -top-0.5 -right-0.5 flex h-3 w-3">
                              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-indigo-400 opacity-75" />
                              <span className="relative inline-flex rounded-full h-3 w-3 bg-indigo-500" />
                            </span>
                          )}
                        </div>
                        <div>
                          <h3 className="text-xl font-bold text-slate-900">AI 综合解答</h3>
                          {isStreaming ? (
                            <div className="flex items-center gap-2">
                              <span className="flex items-center gap-1 text-sm text-indigo-500 font-medium">
                                <Loader2 size={11} className="animate-spin" />
                                {isProductionAI ? (data?.advice_mode ? "正在生成成分建议..." : "正在分析生产数据...") : "正在分析文献并生成回答..."}
                              </span>
                              <button
                                onClick={stopAIStreaming}
                                className="group flex items-center gap-1.5 rounded-full bg-gradient-to-r from-rose-50 to-pink-50 pl-2.5 pr-3 py-1 text-sm font-medium text-rose-500 border border-rose-200/60 shadow-sm shadow-rose-100/50 hover:from-rose-100 hover:to-pink-100 hover:text-rose-600 hover:border-rose-300 hover:shadow-md hover:shadow-rose-200/40 active:scale-95 transition-all duration-200 cursor-pointer"
                              >
                                <span className="flex h-3.5 w-3.5 items-center justify-center rounded-full bg-rose-500/10 group-hover:bg-rose-500/20 transition-colors">
                                  <Square size={7} fill="currentColor" className="text-rose-500" />
                                </span>
                                停止生成
                              </button>
                            </div>
                          ) : aiAnswer ? (
                            <span className="text-sm text-slate-400">{isProductionAI ? (data?.advice_mode ? `基于 ${adviceStandardRecords.length} 条标准数据结果生成` : (steelMark || steelGrade) ? "基于高级筛选后数据结果生成" : `基于 ${productionRecords.length} 条生产数据结果生成`) : `基于 ${literatureResults.length} 篇文献检索结果生成`}</span>
                          ) : null}
                        </div>
                      </div>
                      <button
                        onClick={() => {
                          setResultView("rag");
                          if (adviceModeEnabled) {
                            setActiveTab("production");
                          }
                        }}
                        className="flex h-9 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg border border-slate-200 bg-white px-4 text-base font-medium text-slate-600 transition-all duration-200 hover:border-indigo-300 hover:text-indigo-600 hover:bg-indigo-50 hover:shadow-sm max-md:px-2.5 max-md:text-sm"
                      >
                        <Layers size={15} className="shrink-0" />
                        RAG检索结果
                        <ChevronRight size={14} className="shrink-0 text-slate-400" />
                      </button>
                    </div>
                    {/* AI 回答内容 */}
                    <div ref={aiAnswerRef} className="flex-1 min-h-0 overflow-y-auto pr-1 ai-answer-scroll">
                      {!aiAnswer && isStreaming && (
                        <div className="ai-skeleton-container space-y-4 p-4">
                          <div className="flex items-center gap-3 mb-2">
                            <div className="h-4 w-4 rounded bg-indigo-100 animate-pulse" />
                            <div className="h-4 bg-slate-100 rounded-full w-32 animate-pulse" />
                          </div>
                          <div className="space-y-2.5">
                            <div className="h-3.5 bg-slate-100 rounded-full w-[90%] animate-pulse" style={{animationDelay: '0.1s'}} />
                            <div className="h-3.5 bg-slate-100 rounded-full w-full animate-pulse" style={{animationDelay: '0.2s'}} />
                            <div className="h-3.5 bg-slate-100 rounded-full w-[85%] animate-pulse" style={{animationDelay: '0.3s'}} />
                            <div className="h-3.5 bg-slate-50 rounded-full w-[70%] animate-pulse" style={{animationDelay: '0.4s'}} />
                          </div>
                          <div className="h-px bg-slate-100 my-3" />
                          <div className="space-y-2.5">
                            <div className="h-3.5 bg-slate-100 rounded-full w-[80%] animate-pulse" style={{animationDelay: '0.5s'}} />
                            <div className="h-3.5 bg-slate-100 rounded-full w-full animate-pulse" style={{animationDelay: '0.6s'}} />
                            <div className="h-3.5 bg-slate-50 rounded-full w-[65%] animate-pulse" style={{animationDelay: '0.7s'}} />
                          </div>
                        </div>
                      )}
                      {!aiAnswer && !isStreaming && (
                        <EmptyState text="AI 未返回结果，请检查后端 LLM 配置" />
                      )}
                      {aiAnswer && (
                        <div className="ai-markdown-body text-[17px] text-slate-700 leading-relaxed px-1">
                          <AIAnswerRenderer
                            answer={aiAnswer}
                            literatureResults={literatureResults}
                            imageResults={literatureImages.map((img: ImageResultItem) => ({
                              imagePath: img.image_path,
                              caption: img.caption || "",
                              paperName: img.paper_name,
                              headerPath: img.header_path,
                            }))}
                            experimentalImageResults={experimentalImages.map((img: ImageResultItem) => ({
                              imagePath: img.image_path,
                              caption: img.caption || "",
                              paperName: img.paper_name,
                              headerPath: img.header_path,
                            }))}
                            fallbackPrefix={adviceModeEnabled ? "成分标准" : "文献"}
                          />
                        </div>
                      )}
                    </div>
                    {/* 工艺寻优按钮 - AI解答右下角 */}
                    {data?.advice_mode === "composition" && adviceStandardRecords.length > 0 && aiAnswer && !isStreaming && (
                      <div className="flex justify-end mt-2 shrink-0">
                        <button
                          onClick={handleOpenOptimizer}
                          className="flex items-center gap-2 px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-700 transition-colors text-base font-medium shadow-sm"
                        >
                          <Settings2 className="w-5 h-5" />
                          工艺寻优
                        </button>
                      </div>
                    )}
                  </>
                )}

                {/* ===== RAG 检索结果视图 ===== */}
                {(resultView === "rag" || !isAIMode) && (
                  <>
                {/* Tabs 和操作按钮 */}
                <div className="flex items-center justify-between mb-3 shrink-0 max-md:flex-wrap max-md:gap-2">
                  {/* 左侧: 返回AI按钮 + Tabs */}
                  <div className="flex items-center gap-2 max-md:w-full max-md:overflow-x-auto max-md:pb-1">
                    {isAIMode && (
                      <button
                        onClick={() => setResultView("ai")}
                        className="flex h-9 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg border border-indigo-200 bg-gradient-to-r from-indigo-50 to-purple-50 px-4 text-base font-medium text-indigo-600 transition-all duration-200 hover:from-indigo-100 hover:to-purple-100 hover:shadow-sm max-md:px-2.5 max-md:text-sm"
                      >
                        <ArrowLeft size={15} className="shrink-0" />
                        <BotMessageSquare size={15} className="shrink-0 max-md:hidden" />
                        AI 解答
                      </button>
                    )}
                    <div className="flex items-center gap-1 rounded-lg border border-slate-200 bg-slate-50 p-1 max-md:w-max max-md:shrink-0">
                    {visibleTabs.map((tab: ResultTab) => {
                      const Icon = tab.icon;
                      const isActive = activeTab === tab.id;
                      const count =
                        tab.id === "production"
                          ? productionRecords.length
                          : tab.id === "standard"
                          ? adviceStandardRecords.length
                          : tab.id === "literature"
                          ? literatureResults.length
                          : tab.id === "litImages"
                            ? literatureImages.length
                            : experimentalImages.length;
                      return (
                        <button
                          key={tab.id}
                          onClick={() => handleTabChange(tab.id)}
                          className={`flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-3 py-1.5 text-base font-medium transition-all duration-200 max-md:px-2 max-md:text-sm ${
                            isActive
                              ? "bg-white text-slate-900 shadow-sm border border-slate-200"
                              : "text-slate-500 hover:text-slate-700 hover:bg-white/50"
                          }`}
                        >
                          <Icon size={16} />
                          {tab.label}
                          <span
                            className={`ml-1 rounded-full px-1.5 py-0.5 text-xs font-semibold ${
                              isActive
                                ? "bg-indigo-100 text-indigo-600"
                                : "bg-slate-200 text-slate-500"
                            }`}
                          >
                            {count}
                          </span>
                        </button>
                      );
                    })}
                    </div>
                  </div>
                  {/* 标签页右侧操作按钮 */}
                  <div className="flex items-center gap-2">
                    <button
                      onClick={openAdvancedFilter}
                      disabled={!includeProduction}
                      className={`flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md border px-2.5 text-sm font-medium transition-all duration-200 disabled:opacity-40 disabled:cursor-not-allowed ${
                        (steelMark || steelGrade)
                          ? "border-indigo-300 bg-indigo-50 text-indigo-700"
                          : "border-slate-200 bg-white text-slate-600 hover:border-slate-300 hover:text-slate-900"
                      }`}
                    >
                      <Filter size={16} className="w-4 h-4" />
                      高级筛选
                      {(steelMark || steelGrade) && (
                        <span className="ml-0.5 h-1.5 w-1.5 rounded-full bg-indigo-500" />
                      )}
                    </button>
                    <button
                      onClick={handleExport}
                      disabled={exporting || !data?.success || !includeProduction || productionRecords.length === 0}
                      className="flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md border border-slate-200 bg-white px-2.5 text-sm font-medium text-slate-600 transition-all duration-200 hover:border-slate-300 hover:text-slate-900 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {exporting ? <Loader2 size={16} className="animate-spin w-4 h-4" /> : <Download size={16} className="w-4 h-4" />}
                      {exporting ? "导出中" : "导出数据"}
                    </button>
                  </div>
                </div>

                  {/* ---- TAB CONTENT (flex-1 占据剩余空间，内部滚动) ---- */}
                  <div ref={resultPaneRef} onScroll={handleResultPaneScroll} className="flex-1 min-h-0 overflow-auto pr-1">
                  {/* ---------- Tab 0: Production records ---------- */}
                  {activeTab === "production" && includeProduction && (
                    <div className="animate-in">
                      {productionRecords.length === 0 ? (
                        <EmptyState text="未找到符合条件的生产数据" />
                      ) : (
                        <div>
                          {totalProductionCount > 50 && (
                            <div className="mb-3 rounded-lg border border-amber-100 bg-amber-50 px-3 py-2 text-base text-amber-700">
                              当前仅展示前 50 条生产数据，实际共 {totalProductionCount} 条。
                            </div>
                          )}
                          <table className="min-w-full text-base whitespace-nowrap">
                            <thead className="sticky top-0 bg-slate-100 text-slate-600 z-10 border-b border-slate-200">
                              <tr>
                                {productionColumns.filter((c: string) => c !== "created_at").map((col: string) => (
                                  <th
                                    key={col}
                                    className="px-4 py-2.5 text-center font-semibold"
                                  >
                                    {col}
                                  </th>
                                ))}
                              </tr>
                            </thead>
                            <tbody>
                              {displayedProductionRecords.map((row: RecordRow, idx: number) => (
                                <tr key={idx} className="border-t border-slate-200 hover:bg-slate-50">
                                  {productionColumns.filter((c: string) => c !== "created_at").map((col: string) => (
                                    <td key={col} className="px-4 py-2.5 text-center">
                                      {row[col] ?? ""}
                                    </td>
                                  ))}
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      )}
                    </div>
                  )}

                  {/* ---------- Tab: Standard data (composition / process) ---------- */}
                  {activeTab === "standard" && (
                    <div className="animate-in">
                      {adviceStandardColumns.length === 0 ? (
                        <EmptyState text="未找到对应的标准数据" />
                      ) : (
                        <table className="min-w-full text-base whitespace-nowrap">
                          <thead className="sticky top-0 z-10">
                            <tr>
                              {adviceStandardColumns.filter((c: string) => c !== "created_at").map((col: string) => (
                                <th key={col} className={`px-4 py-2.5 text-center font-semibold border-b ${
                                  data.advice_mode === "composition"
                                    ? "bg-indigo-50 text-indigo-700 border-indigo-200"
                                    : "bg-teal-50 text-teal-700 border-teal-200"
                                }`}>
                                  {col}
                                </th>
                              ))}
                            </tr>
                          </thead>
                          <tbody>
                            {adviceStandardRecords.map((stdRow: RecordRow | null, idx: number) => (
                              <tr key={idx} className={`border-t ${
                                data.advice_mode === "composition"
                                  ? "border-indigo-100 hover:bg-indigo-50/40"
                                  : "border-teal-100 hover:bg-teal-50/40"
                              }`}>
                                {adviceStandardColumns.filter((c: string) => c !== "created_at").map((col: string) => (
                                  <td key={col} className="px-4 py-2.5 text-center text-slate-700">
                                    {stdRow ? String(stdRow[col] ?? "") : "—"}
                                  </td>
                                ))}
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      )}
                    </div>
                  )}

                  {/* ---------- Tab 1: Literature text ---------- */}
                  {activeTab === "literature" && (
                    <div className="space-y-3 animate-in">
                      {literatureResults.length === 0 && (
                        <EmptyState text="未找到相关文献片段" />
                      )}
                      {literatureResults.map((lit: LiteratureResultItem, idx: number) => {
                        const score = scorePercent(lit.similarity_score);
                        const scoreColor = score >= 10 ? "emerald" : score >= 5 ? "indigo" : "slate";
                        const scoreColorMap: Record<string, {bg: string; text: string; border: string; dot: string}> = {
                          emerald: { bg: "bg-emerald-50", text: "text-emerald-600", border: "border-emerald-100", dot: "bg-emerald-500" },
                          indigo: { bg: "bg-indigo-50", text: "text-indigo-600", border: "border-indigo-100", dot: "bg-indigo-500" },
                          slate: { bg: "bg-slate-50", text: "text-slate-500", border: "border-slate-200", dot: "bg-slate-400" },
                        };
                        const sc = scoreColorMap[scoreColor];
                        return (
                        <div
                          key={idx}
                          className="group relative rounded-xl border border-slate-200 bg-white p-4 shadow-sm transition-all duration-300 hover:-translate-y-0.5 hover:shadow-lg hover:border-slate-300 overflow-hidden"
                        >
                          {/* 左侧彩色条 */}
                          <div className={`absolute left-0 top-0 bottom-0 w-1 ${sc.dot} rounded-l-xl opacity-60 group-hover:opacity-100 transition-opacity`} />
                          <div className="pl-3">
                            <div className="flex items-start justify-between gap-4 mb-2">
                              <div className="flex-1 min-w-0">
                                <div className="flex items-center gap-2 mb-1">
                                  <FileText size={14} className="text-slate-400 shrink-0" />
                                  <h3 className="text-base font-semibold text-slate-900 truncate">
                                    {renderHighlighted(lit.paper_name, query)}
                                  </h3>
                                </div>
                                <p className="text-sm text-slate-500 truncate pl-5">
                                  {renderHighlighted(lit.header_path, query)}
                                </p>
                              </div>
                              <div className={`shrink-0 flex items-center gap-1.5 rounded-full ${sc.bg} px-2.5 py-1 border ${sc.border}`}>
                                <div className={`h-1.5 w-1.5 rounded-full ${sc.dot}`} />
                                <span className={`text-xs font-mono font-semibold ${sc.text}`}>
                                  {score.toFixed(1)}%
                                </span>
                              </div>
                            </div>
                            <p className="text-sm leading-relaxed text-slate-600 line-clamp-4 pl-5">
                              {renderHighlighted(lit.content, query)}
                            </p>
                          </div>
                        </div>
                        );
                      })}
                    </div>
                  )}

                  {/* ---------- Tab 2: Literature images ---------- */}
                  {activeTab === "litImages" && (
                    <div className="animate-in">
                      {literatureImages.length === 0 && (
                        <EmptyState text="未找到相关文献配图" />
                      )}
                      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
                        {literatureImages.map((img: ImageResultItem, idx: number) => (
                          <div
                            key={idx}
                            className="group relative overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm transition-all duration-300 hover:-translate-y-0.5 hover:shadow-lg hover:border-slate-300"
                          >
                            <div className="relative aspect-[4/3] overflow-hidden bg-slate-100">
                              <img
                                src={proxyImg(img.image_path)}
                                alt={img.caption || "文献配图"}
                                className="h-full w-full object-cover transition-transform duration-500 group-hover:scale-105"
                                loading="lazy"
                              />
                              <button
                                onClick={() => setLightboxSrc(proxyImg(img.image_path))}
                                className="absolute inset-0 flex items-center justify-center bg-slate-900/0 group-hover:bg-slate-900/40 transition-colors duration-300"
                              >
                                <ZoomIn
                                  size={24}
                                  className="text-white opacity-0 group-hover:opacity-100 transition-opacity duration-300 drop-shadow-lg"
                                />
                              </button>
                            </div>
                            <div className="p-3 space-y-1.5">
                              <div className="flex items-start justify-between gap-2">
                                {img.paper_name && (
                                  <h4 className="text-sm font-bold text-slate-900 truncate flex-1">
                                    {renderHighlighted(img.paper_name, query)}
                                  </h4>
                                )}
                                <div className="shrink-0 flex items-center gap-1 rounded-full bg-cyan-50 px-2 py-0.5 border border-cyan-100">
                                  <div className="h-1.5 w-1.5 rounded-full bg-cyan-500" />
                                  <span className="text-xs font-mono text-cyan-600">
                                    {scorePercent(img.similarity_score).toFixed(1)}%
                                  </span>
                                </div>
                              </div>
                              {img.header_path && (
                                <p className="text-sm text-slate-500 truncate">
                                  {renderHighlighted(img.header_path, query)}
                                </p>
                              )}
                              {img.caption && (
                                <p className="text-sm text-slate-600 mt-1 line-clamp-2">
                                  {renderHighlighted(img.caption, query)}
                                </p>
                              )}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* ---------- Tab 3: Experimental photos ---------- */}
                  {activeTab === "expImages" && (
                    <div className="animate-in">
                      {experimentalImages.length === 0 && (
                        <EmptyState text="未找到相关金相照片" />
                      )}
                      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
                        {experimentalImages.map((img: ImageResultItem, idx: number) => (
                          <div
                            key={idx}
                            className="group relative overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm transition-all duration-300 hover:-translate-y-0.5 hover:shadow-lg hover:border-slate-300"
                          >
                            <div className="relative aspect-square overflow-hidden bg-slate-100">
                              <img
                                src={proxyImg(img.image_path)}
                                alt={img.description || "金相照片"}
                                className="h-full w-full object-cover transition-transform duration-500 group-hover:scale-105"
                                loading="lazy"
                              />
                              <button
                                onClick={() => setLightboxSrc(proxyImg(img.image_path))}
                                className="absolute inset-0 flex items-center justify-center bg-slate-900/0 group-hover:bg-slate-900/40 transition-colors duration-300"
                              >
                                <ZoomIn
                                  size={24}
                                  className="text-white opacity-0 group-hover:opacity-100 transition-opacity duration-300 drop-shadow-lg"
                                />
                              </button>
                            </div>
                            <div className="p-3 space-y-1.5">
                              <div className="flex items-start justify-between gap-2">
                                {img.caption && (
                                  <h4 className="text-sm font-semibold text-slate-900 truncate flex-1">
                                    {img.caption}
                                  </h4>
                                )}
                                <div className="shrink-0 flex items-center gap-1 rounded-full bg-emerald-50 px-2 py-0.5 border border-emerald-100">
                                  <div className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                                  <span className="text-xs font-mono text-emerald-600">
                                    {scorePercent(img.similarity_score).toFixed(1)}%
                                  </span>
                                </div>
                              </div>
                              {img.description && (
                                <p className="text-sm text-slate-600 line-clamp-3">
                                  {img.description}
                                </p>
                              )}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                  </div>
                  </>
                )}

              </div>
            </section>
  );
}
