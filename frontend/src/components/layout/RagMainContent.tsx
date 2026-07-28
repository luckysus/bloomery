import {
  BotMessageSquare,
  BookOpen,
  Brain,
  Loader2,
  Menu,
  Search,
  Settings2,
  Sparkles,
  TrendingUp,
} from "lucide-react";
import AgentPage from "../../pages/AgentPage";
import SearchPage from "../../pages/SearchPage";

type RagMainContentProps = Record<string, any>;

export default function RagMainContent(props: RagMainContentProps) {
  const {
    isAgentMode,
    agentSidebarCollapsed,
    onOpenSidebar,
    openKnowledgeWizard,
    setTrainingEntrySource,
    setShowTraining,
    handleOpenOptimizer,
    optimizing,
    query,
    setQuery,
    handleSearch,
    loading,
    coilMatchLoading,
    includeProduction,
    adviceModeEnabled,
    isCoilMatchMode,
    isAIMode,
    handleAIModeToggle,
    handleCompositionModeToggle,
    handleCoilMatchModeToggle,
    isCompositionMode,
  } = props;

  return (
    <main className={`flex-1 flex flex-col h-full overflow-hidden ${isAgentMode ? "bg-[#fbf7ef]" : ""}`}>
      <section
        className={`pt-4 pb-3 shrink-0 transition-[padding] duration-300 ${
          isAgentMode ? "border-b border-[#eadfd2] bg-[#fbf7ef]/95" : ""
        } ${isAgentMode && agentSidebarCollapsed ? "pl-8 pr-6" : "px-6"} max-md:px-3`}
      >
        <div className="mb-3 flex items-center justify-between md:justify-end">
          <button
            type="button"
            onClick={onOpenSidebar}
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-[#eadfd2] bg-white/80 text-[#6f6258] transition-colors hover:bg-[#fffaf3] md:hidden"
            aria-label="打开侧栏"
            title="打开侧栏"
          >
            <Menu size={20} />
          </button>
          <div className="flex items-center gap-2 max-md:gap-1">
            <button
              onClick={openKnowledgeWizard}
              className="flex items-center gap-2 rounded-xl px-3.5 py-2 text-lg text-[#6f6258] transition-colors hover:bg-[#fffaf3] hover:text-[#cc785c] max-md:px-2.5"
            >
              <BookOpen className="w-6 h-6" />
              <span className="max-md:hidden">知识库</span>
            </button>
            <button
              onClick={() => {
                setTrainingEntrySource("main");
                setShowTraining(true);
              }}
              className="flex items-center gap-2 rounded-xl px-3.5 py-2 text-lg text-[#6f6258] transition-colors hover:bg-[#fffaf3] hover:text-[#cc785c] max-md:px-2.5"
            >
              <Brain className="w-6 h-6" />
              <span className="max-md:hidden">模型训练</span>
            </button>
            <button
              onClick={handleOpenOptimizer}
              className={`flex items-center gap-2 px-3.5 py-2 text-lg rounded-lg transition-colors max-md:px-2.5 ${
                optimizing
                  ? "bg-[#fffaf3] text-[#cc785c]"
                  : "text-[#6f6258] hover:bg-[#fffaf3] hover:text-[#cc785c]"
              }`}
            >
              <Settings2 className={`w-6 h-6 ${optimizing ? "animate-spin" : ""}`} />
              <span className="max-md:hidden">{optimizing ? "优化中" : "工艺优化"}</span>
            </button>
          </div>
        </div>

        {!isAgentMode && (
          <>
            <div className="relative">
              <div className="relative flex items-center gap-3 rounded-xl border border-slate-200 bg-white shadow-sm p-2 transition-shadow duration-200 focus-within:shadow-md focus-within:border-indigo-300 focus-within:ring-4 focus-within:ring-indigo-50">
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-slate-50">
                  <Search size={16} className="text-slate-400" />
                </div>
                <input
                  type="text"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      handleSearch();
                    }
                  }}
                  placeholder="例如：钢铁材料中析出强化对屈服强度的影响机理"
                  className="min-w-0 flex-1 bg-transparent text-lg text-slate-900 placeholder:text-slate-400 outline-none max-md:text-base"
                />
                <button
                  onClick={handleSearch}
                  disabled={loading || coilMatchLoading || (!query.trim() && !includeProduction && !adviceModeEnabled && !isCoilMatchMode)}
                  className="flex h-10 shrink-0 items-center gap-2 rounded-lg bg-indigo-600 px-5 text-base font-semibold text-white shadow-md shadow-indigo-200 transition-all duration-200 hover:bg-indigo-700 hover:shadow-lg hover:shadow-indigo-200 disabled:opacity-40 disabled:cursor-not-allowed max-md:px-3"
                >
                  {loading ? <Loader2 size={14} className="animate-spin" /> : <Sparkles size={14} />}
                  {loading ? "检索中" : "检索"}
                </button>
              </div>
            </div>
            <p className="mt-1.5 text-sm text-slate-400 text-right max-md:hidden">Ctrl + Enter 快速检索</p>
          </>
        )}

        {!isAgentMode && (
          <div className="flex items-center gap-3 mt-2 max-md:gap-2 max-md:overflow-x-auto max-md:pb-1">
            <button
              onClick={handleAIModeToggle}
              disabled={adviceModeEnabled}
              className={`flex items-center gap-2 rounded-lg px-3.5 py-2 text-base font-medium transition-colors duration-300 whitespace-nowrap max-md:px-2 max-md:py-1.5 max-md:text-xs max-md:gap-1 ${
                isAIMode
                  ? "bg-gradient-to-r from-indigo-500 via-purple-500 to-pink-500 text-white shadow-md shadow-indigo-200/50"
                  : "bg-slate-100 text-slate-500 hover:bg-slate-200 hover:text-slate-700"
              } ${adviceModeEnabled ? "cursor-not-allowed opacity-80" : ""}`}
              title={adviceModeEnabled ? "选择成分建议后，智能深度回答会自动开启且不可关闭" : undefined}
            >
              <BotMessageSquare size={16} />
              <span>智能深度问答</span>
              <span
                className={`ml-0.5 inline-flex items-center justify-center w-[32px] rounded-md py-0.5 text-xs font-bold uppercase tracking-wide max-md:ml-0 max-md:w-[26px] max-md:text-[10px] ${
                  isAIMode ? "bg-white/20 text-white" : "bg-slate-200 text-slate-500"
                }`}
              >
                {isAIMode ? "ON" : "OFF"}
              </span>
            </button>
            <button
              onClick={handleCompositionModeToggle}
              className={`flex items-center gap-2 rounded-lg px-3.5 py-2 text-base font-medium transition-colors duration-300 whitespace-nowrap max-md:px-2 max-md:py-1.5 max-md:text-xs max-md:gap-1 ${
                isCompositionMode
                  ? "bg-gradient-to-r from-indigo-500 via-purple-500 to-pink-500 text-white shadow-md shadow-indigo-200/50"
                  : "bg-slate-100 text-slate-500 hover:bg-slate-200 hover:text-slate-700"
              }`}
            >
              <BotMessageSquare size={16} />
              <span>成分建议</span>
              <span
                className={`ml-0.5 inline-flex items-center justify-center w-[32px] rounded-md py-0.5 text-xs font-bold uppercase tracking-wide max-md:ml-0 max-md:w-[26px] max-md:text-[10px] ${
                  isCompositionMode ? "bg-white/20 text-white" : "bg-slate-200 text-slate-500"
                }`}
              >
                {isCompositionMode ? "ON" : "OFF"}
              </span>
            </button>
            <button
              onClick={handleCoilMatchModeToggle}
              className={`flex items-center gap-2 rounded-lg px-3.5 py-2 text-base font-medium transition-colors duration-300 whitespace-nowrap max-md:px-2 max-md:py-1.5 max-md:text-xs max-md:gap-1 ${
                isCoilMatchMode
                  ? "bg-[#cc785c] text-white shadow-md shadow-[#d8c9ba]/70 hover:bg-[#b8664d]"
                  : "bg-slate-100 text-slate-500 hover:bg-slate-200 hover:text-slate-700"
              }`}
            >
              <TrendingUp size={16} />
              <span>钢卷匹配</span>
              <span
                className={`ml-0.5 inline-flex items-center justify-center w-[32px] rounded-md py-0.5 text-xs font-bold uppercase tracking-wide max-md:ml-0 max-md:w-[26px] max-md:text-[10px] ${
                  isCoilMatchMode ? "bg-white/20 text-white" : "bg-slate-200 text-slate-500"
                }`}
              >
                {isCoilMatchMode ? "ON" : "OFF"}
              </span>
            </button>
          </div>
        )}
      </section>

      <SearchPage {...props} />
      <AgentPage {...props} />
    </main>
  );
}

