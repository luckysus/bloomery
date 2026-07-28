import { fetchJson } from "./api";

export interface MinerUVersionRecord {
  status: string;
  source?: string;
  verified?: boolean;
  conda_env?: string;
  installed_at?: string;
  error?: string;
}

export interface MinerUStatus {
  active: string | null;
  versions: Record<string, MinerUVersionRecord>;
  has_running_jobs: boolean;
  keep_versions: number;
}

export interface MinerUReleaseItem {
  version: string;
  published_at: string;
  prerelease: boolean;
}

export interface MinerUUpdateJob {
  job_id: string;
  version: string;
  status: string;
  logs: string[];
  error?: string;
}

const BASE = "/api/admin/mineru";

export const getMineruStatus = () => fetchJson<MinerUStatus>(`${BASE}/status`);

export const getMineruReleases = () =>
  fetchJson<{ releases: MinerUReleaseItem[] }>(`${BASE}/releases`);

export const startMineruUpdate = (version: string) =>
  fetchJson<{ job_id: string }>(`${BASE}/update`, {
    method: "POST",
    body: JSON.stringify({ version }),
  });

export const getMineruUpdateJob = (jobId: string) =>
  fetchJson<MinerUUpdateJob>(`${BASE}/update/jobs/${jobId}`);

export const activateMineruVersion = (version: string) =>
  fetchJson<Record<string, unknown>>(`${BASE}/activate`, {
    method: "POST",
    body: JSON.stringify({ version }),
  });

export const rollbackMineruVersion = (version: string) =>
  fetchJson<Record<string, unknown>>(`${BASE}/rollback`, {
    method: "POST",
    body: JSON.stringify({ version }),
  });

export const deleteMineruVersion = (version: string) =>
  fetchJson<Record<string, unknown>>(`${BASE}/versions/${version}`, {
    method: "DELETE",
  });
