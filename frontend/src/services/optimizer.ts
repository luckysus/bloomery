import { authHeaders, getApiBase } from "./api";
import { canUseDesktopCloudTasks, desktopCloudTaskFetch } from "../desktop/services/cloudTasks";

export async function getRecentOptimizeJobs(limit = 30): Promise<Response> {
  const path = `/api/optimize/recent?limit=${encodeURIComponent(String(limit))}`;
  if (canUseDesktopCloudTasks()) return desktopCloudTaskFetch(path);
  return fetch(`${getApiBase()}${path}`, { credentials: "include", headers: authHeaders() });
}

export async function getOptimizeLogs(): Promise<Response> {
  if (canUseDesktopCloudTasks()) return desktopCloudTaskFetch("/api/optimize/logs");
  return fetch(`${getApiBase()}/api/optimize/logs`, { credentials: "include", headers: authHeaders() });
}

export async function cancelOptimize(): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch("/api/optimize/cancel", { method: "POST" });
  }
  return fetch(`${getApiBase()}/api/optimize/cancel`, {
    method: "POST",
    credentials: "include",
    headers: authHeaders(),
  });
}

export async function runOptimize(payload: unknown, signal?: AbortSignal): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch("/api/optimize", {
      method: "POST",
      body: payload,
      signal,
      mirror: { jobType: "optimization", status: "running", source: "optimizer" },
    });
  }
  return fetch(`${getApiBase()}/api/optimize`, {
    method: "POST",
    credentials: "include",
    headers: authHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify(payload),
    signal,
  });
}
