import type { Dispatch, SetStateAction } from "react";
import { FileText, Globe, KeyRound, Mic, SearchCode, ShieldCheck, UserPlus } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export type ProfileTab = "model" | "registration" | "knowledge" | "mineru" | "retrieval" | "captcha" | "asr";

interface ProfileTabsProps {
  profileTab: ProfileTab;
  setProfileTab: Dispatch<SetStateAction<ProfileTab>>;
  isDslAdmin: boolean;
}

const PROFILE_TABS: Array<{ tab: ProfileTab; label: string; icon: LucideIcon; adminOnly: boolean }> = [
  { tab: "model", label: "模型配置", icon: KeyRound, adminOnly: false },
  { tab: "registration", label: "注册设置", icon: UserPlus, adminOnly: true },
  { tab: "knowledge", label: "知识库共享", icon: Globe, adminOnly: true },
  { tab: "mineru", label: "文献解析", icon: FileText, adminOnly: true },
  { tab: "retrieval", label: "检索模型", icon: SearchCode, adminOnly: true },
  { tab: "captcha", label: "人机验证", icon: ShieldCheck, adminOnly: true },
  { tab: "asr", label: "语音输入", icon: Mic, adminOnly: true },
];

export function ProfileTabs({ profileTab, setProfileTab, isDslAdmin }: ProfileTabsProps) {
  const visibleTabs = isDslAdmin ? PROFILE_TABS : PROFILE_TABS.filter((item) => !item.adminOnly);

  return (
    <nav className="flex w-52 shrink-0 flex-col gap-1 border-r border-slate-200 bg-slate-50/60 p-3 max-md:w-full max-md:flex-row max-md:overflow-x-auto max-md:border-r-0 max-md:border-b max-md:p-2">
      {visibleTabs.map(({ tab, label, icon: Icon }) => {
        const active = profileTab === tab;
        return (
          <button
            key={tab}
            type="button"
            onClick={() => setProfileTab(tab)}
            className={`flex h-10 items-center gap-3 rounded-lg px-3 text-base font-semibold transition-colors max-md:shrink-0 max-md:gap-2 max-md:whitespace-nowrap ${
              active ? "bg-white text-slate-900 shadow-sm" : "text-slate-500 hover:bg-white/70 hover:text-slate-800"
            }`}
          >
            <Icon size={20} className={active ? "text-indigo-600" : "text-slate-400"} />
            {label}
          </button>
        );
      })}
    </nav>
  );
}
