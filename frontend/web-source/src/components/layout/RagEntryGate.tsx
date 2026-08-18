import { Loader2 } from "lucide-react";
import LoginPage from "../../LoginPage";
import ModeSelectPage from "../../ModeSelectPage";
import type { AppMode } from "../../types/rag";
import type { AuthUserInfo } from "../../LoginPage";

type RagEntryGateProps = {
  authChecked: boolean;
  isAuthenticated: boolean;
  appMode: AppMode;
  authClient?: "web";
  onLogin: (user?: AuthUserInfo) => void;
  onSelectRetrieval: () => void;
  onSelectAgent: () => void;
  onLogout: () => void;
};

export default function RagEntryGate({
  authChecked,
  isAuthenticated,
  appMode,
  authClient = "web",
  onLogin,
  onSelectRetrieval,
  onSelectAgent,
  onLogout,
}: RagEntryGateProps) {
  if (!authChecked) {
    return (
      <div className="flex h-screen items-center justify-center bg-slate-50 text-slate-500">
        <Loader2 size={22} className="mr-2 animate-spin" />
        正在验证登录状态...
      </div>
    );
  }

  if (!isAuthenticated) {
    return <LoginPage onLogin={onLogin} authClient={authClient} />;
  }

  if (appMode === "select") {
    return (
      <ModeSelectPage
        onSelectRetrieval={onSelectRetrieval}
        onSelectAgent={onSelectAgent}
        onLogout={onLogout}
      />
    );
  }

  return null;
}
