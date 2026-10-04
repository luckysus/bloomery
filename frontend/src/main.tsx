import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource/inter/300.css";
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@fontsource/inter/800.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/700.css";
import "@fontsource/noto-sans-sc/400.css";
import "@fontsource/noto-sans-sc/500.css";
import "@fontsource/noto-sans-sc/700.css";
import SunaApp from "./app/SunaApp";
import { desktop, isDesktopRuntime } from "./bridge/desktop";
import "./index.css";

async function bootstrap() {
  // Settings/theme providers read the desktop store during their first
  // effects. Initialise SQLite before mounting the tree so the first read
  // cannot race `db_init` and surface a false "database not initialized"
  // error on a fresh desktop launch.
  if (isDesktopRuntime()) {
    await desktop.initialize().catch(() => undefined);
  }
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <SunaApp />
    </StrictMode>,
  );
}

void bootstrap();
