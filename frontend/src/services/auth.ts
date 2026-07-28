import { API_BASE } from "./api";

export type AuthMeResponse = {
  authenticated?: boolean;
  username?: unknown;
  role?: unknown;
  email?: unknown;
};

export async function getAuthMe(): Promise<AuthMeResponse> {
  const response = await fetch(`${API_BASE}/api/auth/me`, { credentials: "include" });
  return response.json() as Promise<AuthMeResponse>;
}

export async function logout(): Promise<void> {
  await fetch(`${API_BASE}/api/auth/logout`, { method: "POST", credentials: "include" });
}
