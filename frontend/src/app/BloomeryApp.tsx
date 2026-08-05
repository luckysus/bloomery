import { useEffect, useState } from "react";
import { BookOpen, Database, MessageSquareText, Settings } from "lucide-react";
import { desktop } from "../bridge/desktop";

type InitializationState = "loading" | "ready" | "failed";

const sections = [
  { id: "workbench", label: "工作台", icon: Database },
  { id: "chat", label: "对话", icon: MessageSquareText },
  { id: "knowledge", label: "知识库", icon: BookOpen },
  { id: "settings", label: "设置", icon: Settings },
] as const;

export default function BloomeryApp() {
  const [state, setState] = useState<InitializationState>("loading");

  useEffect(() => {
    desktop.initialize().then(
      () => setState("ready"),
      () => setState("failed"),
    );
  }, []);

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100">
      <header className="flex h-14 items-center border-b border-slate-800 px-5">
        <strong className="text-base font-semibold">Bloomery</strong>
        <span className="ml-3 text-xs text-slate-500">钢铁领域智能体工作台</span>
      </header>
      <div className="grid min-h-[calc(100vh-3.5rem)] grid-cols-[13rem_1fr]">
        <nav className="border-r border-slate-800 p-3" aria-label="主导航">
          {sections.map(({ id, label, icon: Icon }, index) => (
            <button
              key={id}
              type="button"
              className={`mb-1 flex h-10 w-full items-center gap-3 rounded-md px-3 text-left text-sm ${
                index === 0 ? "bg-slate-800 text-white" : "text-slate-400 hover:bg-slate-900"
              }`}
            >
              <Icon size={17} aria-hidden="true" />
              {label}
            </button>
          ))}
        </nav>
        <main className="p-6" aria-label="工作台">
          <h1 className="text-xl font-semibold">工作台</h1>
          {state === "loading" && <p className="mt-4 text-sm text-slate-400">正在初始化本地数据...</p>}
          {state === "failed" && <p className="mt-4 text-sm text-red-400">本地数据初始化失败。</p>}
          {state === "ready" && <p className="mt-4 text-sm text-slate-400">尚未建立知识库。</p>}
        </main>
      </div>
    </div>
  );
}
