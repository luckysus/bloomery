export function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const LAST_ERROR_KEY = "bloomery:desktop:last-error-kind";

export function rememberDesktopError(command: string, error: unknown) {
  if (typeof window === "undefined") return;
  const name = error instanceof Error ? error.name : typeof error;
  const value = `${command}:${name || "Error"}`.replace(/[^\w:.-]/g, "").slice(0, 80);
  window.localStorage.setItem(LAST_ERROR_KEY, value);
}

export function getLastDesktopErrorKind() {
  if (typeof window === "undefined") return null;
  return window.localStorage.getItem(LAST_ERROR_KEY);
}

export function createDesktopRunId(prefix = "run") {
  const random = typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `${prefix}-${random}`;
}

export async function invokeDesktop<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    throw new Error("当前页面不在 Tauri 桌面运行时中");
  }
  const tauri = await import("@tauri-apps/api/core");
  try {
    return await tauri.invoke<T>(command, args);
  } catch (error) {
    rememberDesktopError(command, error);
    throw error;
  }
}

export async function initDesktopDb() {
  await invokeDesktop<void>("db_init");
}

export type DesktopAuthSession = {
  user_id: string;
  token: string;
  username?: string | null;
  email?: string | null;
};

export type DesktopAuthUser = {
  username?: string;
  email?: string;
  session_token?: string;
};

export function getDesktopAuthSession() {
  return invokeDesktop<DesktopAuthSession | null>("auth_get_session");
}

export function saveDesktopAuthSession(user: DesktopAuthUser) {
  const username = String(user.username || "").trim();
  if (!username) return Promise.resolve();
  return invokeDesktop<void>("auth_save_session", {
    session: {
      user_id: username,
      token: user.session_token || "",
      username,
      email: user.email || null,
    },
  });
}

export function clearDesktopAuthSession() {
  return invokeDesktop<void>("auth_clear_session");
}
