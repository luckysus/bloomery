const DESKTOP_SESSION_TOKEN_KEY = "bloomery_session_token";
const DESKTOP_API_BASE_KEY = "bloomery_api_base";

const DEFAULT_API_BASE =
  (globalThis as typeof globalThis & { __RAG_DESKTOP_CONFIG__?: { apiBase?: string } })
    .__RAG_DESKTOP_CONFIG__?.apiBase ??
  import.meta.env.VITE_BLOOMERY_API_BASE ??
  "";

function readStoredApiBase() {
  try {
    return localStorage.getItem(DESKTOP_API_BASE_KEY) || "";
  } catch {
    return "";
  }
}

const INITIAL_API_BASE = readStoredApiBase() || DEFAULT_API_BASE;

export let API_BASE = INITIAL_API_BASE;

let runtimeApiBase = INITIAL_API_BASE;

export function getApiBase() {
  if (runtimeApiBase) return runtimeApiBase;
  return readStoredApiBase();
}

export function setRuntimeApiBase(apiBase: string) {
  runtimeApiBase = apiBase.trim();
  API_BASE = runtimeApiBase;
  try {
    if (runtimeApiBase) localStorage.setItem(DESKTOP_API_BASE_KEY, runtimeApiBase);
    else localStorage.removeItem(DESKTOP_API_BASE_KEY);
  } catch {
    // Runtime API base still works for this session.
  }
}

export function getDesktopSessionToken() {
  try {
    return localStorage.getItem(DESKTOP_SESSION_TOKEN_KEY) || "";
  } catch {
    return "";
  }
}

export function setDesktopSessionToken(token: string) {
  try {
    if (token) localStorage.setItem(DESKTOP_SESSION_TOKEN_KEY, token);
    else localStorage.removeItem(DESKTOP_SESSION_TOKEN_KEY);
  } catch {
    // The cookie session can still work when storage is unavailable.
  }
}

export function authHeaders(headers?: HeadersInit): HeadersInit {
  const token = getDesktopSessionToken();
  return {
    ...(headers || {}),
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
}

export async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${getApiBase()}${path}`, {
    credentials: "include",
    ...init,
    headers: authHeaders({
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...(init?.headers || {}),
    }),
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return response.json() as Promise<T>;
}
