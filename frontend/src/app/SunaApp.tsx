import { useEffect, useRef, useState } from "react";
import { Activity, Archive, CircleUserRound, Factory, PanelLeftClose, PanelLeftOpen, Plus, Search, SlidersHorizontal } from "lucide-react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import ChatPage from "../features/chat/ChatPage";
import DiagnosticsPage from "../features/diagnostics/DiagnosticsPage";
import SettingsPage from "../features/settings/SettingsPage";
import {
  getNavigationSection,
  type SectionId,
} from "./navigation";
import { LocaleProvider, useLocale } from "../i18n/locale";
import { ThemeProvider } from "../theme/theme";

export default function SunaApp() {
  return (
    <LocaleProvider>
      <ThemeProvider>
        <SunaAppShell />
      </ThemeProvider>
    </LocaleProvider>
  );
}

function SunaAppShell() {
  const [activeSection, setActiveSection] = useState<SectionId>("chat");
  const [collapsed, setCollapsed] = useState(false);
  const initializationRef = useRef<Promise<void> | null>(null);
  const active = getNavigationSection(activeSection);
  const { t } = useLocale();

  useEffect(() => {
    let mounted = true;
    const initialization = initializationRef.current ?? (initializationRef.current = desktop.initialize());
    initialization.then(() => {
      if (!mounted) return;
      if (!isDesktopRuntime()) return;
    }, () => {
      if (initializationRef.current === initialization) initializationRef.current = null;
    });
    return () => {
      mounted = false;
    };
  }, []);

  return (
    <div className={`suna-app ${collapsed ? "is-collapsed" : ""}`}>
      <header className="suna-topbar">
        <div className="suna-brand-lockup">
          <div className="suna-brand-mark" aria-hidden="true">
            <Factory size={17} />
          </div>
          {!collapsed && (
            <div className="suna-brand-copy">
              <strong>SUNA</strong>
            </div>
          )}
        </div>
        {!collapsed && (
          <div className="suna-topbar-context" aria-live="polite">
            <span className="suna-eyebrow">{activeSection === "chat" ? "AGENT WORKSPACE" : t("workspace")}</span>
            <strong>{t(active.labelKey)}</strong>
          </div>
        )}
        <div className="suna-topbar-meta">
          <span className="suna-local-indicator">
            <span className="suna-state-dot" aria-hidden="true" />
            {t("localAgent")}
          </span>
        </div>
      </header>

      <div className="suna-body">
        <nav className="suna-sidebar" aria-label={t("mainNavigation")}>
          <div className="suna-sidebar-head">
            {!collapsed && <span className="suna-sidebar-caption">{t("workspace")}</span>}
            <button
              type="button"
              className="suna-icon-button"
              aria-label={collapsed ? t("expandSidebar") : t("collapseSidebar")}
              title={collapsed ? t("expandSidebar") : t("collapseSidebar")}
              onClick={() => setCollapsed((value) => !value)}
            >
              {collapsed ? <PanelLeftOpen size={17} aria-hidden="true" /> : <PanelLeftClose size={17} aria-hidden="true" />}
            </button>
          </div>
          <div className="suna-sidebar-actions" aria-label={t("agentActions")}>
            <button type="button" className="suna-sidebar-action is-primary" onClick={() => setActiveSection("chat")} aria-label={t("newTask")} title={collapsed ? t("newTask") : undefined}><Plus size={17} aria-hidden="true" />{!collapsed && <span>{t("newTask")}</span>}</button>
            <button type="button" className="suna-sidebar-action" onClick={() => setActiveSection("chat")} aria-label={t("searchWorkspace")} title={collapsed ? t("searchWorkspace") : undefined}><Search size={16} aria-hidden="true" />{!collapsed && <span>{t("searchWorkspace")}</span>}{!collapsed && <kbd>Ctrl K</kbd>}</button>
          </div>
          <button type="button" className="suna-sidebar-workspace" onClick={() => setActiveSection("chat")} aria-label={t("currentWorkspace")} title={collapsed ? t("currentWorkspace") : undefined}><span className="suna-workspace-mark">S</span>{!collapsed && <span><strong>{t("localWorkspace")}</strong><small>{t("workspaceStatus")}</small></span>}{!collapsed && <SlidersHorizontal size={14} aria-hidden="true" />}</button>
          {!collapsed && <span className="suna-sidebar-group-label">{t("recentConversations")}</span>}
          <div className="suna-sidebar-sessions" aria-label={t("recentConversations")}>
            <button type="button" className="suna-sidebar-session is-active" onClick={() => setActiveSection("chat")}><span className="suna-session-dot" aria-hidden="true" />{!collapsed && <span>{t("newConversation")}</span>}</button>
            {!collapsed && <div className="suna-sidebar-empty">{t("conversationEmpty")}</div>}
          </div>
          <div className="suna-sidebar-footer" data-testid="utility-navigation">
            {!collapsed && <div className="suna-sidebar-runtime"><Activity size={14} aria-hidden="true" /><span>{t("localAgent")}</span><i aria-hidden="true" /></div>}
            <button type="button" className="suna-sidebar-utility" aria-label={t("taskCenter")} title={collapsed ? t("taskCenter") : undefined}><Archive size={16} aria-hidden="true" />{!collapsed && <span>{t("taskCenter")}</span>}</button>
            <button type="button" className="suna-sidebar-utility" aria-label={t("navSettings")} title={collapsed ? t("navSettings") : undefined} onClick={() => setActiveSection("settings")}><SlidersHorizontal size={16} aria-hidden="true" />{!collapsed && <span>{t("navSettings")}</span>}</button>
            <button
              type="button"
              className="suna-account-entry"
              aria-label={t("accountEntry")}
              title={collapsed ? t("accountEntry") : undefined}
              onClick={() => setActiveSection("settings")}
            >
              <span className="suna-account-avatar" aria-hidden="true">S</span>
              {!collapsed && (
                <span className="suna-account-copy">
                  <strong>Suna</strong>
                  <small>{t("localAccount")}</small>
                </span>
              )}
              {!collapsed && <CircleUserRound size={16} aria-hidden="true" />}
            </button>
          </div>
        </nav>

        <main className="suna-main" aria-label={t(active.labelKey)}>
          <div className={`suna-main-inner ${activeSection === "chat" ? "is-chat-shell" : ""}`}>
            {activeSection === "chat" ? (
              <ChatPage onOpenSection={setActiveSection} />
            ) : activeSection === "settings" ? (
              <SettingsPage onOpenDiagnostics={() => setActiveSection("diagnostics")} />
            ) : activeSection === "diagnostics" ? (
              <DiagnosticsPage />
            ) : (
              <ChatPage onOpenSection={setActiveSection} />
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
