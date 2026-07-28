import { ArrowRight, Atom, BotMessageSquare, Database, Search, Sparkles } from "lucide-react";

interface ModeSelectPageProps {
  onSelectRetrieval: () => void;
  onSelectAgent: () => void;
  onLogout: () => void;
}

export default function ModeSelectPage({ onSelectRetrieval, onSelectAgent, onLogout }: ModeSelectPageProps) {
  return (
    <div className="relative h-[100dvh] overflow-hidden bg-[#f5f0e8] text-[#2b2118]">
      <svg className="pointer-events-none fixed inset-0 z-[1] h-full w-full opacity-[0.035]" xmlns="http://www.w3.org/2000/svg">
        <filter id="noise-mode">
          <feTurbulence type="fractalNoise" baseFrequency="0.8" numOctaves="4" stitchTiles="stitch" />
        </filter>
        <rect width="100%" height="100%" filter="url(#noise-mode)" />
      </svg>

      <header className="relative z-10 flex items-center justify-between px-8 py-5">
        <div className="flex items-center gap-3">
          <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-[#f5f0e8] shadow-neu-card">
            <Atom size={22} className="text-[#cc785c]" />
          </div>
          <div className="leading-tight">
            <div className="font-sans text-xl font-bold tracking-tight text-[#2b2118]">钢铁智能体</div>
            <div className="font-mono text-xs uppercase tracking-[0.1em] text-[#6f6258]">多模态检索与工艺决策平台</div>
          </div>
        </div>
        <button
          onClick={onLogout}
          className="rounded-lg bg-[#f5f0e8] px-4 py-2 font-mono text-xs uppercase tracking-wider text-[#6f6258] shadow-neu-card transition-all hover:text-[#7e422f] active:translate-y-[2px] active:shadow-neu-pressed"
        >
          退出登录
        </button>
      </header>

      <main className="relative z-10 flex h-[calc(100dvh-82px)] items-center justify-center overflow-y-auto px-6 max-md:items-start max-md:px-4 max-md:py-6">
        <div className="grid w-full max-w-5xl grid-cols-1 gap-8 md:grid-cols-2">
          <button
            onClick={onSelectRetrieval}
            className="group min-h-[245px] rounded-2xl bg-[#f5f0e8] px-7 py-7 text-left shadow-neu-card transition-all duration-200 hover:-translate-y-1 hover:shadow-neu-float focus:outline-none focus:ring-4 focus:ring-[#d58a6e]/15"
          >
            <div className="mb-5 flex h-12 w-12 items-center justify-center rounded-xl bg-[#f5f0e8] shadow-neu-pressed">
              <Search size={24} className="text-[#cc785c]" />
            </div>
            <div className="font-sans text-2xl font-bold text-[#2b2118]">检索模式</div>
            <p className="mt-3 text-base leading-relaxed text-[#6f6258]">
              进入原本的多模态检索、生产数据、成分建议、工艺优化、模型训练等完整工作台。
            </p>
            <div className="mt-6 flex items-center justify-between">
              <div className="flex items-center gap-2 font-mono text-xs uppercase tracking-[0.1em] text-[#6f6258]">
                <Database size={16} />
                保留原有使用体验
              </div>
              <ArrowRight size={18} className="text-[#d8c9ba] transition group-hover:translate-x-1 group-hover:text-[#cc785c]" />
            </div>
          </button>

          <button
            onClick={onSelectAgent}
            className="group min-h-[245px] rounded-2xl bg-[#f5f0e8] px-7 py-7 text-left shadow-neu-card transition-all duration-200 hover:-translate-y-1 hover:shadow-neu-float focus:outline-none focus:ring-4 focus:ring-[#d58a6e]/15"
          >
            <div className="mb-5 flex h-12 w-12 items-center justify-center rounded-xl bg-[#f5f0e8] shadow-neu-pressed">
              <BotMessageSquare size={24} className="text-[#cc785c]" />
            </div>
            <div className="font-sans text-2xl font-bold text-[#2b2118]">智能体模式</div>
            <p className="mt-3 text-base leading-relaxed text-[#6f6258]">
              单独进入钢铁智能体页面，让系统自动规划、调用工具、汇总证据和比较方案。
            </p>
            <div className="mt-6 flex items-center justify-between">
              <div className="flex items-center gap-2 font-mono text-xs uppercase tracking-[0.1em] text-[#6f6258]">
                <Sparkles size={16} />
                独立验证新能力
              </div>
              <ArrowRight size={18} className="text-[#d8c9ba] transition group-hover:translate-x-1 group-hover:text-[#cc785c]" />
            </div>
          </button>
        </div>
      </main>
    </div>
  );
}
