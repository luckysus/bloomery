import { invokeDesktop, isTauriRuntime } from "./tauri";

export type DesktopCloudTaskMirror = {
  jobType: string;
  cloudJobId?: string;
  status?: string;
  source?: string;
};

type DesktopCloudTaskResponse = {
  status: number;
  body: string;
};

type DesktopCloudDownloadResponse = {
  status: number;
  bytes: number[];
  contentType?: string | null;
  contentDisposition?: string | null;
};

export function canUseDesktopCloudTasks() {
  return isTauriRuntime();
}

export async function desktopCloudTaskFetch(
  path: string,
  options: {
    method?: string;
    body?: unknown;
    mirror?: DesktopCloudTaskMirror;
    signal?: AbortSignal;
  } = {},
): Promise<Response> {
  if (!isTauriRuntime()) {
    throw new Error("当前页面不在 Tauri 桌面运行时中");
  }
  if (options.signal?.aborted) throw new DOMException("Aborted", "AbortError");
  const task = invokeDesktop<DesktopCloudTaskResponse>("desktop_cloud_task_request", {
    request: {
      path,
      method: options.method || "GET",
      body: options.body ?? null,
      mirror: options.mirror ?? null,
    },
  });
  const result = options.signal ? await abortable(task, options.signal) : await task;
  return new Response(result.body || "", {
    status: result.status || 200,
    headers: { "Content-Type": "application/json" },
  });
}

export async function desktopCloudBinaryFetch(
  path: string,
  bytes: Uint8Array,
  options: {
    method?: string;
    contentType?: string;
    signal?: AbortSignal;
  } = {},
): Promise<Response> {
  if (!isTauriRuntime()) {
    throw new Error("当前页面不在 Tauri 桌面运行时中");
  }
  if (options.signal?.aborted) throw new DOMException("Aborted", "AbortError");
  const task = invokeDesktop<DesktopCloudTaskResponse>("desktop_cloud_binary_request", {
    request: {
      path,
      method: options.method || "POST",
      contentType: options.contentType || "application/octet-stream",
      bytes: Array.from(bytes),
    },
  });
  const result = options.signal ? await abortable(task, options.signal) : await task;
  return new Response(result.body || "", {
    status: result.status || 200,
    headers: { "Content-Type": "application/json" },
  });
}

export async function desktopCloudDownloadFetch(
  path: string,
  options: {
    signal?: AbortSignal;
  } = {},
): Promise<Response> {
  if (!isTauriRuntime()) {
    throw new Error("当前页面不在 Tauri 桌面运行时中");
  }
  if (options.signal?.aborted) throw new DOMException("Aborted", "AbortError");
  const task = invokeDesktop<DesktopCloudDownloadResponse>("desktop_cloud_download_request", {
    request: { path },
  });
  const result = options.signal ? await abortable(task, options.signal) : await task;
  const headers = new Headers();
  if (result.contentType) headers.set("Content-Type", result.contentType);
  if (result.contentDisposition) headers.set("Content-Disposition", result.contentDisposition);
  return new Response(new Uint8Array(result.bytes || []), {
    status: result.status || 200,
    headers,
  });
}

async function abortable<T>(task: Promise<T>, signal: AbortSignal): Promise<T> {
  if (signal.aborted) throw new DOMException("Aborted", "AbortError");
  return Promise.race([
    task,
    new Promise<T>((_, reject) => {
      signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
    }),
  ]);
}
