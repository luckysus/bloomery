import { API_BASE } from "./api";

export type TrainingStatusResponse = {
  status?: unknown;
  logs?: string[];
  [key: string]: unknown;
};

export async function getTrainingModels(): Promise<{ models?: unknown[] }> {
  const response = await fetch(`${API_BASE}/api/training/models`);
  return response.json() as Promise<{ models?: unknown[] }>;
}

export async function getTrainingStatus(jobId: string): Promise<Response> {
  return fetch(`${API_BASE}/api/training/status/${jobId}`);
}

export async function getLatestTrainingJob(): Promise<Response> {
  return fetch(`${API_BASE}/api/training/latest`);
}

export async function getTrainingModelLogs(version: string): Promise<Response> {
  return fetch(`${API_BASE}/api/training/models/${version}/logs`);
}
