import { isTauriRuntime } from "./tauri";

const WINDOW_STATE_KEY = "bloomery:desktop:window-state";

type StoredWindowState = {
  width?: number;
  height?: number;
  x?: number;
  y?: number;
};

export async function setupWindowStatePersistence() {
  if (!isTauriRuntime() || typeof window === "undefined") return () => {};
  const { getCurrentWindow, LogicalPosition, LogicalSize } = await import("@tauri-apps/api/window");
  const appWindow = getCurrentWindow();
  const stored = readWindowState();
  if (stored.width && stored.height) {
    await appWindow.setSize(new LogicalSize(stored.width, stored.height)).catch(() => {});
  }
  if (Number.isFinite(stored.x) && Number.isFinite(stored.y)) {
    await appWindow.setPosition(new LogicalPosition(stored.x!, stored.y!)).catch(() => {});
  }

  let saveTimer = 0;
  const scheduleSave = () => {
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      void saveWindowState(appWindow);
    }, 250);
  };
  const unlistenResize = await appWindow.onResized(scheduleSave);
  const unlistenMove = await appWindow.onMoved(scheduleSave);
  return () => {
    window.clearTimeout(saveTimer);
    unlistenResize();
    unlistenMove();
  };
}

function readWindowState(): StoredWindowState {
  try {
    return JSON.parse(window.localStorage.getItem(WINDOW_STATE_KEY) || "{}") as StoredWindowState;
  } catch {
    return {};
  }
}

async function saveWindowState(appWindow: Awaited<ReturnType<typeof import("@tauri-apps/api/window")["getCurrentWindow"]>>) {
  const [size, position] = await Promise.all([
    appWindow.innerSize(),
    appWindow.outerPosition(),
  ]);
  window.localStorage.setItem(
    WINDOW_STATE_KEY,
    JSON.stringify({
      width: size.width,
      height: size.height,
      x: position.x,
      y: position.y,
    }),
  );
}
