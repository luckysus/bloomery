import { API_BASE } from "./api";

export async function getOverview(): Promise<Response> {
  return fetch(`${API_BASE}/api/overview`, { credentials: "include" });
}

export async function searchRetrieval(payload: unknown): Promise<Response> {
  const requestId = payload && typeof payload === "object" && "client_request_id" in payload
    ? String((payload as { client_request_id?: string }).client_request_id || "")
    : "";
  const request = {
    method: "POST",
    credentials: "include" as const,
    headers: {
      "Content-Type": "application/json",
      ...(requestId ? { "X-Client-Request-Id": requestId } : {}),
    },
    body: JSON.stringify(payload),
  };
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      const response = await fetch(`${API_BASE}/api/search`, request);
      if (response.status < 500 || attempt === 1) return response;
    } catch (error) {
      if (attempt === 1) throw error;
    }
    await new Promise(resolve => window.setTimeout(resolve, 250));
  }
  throw new Error("search request failed");
}

export async function coilMatch(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/coil_match`, {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function exportData(params: URLSearchParams): Promise<Response> {
  return fetch(`${API_BASE}/api/export?${params.toString()}`, { credentials: "include" });
}
