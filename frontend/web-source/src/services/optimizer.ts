import { API_BASE } from "./api";

export async function getRecentOptimizeJobs(limit = 30): Promise<Response> {
  return fetch(`${API_BASE}/api/optimize/recent?limit=${encodeURIComponent(String(limit))}`, { credentials: "include" });
}

export async function getOptimizeLogs(): Promise<Response> {
  return fetch(`${API_BASE}/api/optimize/logs`, { credentials: "include" });
}

export async function cancelOptimize(): Promise<Response> {
  return fetch(`${API_BASE}/api/optimize/cancel`, {
    method: "POST",
    credentials: "include",
  });
}

export async function runOptimize(payload: unknown, signal?: AbortSignal): Promise<Response> {
  return fetch(`${API_BASE}/api/optimize`, {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
    signal,
  });
}
