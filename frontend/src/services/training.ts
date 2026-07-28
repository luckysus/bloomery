import { authHeaders, getApiBase } from "./api";
import { canUseDesktopCloudTasks, desktopCloudTaskFetch } from "../desktop/services/cloudTasks";

export type TrainingStatusResponse = {
  status?: unknown;
  logs?: string[];
  [key: string]: unknown;
};

export type TrainingStartPayload = {
  model_version: string;
  max_rows: number | null;
};

export async function startTraining(payload: TrainingStartPayload, signal?: AbortSignal): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch("/api/training/start", {
      method: "POST",
      body: payload,
      signal,
      mirror: { jobType: "training", status: "running", source: "training_start" },
    });
  }
  return fetch(`${getApiBase()}/api/training/start`, {
    method: "POST",
    credentials: "include",
    headers: authHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify(payload),
    signal,
  });
}

export async function getTrainingModels(): Promise<{ models?: unknown[] }> {
  if (canUseDesktopCloudTasks()) {
    const response = await desktopCloudTaskFetch("/api/training/models");
    return response.json() as Promise<{ models?: unknown[] }>;
  }
  const response = await fetch(`${getApiBase()}/api/training/models`, { credentials: "include", headers: authHeaders() });
  return response.json() as Promise<{ models?: unknown[] }>;
}

export async function getTrainingStatus(jobId: string): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch(`/api/training/status/${encodeURIComponent(jobId)}`, {
      mirror: { jobType: "training", cloudJobId: jobId, source: "training_status" },
    });
  }
  return fetch(`${getApiBase()}/api/training/status/${jobId}`, { credentials: "include", headers: authHeaders() });
}

export async function getLatestTrainingJob(): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch("/api/training/latest");
  }
  return fetch(`${getApiBase()}/api/training/latest`, { credentials: "include", headers: authHeaders() });
}

export async function getTrainingModelLogs(version: string): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch(`/api/training/models/${encodeURIComponent(version)}/logs`);
  }
  return fetch(`${getApiBase()}/api/training/models/${version}/logs`, { credentials: "include", headers: authHeaders() });
}

export async function cancelTraining(jobId: string): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch(`/api/training/cancel/${encodeURIComponent(jobId)}`, {
      method: "POST",
      mirror: { jobType: "training", cloudJobId: jobId, status: "cancelled", source: "training_cancel" },
    });
  }
  return fetch(`${getApiBase()}/api/training/cancel/${jobId}`, { method: "POST", credentials: "include", headers: authHeaders() });
}

export async function activateTrainingModel(version: string): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch(`/api/training/models/${encodeURIComponent(version)}/activate`, { method: "POST" });
  }
  return fetch(`${getApiBase()}/api/training/models/${version}/activate`, { method: "POST", credentials: "include", headers: authHeaders() });
}

export async function deleteTrainingModel(version: string): Promise<Response> {
  if (canUseDesktopCloudTasks()) {
    return desktopCloudTaskFetch(`/api/training/models/${encodeURIComponent(version)}`, { method: "DELETE" });
  }
  return fetch(`${getApiBase()}/api/training/models/${version}`, { method: "DELETE", credentials: "include", headers: authHeaders() });
}
