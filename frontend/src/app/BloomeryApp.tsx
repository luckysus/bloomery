import { useEffect, useRef, useState } from "react";
import { Factory, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import ChatPage from "../features/chat/ChatPage";
import AnalysisPage from "../features/analysis/AnalysisPage";
import DiagnosticsPage from "../features/diagnostics/DiagnosticsPage";
import ExtensionsPage from "../features/extensions/ExtensionsPage";
import KnowledgePage from "../features/knowledge/KnowledgePage";
import SettingsPage from "../features/settings/SettingsPage";
import SectionPlaceholder from "./SectionPlaceholder";
import WorkbenchHome from "./WorkbenchHome";
import {
  getNavigationSection,
  primaryNavigationSections,
  utilityNavigationSections,
  type SectionId,
} from "./navigation";
import { LocaleProvider, useLocale } from "../i18n/locale";
import { ThemeProvider } from "../theme/theme";

type InitializationState = "loading" | "ready" | "failed";

export default function BloomeryApp() {
  return (
    <LocaleProvider>
      <ThemeProvider>
        <BloomeryAppShell />
      </ThemeProvider>
    </LocaleProvider>
  );
}

function BloomeryAppShell() {
  const [initializationState, setInitializationState] = useState<InitializationState>("loading");
  const [activeSection, setActiveSection] = useState<SectionId>("workbench");
  const [collapsed, setCollapsed] = useState(false);
  const initializationRef = useRef<Promise<void> | null>(null);
  const active = getNavigationSection(activeSection);
  const { t } = useLocale();

  useEffect(() => {
    let mounted = true;
    const initialization = initializationRef.current ?? (initializationRef.current = desktop.initialize());
    initialization.then(async () => {
      if (!mounted) return;
      if (!isDesktopRuntime()) {
        setInitializationState("ready");
        return;
      }
      try {
        if (mounted) setInitializationState("ready");
      } catch {
        if (mounted) setInitializationState("failed");
      }
    }, () => {
      if (initializationRef.current === initialization) initializationRef.current = null;
      if (mounted) setInitializationState("failed");
    });
    return () => {
      mounted = false;
    };
  }, []);

  return (
    <div className={`bloomery-app ${collapsed ? "is-collapsed" : ""}`}>
      <header className="bloomery-topbar">
        <div className="bloomery-brand-lockup">
          <div className="bloomery-brand-mark" aria-hidden="true">
            <Factory size={17} />
          </div>
          {!collapsed && (
            <div className="bloomery-brand-copy">
              <strong>BLOOMERY</strong>
            </div>
          )}
        </div>
      </header>

      <div className="bloomery-body">
        <nav className="bloomery-sidebar" aria-label={t("mainNavigation")}>
          <div className="bloomery-sidebar-head">
            {!collapsed && <span className="bloomery-sidebar-caption">{t("workspace")}</span>}
            <button
              type="button"
              className="bloomery-icon-button"
              aria-label={collapsed ? t("expandSidebar") : t("collapseSidebar")}
              title={collapsed ? t("expandSidebar") : t("collapseSidebar")}
              onClick={() => setCollapsed((value) => !value)}
            >
              {collapsed ? <PanelLeftOpen size={17} aria-hidden="true" /> : <PanelLeftClose size={17} aria-hidden="true" />}
            </button>
          </div>
          <div className="bloomery-nav-list" aria-label={t("moduleNavigation")}>
            {primaryNavigationSections.map(({ id, labelKey, icon: Icon }) => {
              const isActive = activeSection === id;
              return (
                <button
                  key={id}
                  type="button"
                  className={`bloomery-nav-item ${isActive ? "is-active" : ""}`}
                  aria-label={t(labelKey)}
                  aria-current={isActive ? "page" : undefined}
                  title={collapsed ? t(labelKey) : undefined}
                  onClick={() => setActiveSection(id)}
                >
                  <Icon size={18} strokeWidth={isActive ? 2.2 : 1.8} aria-hidden="true" />
                  {!collapsed && <span data-testid={`nav-label-${id}`}>{t(labelKey)}</span>}
                </button>
              );
            })}
          </div>
          <div className="bloomery-sidebar-footer" data-testid="utility-navigation">
            <div className="bloomery-nav-list" aria-label={t("utilityNavigation")}>
              {utilityNavigationSections.map(({ id, labelKey, icon: Icon }) => {
                const isActive = activeSection === id || (id === "settings" && activeSection === "diagnostics");
                return (
                  <button
                    key={id}
                    type="button"
                    className={`bloomery-nav-item ${isActive ? "is-active" : ""}`}
                    aria-label={t(labelKey)}
                    aria-current={isActive ? "page" : undefined}
                    title={collapsed ? t(labelKey) : undefined}
                    onClick={() => setActiveSection(id)}
                  >
                    <Icon size={18} strokeWidth={isActive ? 2.2 : 1.8} aria-hidden="true" />
                    {!collapsed && <span data-testid={`nav-label-${id}`}>{t(labelKey)}</span>}
                  </button>
                );
              })}
            </div>
          </div>
        </nav>

        <main className="bloomery-main" aria-label={t(active.labelKey)}>
          <div className={`bloomery-main-inner ${activeSection === "chat" ? "is-chat-shell" : ""}`}>
            {activeSection === "workbench" ? (
              <WorkbenchHome
                initializationState={initializationState}
                onOpenSection={setActiveSection}
              />
            ) : activeSection === "analysis" ? (
              <AnalysisPage />
            ) : activeSection === "knowledge" ? (
              <KnowledgePage />
            ) : activeSection === "settings" ? (
              <SettingsPage onOpenDiagnostics={() => setActiveSection("diagnostics")} />
            ) : activeSection === "diagnostics" ? (
              <DiagnosticsPage />
            ) : activeSection === "extensions" ? (
              <ExtensionsPage />
            ) : activeSection === "chat" ? (
              <ChatPage onOpenSection={setActiveSection} />
            ) : (
              <SectionPlaceholder section={active} />
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
