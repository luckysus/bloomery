import { useEffect, useState } from "react";
import { Factory, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import LanguageSelect from "../components/common/LanguageSelect";
import ChatPage from "../features/chat/ChatPage";
import AnalysisPage from "../features/analysis/AnalysisPage";
import DiagnosticsPage from "../features/diagnostics/DiagnosticsPage";
import ExtensionsPage from "../features/extensions/ExtensionsPage";
import KnowledgePage from "../features/knowledge/KnowledgePage";
import OnboardingPage from "../features/onboarding/OnboardingPage";
import SettingsPage from "../features/settings/SettingsPage";
import SectionPlaceholder from "./SectionPlaceholder";
import WorkbenchHome from "./WorkbenchHome";
import { getNavigationSection, navigationSections, type SectionId } from "./navigation";
import { LocaleProvider, useLocale } from "../i18n/locale";

type InitializationState = "loading" | "setup" | "ready" | "failed";

export default function BloomeryApp() {
  return (
    <LocaleProvider>
      <BloomeryAppShell />
    </LocaleProvider>
  );
}

function BloomeryAppShell() {
  const [initializationState, setInitializationState] = useState<InitializationState>("loading");
  const [activeSection, setActiveSection] = useState<SectionId>("workbench");
  const [collapsed, setCollapsed] = useState(false);
  const active = getNavigationSection(activeSection);
  const { t } = useLocale();

  useEffect(() => {
    let mounted = true;
    desktop.initialize().then(async () => {
      if (!mounted) return;
      if (!isDesktopRuntime()) {
        setInitializationState("ready");
        return;
      }
      try {
        const value = await desktop.getSetting("onboarding.completed");
        const complete = value ? JSON.parse(value).completed === true : false;
        if (mounted) setInitializationState(complete ? "ready" : "setup");
      } catch {
        if (mounted) setInitializationState("failed");
      }
    }, () => mounted && setInitializationState("failed"));
    return () => {
      mounted = false;
    };
  }, []);

  if (initializationState === "setup") {
    return <OnboardingPage onComplete={() => setInitializationState("ready")} />;
  }

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
            <span>{t("brandTagline")}</span>
            </div>
          )}
        </div>
        <div className="bloomery-topbar-meta">
          <span className="bloomery-local-indicator">
            <span className="bloomery-state-dot" aria-hidden="true" />
            {t("localWorkspace")}
          </span>
          <span className="bloomery-version-label">LOCAL / 0.1</span>
          <LanguageSelect />
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
            {navigationSections.map(({ id, labelKey, icon: Icon }) => (
              <button
                key={id}
                type="button"
                className={`bloomery-nav-item ${activeSection === id ? "is-active" : ""}`}
                aria-label={t(labelKey)}
                aria-current={activeSection === id ? "page" : undefined}
                title={collapsed ? t(labelKey) : undefined}
                onClick={() => setActiveSection(id)}
              >
                <Icon size={18} strokeWidth={activeSection === id ? 2.2 : 1.8} aria-hidden="true" />
                {!collapsed && <span data-testid={`nav-label-${id}`}>{t(labelKey)}</span>}
              </button>
            ))}
          </div>
          {!collapsed && <p className="bloomery-sidebar-footer">{t("offlineFooter")}</p>}
        </nav>

        <main className="bloomery-main" aria-label={t(active.labelKey)}>
          <div className="bloomery-main-inner">
            {activeSection === "workbench" ? (
              <WorkbenchHome
                initializationState={initializationState}
                onOpenSection={setActiveSection}
              />
            ) : activeSection === "chat" ? (
              <ChatPage />
            ) : activeSection === "analysis" ? (
              <AnalysisPage />
            ) : activeSection === "knowledge" ? (
              <KnowledgePage />
            ) : activeSection === "settings" ? (
              <SettingsPage />
            ) : activeSection === "diagnostics" ? (
              <DiagnosticsPage />
            ) : activeSection === "extensions" ? (
              <ExtensionsPage />
            ) : (
              <SectionPlaceholder section={active} />
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
