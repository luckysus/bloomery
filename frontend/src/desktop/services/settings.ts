import { getApiBase, setRuntimeApiBase } from "../../services/api";
import { invokeDesktop } from "./tauri";

export function getSetting(key: string) {
  return invokeDesktop<string | null>("get_setting", { key });
}

export function setSetting(key: string, valueJson: string) {
  return invokeDesktop<void>("set_setting", { key, valueJson });
}

export async function getCloudApiBaseSetting() {
  const stored = await getSetting("cloud_api_base").catch(() => null);
  if (!stored) return getApiBase();
  try {
    return String(JSON.parse(stored) || "");
  } catch {
    return stored;
  }
}

export async function saveCloudApiBaseSetting(apiBase: string) {
  setRuntimeApiBase(apiBase);
  await setSetting("cloud_api_base", JSON.stringify(apiBase));
}
