import type { Dispatch, SetStateAction } from "react";
import type { AuthUserInfo } from "../LoginPage";
import type { AppMode, SearchResponse, UserProfileInfo } from "../types/rag";

type UseRagEntryGatePropsInput = {
  authChecked: boolean;
  isAuthenticated: boolean;
  appMode: AppMode;
  markLoggedIn: (user?: AuthUserInfo) => void;
  handleLogout: () => void;
  setProfileInfo: Dispatch<SetStateAction<UserProfileInfo | null>>;
  setAppMode: Dispatch<SetStateAction<AppMode>>;
  setIsAgentMode: Dispatch<SetStateAction<boolean>>;
  setQuery: Dispatch<SetStateAction<string>>;
  setIsAIMode: Dispatch<SetStateAction<boolean>>;
  setIsCompositionMode: Dispatch<SetStateAction<boolean>>;
  setIsCoilMatchMode: Dispatch<SetStateAction<boolean>>;
  setData: Dispatch<SetStateAction<SearchResponse | null>>;
  setCoilMatchResults: Dispatch<SetStateAction<any[]>>;
  setCoilMatchError: Dispatch<SetStateAction<string>>;
};

export function useRagEntryGateProps({
  authChecked,
  isAuthenticated,
  appMode,
  markLoggedIn,
  handleLogout,
  setProfileInfo,
  setAppMode,
  setIsAgentMode,
  setQuery,
  setIsAIMode,
  setIsCompositionMode,
  setIsCoilMatchMode,
  setData,
  setCoilMatchResults,
  setCoilMatchError,
}: UseRagEntryGatePropsInput) {
  return {
    shouldShowEntryGate: !authChecked || !isAuthenticated || appMode === "select",
    entryGateProps: {
      authChecked,
      isAuthenticated,
      appMode,
      onLogin: (user?: AuthUserInfo) => {
        markLoggedIn(user);
        if (user?.username?.trim()) {
          setProfileInfo(prev => prev ? {
            ...prev,
            username: user.username.trim(),
            role: user.role || prev.role,
          } : prev);
        }
        setIsAgentMode(true);
        setAppMode("agent");
      },
      onSelectRetrieval: () => {
        setIsAgentMode(false);
        setQuery("");
        setAppMode("retrieval");
      },
      onSelectAgent: () => {
        setIsAgentMode(true);
        setQuery("");
        setIsAIMode(false);
        setIsCompositionMode(false);
        setIsCoilMatchMode(false);
        setData(null);
        setCoilMatchResults([]);
        setCoilMatchError("");
        setAppMode("agent");
      },
      onLogout: handleLogout,
    },
  };
}
