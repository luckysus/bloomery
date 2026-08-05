import { useEffect, useState } from "react";
import { Factory, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import OnboardingPage from "../features/onboarding/OnboardingPage";
import SectionPlaceholder from "./SectionPlaceholder";
import WorkbenchHome from "./WorkbenchHome";
import { getNavigationSection, navigationSections, type SectionId } from "./navigation";

type InitializationState = "loading" | "setup" | "ready" | "failed";

export default function BloomeryApp() {
  const [initializationState, setInitializationState] = useState<InitializationState>("loading");
  const [activeSection, setActiveSection] = useState<SectionId>("workbench");
  const [collapsed, setCollapsed] = useState(false);
  const active = getNavigationSection(activeSection);

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
              <span>STEEL AGENT WORKBENCH</span>
            </div>
          )}
        </div>
        <div className="bloomery-topbar-meta">
          <span className="bloomery-local-indicator">
            <span className="bloomery-state-dot" aria-hidden="true" />
            本地工作区
          </span>
          <span className="bloomery-version-label">LOCAL / 0.1</span>
        </div>
      </header>

      <div className="bloomery-body">
        <nav className="bloomery-sidebar" aria-label="主导航">
          <div className="bloomery-sidebar-head">
            {!collapsed && <span className="bloomery-sidebar-caption">工作区</span>}
            <button
              type="button"
              className="bloomery-icon-button"
              aria-label={collapsed ? "展开侧栏" : "折叠侧栏"}
              title={collapsed ? "展开侧栏" : "折叠侧栏"}
              onClick={() => setCollapsed((value) => !value)}
            >
              {collapsed ? <PanelLeftOpen size={17} aria-hidden="true" /> : <PanelLeftClose size={17} aria-hidden="true" />}
            </button>
          </div>
          <div className="bloomery-nav-list" aria-label="模块导航">
            {navigationSections.map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                className={`bloomery-nav-item ${activeSection === id ? "is-active" : ""}`}
                aria-label={label}
                aria-current={activeSection === id ? "page" : undefined}
                title={collapsed ? label : undefined}
                onClick={() => setActiveSection(id)}
              >
                <Icon size={18} strokeWidth={activeSection === id ? 2.2 : 1.8} aria-hidden="true" />
                {!collapsed && <span data-testid={`nav-label-${id}`}>{label}</span>}
              </button>
            ))}
          </div>
          {!collapsed && <p className="bloomery-sidebar-footer">离线优先 · 数据归本地</p>}
        </nav>

        <main className="bloomery-main" aria-label={active.label}>
          <div className="bloomery-main-inner">
            {activeSection === "workbench" ? (
              <WorkbenchHome
                initializationState={initializationState}
                onOpenSection={setActiveSection}
              />
            ) : (
              <SectionPlaceholder section={active} />
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
