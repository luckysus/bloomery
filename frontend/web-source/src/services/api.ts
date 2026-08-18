export const API_BASE =
  (globalThis as typeof globalThis & { __RAG_DESKTOP_CONFIG__?: { apiBase?: string } })
    .__RAG_DESKTOP_CONFIG__?.apiBase ?? "";

export async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    credentials: "include",
    ...init,
    headers: {
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...(init?.headers || {}),
    },
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return response.json() as Promise<T>;
}
