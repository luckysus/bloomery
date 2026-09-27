import { useEffect, useRef, useState } from "react";
import { Activity, Archive, CheckCircle2, ChevronDown, CircleUserRound, Factory, PanelLeftClose, PanelLeftOpen, Plus, Search, SlidersHorizontal, X } from "lucide-react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import ChatPage from "../features/chat/ChatPage";
import { ChatControllerProvider, useChatControllerContext } from "../features/chat/chatController";
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
        <ChatControllerProvider>
          <SunaAppShell />
        </ChatControllerProvider>
      </ThemeProvider>
    </LocaleProvider>
  );
}

function SunaAppShell() {
  const [activeSection, setActiveSection] = useState<SectionId>("chat");
  const [collapsed, setCollapsed] = useState(false);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const initializationRef = useRef<Promise<void> | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const active = getNavigationSection(activeSection);
  const { t } = useLocale();
  const chat = useChatControllerContext();
  const recentConversations = chat.conversations.filter((conversation) => !conversation.archived).slice(0, 8);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSearchOpen(true);
        setWorkspaceOpen(false);
      }
      if (event.key === "Escape") {
        setSearchOpen(false);
        setWorkspaceOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (searchOpen) searchInputRef.current?.focus();
  }, [searchOpen]);

  const openChat = () => {
    setActiveSection("chat");
    setSearchOpen(false);
    setWorkspaceOpen(false);
  };

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
            <button type="button" className="suna-sidebar-action is-primary" onClick={openChat} aria-label={t("newTask")} title={collapsed ? t("newTask") : undefined}><Plus size={17} aria-hidden="true" />{!collapsed && <span>{t("newTask")}</span>}</button>
            <button type="button" className={`suna-sidebar-action ${searchOpen ? "is-active" : ""}`} onClick={() => { if (collapsed) setCollapsed(false); setSearchOpen((value) => !value); setWorkspaceOpen(false); }} aria-label={t("searchWorkspace")} title={collapsed ? t("searchWorkspace") : undefined} aria-expanded={searchOpen}><Search size={16} aria-hidden="true" />{!collapsed && <span>{t("searchWorkspace")}</span>}{!collapsed && <kbd>Ctrl K</kbd>}</button>
          </div>
          {searchOpen && !collapsed && (
            <div className="suna-sidebar-search-panel" role="search">
              <div className="suna-sidebar-search-input">
                <Search size={14} aria-hidden="true" />
                <input ref={searchInputRef} type="search" placeholder={t("searchPlaceholder")} aria-label={t("searchWorkspace")} />
                <button type="button" aria-label={t("closeSearch")} title={t("closeSearch")} onClick={() => setSearchOpen(false)}><X size={14} aria-hidden="true" /></button>
              </div>
              <p className="suna-sidebar-search-empty">{t("searchEmpty")}</p>
            </div>
          )}
          <button type="button" className="suna-sidebar-workspace" onClick={() => { if (collapsed) setCollapsed(false); setWorkspaceOpen((value) => !value); setSearchOpen(false); }} aria-label={t("currentWorkspace")} title={collapsed ? t("currentWorkspace") : undefined} aria-expanded={workspaceOpen}><span className="suna-workspace-mark">S</span>{!collapsed && <span><strong>{t("localWorkspace")}</strong><small>{t("workspaceStatus")}</small></span>}{!collapsed && <ChevronDown className={workspaceOpen ? "is-rotated" : ""} size={14} aria-hidden="true" />}</button>
          {workspaceOpen && !collapsed && (
            <div className="suna-workspace-popover">
              <div className="suna-workspace-popover-title"><span className="suna-workspace-mark">S</span><span><strong>{t("localWorkspace")}</strong><small>{t("workspaceLocalDescription")}</small></span></div>
              <div className="suna-workspace-popover-status"><CheckCircle2 size={14} aria-hidden="true" />{t("workspaceLocalTitle")}</div>
              <button type="button" onClick={() => { setActiveSection("settings"); setWorkspaceOpen(false); }}><SlidersHorizontal size={14} aria-hidden="true" />{t("workspaceManage")}</button>
            </div>
          )}
          {!collapsed && <span className="suna-sidebar-group-label">{t("recentConversations")}</span>}
          <div className="suna-sidebar-sessions" aria-label={t("recentConversations")}>
            {collapsed ? (
              <button type="button" className="suna-sidebar-session is-active" onClick={openChat} aria-label={t("newConversation")} title={t("newConversation")}><span className="suna-session-dot" aria-hidden="true" /></button>
            ) : chat.loading ? (
              <div className="suna-sidebar-empty">{t("loading")}</div>
            ) : recentConversations.length === 0 ? (
              <div className="suna-sidebar-empty">{t("conversationEmpty")}</div>
            ) : recentConversations.map((conversation) => (
              <button type="button" key={conversation.id} className={`suna-sidebar-session ${conversation.id === chat.selectedId ? "is-active" : ""}`} onClick={() => { chat.onSelectConversation(conversation.id); openChat(); }} aria-current={conversation.id === chat.selectedId ? "page" : undefined}>
                <span className="suna-session-dot" aria-hidden="true" />
                <span className="suna-session-title">{conversation.title.trim() || t("newConversation")}</span>
                {conversation.pinned && <span className="suna-session-pin" aria-label={t("pinnedConversation")}>•</span>}
              </button>
            ))}
          </div>
          <div className="suna-sidebar-footer" data-testid="utility-navigation">
            {!collapsed && <div className="suna-sidebar-runtime"><Activity size={14} aria-hidden="true" /><span>{t("localAgent")}</span><i aria-hidden="true" /></div>}
            <button type="button" className={`suna-sidebar-utility ${activeSection === "diagnostics" ? "is-active" : ""}`} aria-label={t("taskCenter")} title={collapsed ? t("taskCenter") : undefined} onClick={() => setActiveSection("diagnostics")}><Archive size={16} aria-hidden="true" />{!collapsed && <span>{t("taskCenter")}</span>}</button>
            <button type="button" className={`suna-sidebar-utility ${activeSection === "settings" ? "is-active" : ""}`} aria-label={t("navSettings")} title={collapsed ? t("navSettings") : undefined} onClick={() => setActiveSection("settings")}><SlidersHorizontal size={16} aria-hidden="true" />{!collapsed && <span>{t("navSettings")}</span>}</button>
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
