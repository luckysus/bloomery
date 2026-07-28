import { API_BASE, authHeaders } from "./api";
import { canUseDesktopCloudTasks, desktopCloudTaskFetch } from "../desktop/services/cloudTasks";

export interface LabServiceStatusInfo {
  available: boolean;
  retrieval_available?: boolean;
  optimization_available?: boolean;
  message: string;
  cached?: boolean;
  checks?: Array<{ name: string; available: boolean; status_code?: number | null; latency_ms?: number; error?: string }>;
}

export async function getLabServiceStatus(options?: { force?: boolean }): Promise<LabServiceStatusInfo> {
  if (canUseDesktopCloudTasks()) {
    const response = await desktopCloudTaskFetch(`/api/lab-service/status${options?.force ? "?refresh=1" : ""}`);
    if (!response.ok) throw new Error(await response.text());
    return response.json() as Promise<LabServiceStatusInfo>;
  }
  const response = await fetch(`${API_BASE}/api/lab-service/status${options?.force ? "?refresh=1" : ""}`, { method: "GET", headers: authHeaders() });
  if (!response.ok) throw new Error(await response.text());
  return response.json() as Promise<LabServiceStatusInfo>;
}

export async function reconnectLabService(): Promise<LabServiceStatusInfo> {
  if (canUseDesktopCloudTasks()) {
    const response = await desktopCloudTaskFetch("/api/lab-service/reconnect", { method: "POST" });
    if (!response.ok) throw new Error(await response.text());
    return response.json() as Promise<LabServiceStatusInfo>;
  }
  const response = await fetch(`${API_BASE}/api/lab-service/reconnect`, { method: "POST", headers: authHeaders() });
  if (!response.ok) throw new Error(await response.text());
  return response.json() as Promise<LabServiceStatusInfo>;
}
