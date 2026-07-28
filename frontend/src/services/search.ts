import { API_BASE, authHeaders } from "./api";
import { canUseDesktopCloudTasks, desktopCloudDownloadFetch, desktopCloudTaskFetch } from "../desktop/services/cloudTasks";

export async function getOverview(): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch("/api/overview");
  }
  return fetch(`${API_BASE}/api/overview`, { credentials: "include", headers: authHeaders() });
}

export async function searchRetrieval(payload: unknown): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch("/api/search", {
      method: "POST",
      body: payload,
    });
  }
  return fetch(`${API_BASE}/api/search`, {
    method: "POST",
    credentials: "include",
    headers: authHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify(payload),
  });
}

export async function coilMatch(payload: unknown): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch("/api/coil_match", {
      method: "POST",
      body: payload,
    });
  }
  return fetch(`${API_BASE}/api/coil_match`, {
    method: "POST",
    credentials: "include",
    headers: authHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify(payload),
  });
}

export async function exportData(params: URLSearchParams): Promise<Response> {
  const path = `/api/export?${params.toString()}`;
  if (canUseDesktopCloudTasks()) {
    return desktopCloudDownloadFetch(path);
  }
  return fetch(`${API_BASE}${path}`, { credentials: "include", headers: authHeaders() });
}
