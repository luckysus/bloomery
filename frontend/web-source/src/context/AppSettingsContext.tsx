import { createContext, useContext, useMemo, type ReactNode } from "react";

type AppSettingsContextValue = {
  appName: string;
  retrievalTitle: string;
  agentTitle: string;
};

const AppSettingsContext = createContext<AppSettingsContextValue | null>(null);

export function AppSettingsProvider({ children }: { children: ReactNode }) {
  const value = useMemo<AppSettingsContextValue>(() => ({
    appName: "钢铁智能体",
    retrievalTitle: "多模态智能检索",
    agentTitle: "钢铁智能体",
  }), []);

  return <AppSettingsContext.Provider value={value}>{children}</AppSettingsContext.Provider>;
}

export function useAppSettings() {
  const settings = useContext(AppSettingsContext);
  if (!settings) {
    throw new Error("useAppSettings must be used within AppSettingsProvider");
  }
  return settings;
}
