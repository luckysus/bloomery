import { useEffect, useState } from "react";
import { Brain, ListTodo, MessageSquareText, PanelRightOpen, Settings, X, type LucideIcon } from "lucide-react";
import DesktopChatPage from "./DesktopChatPage";
import DesktopMemoryPage from "./DesktopMemoryPage";
import DesktopSettingsPage from "./DesktopSettingsPage";
import DesktopTasksPage from "./DesktopTasksPage";
import { isTauriRuntime } from "./services/tauri";

type DesktopExtraTab = "localChat" | "memories" | "tasks" | "settings";

const tabs: Array<{ id: DesktopExtraTab; label: string; icon: LucideIcon }> = [
  { id: "localChat", label: "本地对话", icon: MessageSquareText },
  { id: "memories", label: "本地记忆", icon: Brain },
  { id: "tasks", label: "任务镜像", icon: ListTodo },
  { id: "settings", label: "桌面设置", icon: Settings },
];

export default function DesktopExtrasDock() {
  const [available, setAvailable] = useState(false);
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<DesktopExtraTab>("localChat");

  useEffect(() => {
    setAvailable(isTauriRuntime());
  }, []);

  if (!available) return null;

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="fixed bottom-4 right-4 z-[70] inline-flex h-11 items-center gap-2 rounded-full border border-slate-200 bg-white/95 px-4 text-sm font-semibold text-slate-700 shadow-[0_14px_40px_rgba(15,23,42,0.18)] backdrop-blur transition hover:border-slate-300 hover:bg-white"
        title="打开桌面工具"
      >
        <PanelRightOpen className="h-4 w-4 text-[#cc785c]" />
        桌面工具
      </button>

      {open && (
        <div className="fixed inset-0 z-[90] bg-slate-950/35 backdrop-blur-sm">
          <aside className="ml-auto flex h-full w-[min(1040px,calc(100vw-24px))] flex-col border-l border-slate-800 bg-slate-950 text-slate-100 shadow-2xl">
            <header className="flex h-14 shrink-0 items-center justify-between border-b border-slate-800 px-4">
              <nav className="flex min-w-0 gap-1">
                {tabs.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      onClick={() => setTab(item.id)}
                      className={`inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm ${
                        tab === item.id
                          ? "bg-cyan-500 text-slate-950"
                          : "text-slate-300 hover:bg-slate-900 hover:text-white"
                      }`}
                    >
                      <Icon className="h-4 w-4" />
                      {item.label}
                    </button>
                  );
                })}
              </nav>
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="inline-flex h-9 w-9 items-center justify-center rounded-md border border-slate-700 text-slate-300 hover:bg-slate-900 hover:text-white"
                title="关闭桌面工具"
              >
                <X className="h-4 w-4" />
              </button>
            </header>

            <div className="flex min-h-0 flex-1">
              {tab === "localChat" && <DesktopChatPage />}
              {tab === "memories" && <DesktopMemoryPage />}
              {tab === "tasks" && <DesktopTasksPage />}
              {tab === "settings" && <DesktopSettingsPage />}
            </div>
          </aside>
        </div>
      )}
    </>
  );
}
