import { invokeDesktop, getLastDesktopErrorKind } from "./tauri";

export type DesktopDiagnostics = Record<string, unknown>;

export function exportDiagnostics() {
  return invokeDesktop<DesktopDiagnostics>("export_diagnostics", {
    lastErrorKind: getLastDesktopErrorKind(),
  });
}

export function downloadDiagnostics(report: DesktopDiagnostics) {
  const blob = new Blob([JSON.stringify(report, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `bloomery-diagnostics-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}
