import { invokeDesktop, isTauriRuntime } from "./tauri";

export type DesktopCloudJob = {
  id: string;
  conversation_id?: string | null;
  cloud_job_id: string;
  type: string;
  status: string;
  payload_json: string;
  result_json?: string | null;
  created_at: string;
  updated_at: string;
};

export type DesktopCloudJobInput = {
  id?: string;
  conversation_id?: string | null;
  cloud_job_id: string;
  type: string;
  status: string;
  payload_json?: string;
  result_json?: string | null;
};

export type DesktopCloudJobSyncResult = {
  synced: number;
  failed: number;
  jobs: DesktopCloudJob[];
};

export function listCloudJobs() {
  return invokeDesktop<DesktopCloudJob[]>("list_cloud_jobs");
}

export function syncCloudJobs() {
  return invokeDesktop<DesktopCloudJobSyncResult>("sync_cloud_jobs");
}

export function saveCloudJob(job: DesktopCloudJobInput) {
  return invokeDesktop<DesktopCloudJob>("save_cloud_job", { job });
}

export function updateCloudJob(id: string, status: string, resultJson?: string | null) {
  return invokeDesktop<void>("update_cloud_job", { id, status, resultJson });
}

export function cloudJobLocalId(type: string, cloudJobId: string) {
  return `${type}:${cloudJobId}`;
}

export function toCloudJobJson(value: unknown) {
  try {
    return JSON.stringify(value ?? {});
  } catch {
    return "{}";
  }
}

export async function saveCloudJobMirror(job: DesktopCloudJobInput) {
  if (!isTauriRuntime()) return null;
  const cloudJobId = String(job.cloud_job_id || "").trim();
  if (!cloudJobId) return null;
  try {
    return await saveCloudJob({
      ...job,
      id: job.id || cloudJobLocalId(job.type, cloudJobId),
      cloud_job_id: cloudJobId,
      payload_json: job.payload_json ?? undefined,
      result_json: job.result_json ?? undefined,
    });
  } catch (error) {
    console.warn("Failed to mirror cloud job locally", error);
    return null;
  }
}
