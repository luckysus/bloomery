import { API_BASE } from "./api";

export async function getUserProfile(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/profile`, { credentials: "include" });
}

export async function getUserProfileStats(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/profile/stats`, { credentials: "include" });
}

export async function getUserLlmModels(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/llm-models`, { credentials: "include" });
}

export async function postUserLlmModels(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/llm-models`, {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function getUserLlmConfig(params?: URLSearchParams): Promise<Response> {
  const suffix = params ? `?${params.toString()}` : "";
  return fetch(`${API_BASE}/api/user/llm-config${suffix}`, { credentials: "include" });
}

export async function saveUserLlmConfig(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/llm-config`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function getTurnstileAdminConfig(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/turnstile`, { credentials: "include" });
}

export async function saveTurnstileAdminConfig(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/turnstile`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function getCaptchaAdminConfig(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/captcha`, { credentials: "include" });
}

export async function saveCaptchaAdminConfig(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/captcha`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function getAuthSecurityAdminConfig(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/auth`, { credentials: "include" });
}

export async function saveAuthSecurityAdminConfig(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/auth`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function getKnowledgeBaseSecurityAdminConfig(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/knowledge-base`, { credentials: "include" });
}

export async function saveKnowledgeBaseSecurityAdminConfig(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/knowledge-base`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function getMinerUProcessingAdminConfig(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/mineru`, { credentials: "include" });
}

export async function getMinerUUsageAdminStatus(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/mineru/usage`, { credentials: "include" });
}

export async function saveMinerUProcessingAdminConfig(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/mineru`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function getRetrievalModelsAdminConfig(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/retrieval-models`, { credentials: "include" });
}

export async function saveRetrievalModelsAdminConfig(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/retrieval-models`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export async function getIflytekAdminConfig(): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/iflytek`, { credentials: "include" });
}

export async function saveIflytekAdminConfig(payload: unknown): Promise<Response> {
  return fetch(`${API_BASE}/api/user/security/iflytek`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}
