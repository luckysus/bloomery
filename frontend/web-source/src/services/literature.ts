import { API_BASE } from "./api";

export type LiteratureFolderInfo = {
  name: string;
  pdf_count: number;
};

export type LiteratureFileInfo = {
  name: string;
  size: number;
  updated_at?: number;
};

export type LiteraturePreviewBlock = {
  content: string;
};

export type LiteratureFilePreview = {
  folder: string;
  filename: string;
  processed: boolean;
  source?: string;
  content?: string;
  blocks: LiteraturePreviewBlock[];
};

export type LiteratureJobInfo = {
  job_id: string;
  folder: string;
  status: string;
  progress: string;
  error?: string;
  paper_count?: number;
  pdf_count?: number;
  filenames?: string[];
  created_at?: string;
  duration?: string;
  elapsed_seconds?: number;
  duration_seconds?: number;
  current_page?: number;
  total_pages?: number;
  progress_percent?: number;
  eta_seconds?: number | null;
  estimate_method?: string;
};

export type LiteratureProcessingOptions = {
  parse_mode?: string;
  segment_mode?: string;
  extract_images?: boolean;
  extract_ocr?: boolean;
  extract_tables?: boolean;
  enable_formula?: boolean;
  page_ranges?: string;
  filter_text?: string;
  max_chunk_size?: number;
  min_chunk_size?: number;
  chunk_overlap?: number;
  filenames?: string[];
};

export type LiteratureMergeMode = "existing" | "new";

async function requireOk(response: Response, fallback: string) {
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || fallback);
  }
}

export async function getLiteratureFolders(): Promise<{ folders?: LiteratureFolderInfo[] }> {
  const response = await fetch(`${API_BASE}/api/literature/folders`, { credentials: "include" });
  await requireOk(response, "获取知识库列表失败");
  return response.json() as Promise<{ folders?: LiteratureFolderInfo[] }>;
}

export async function getLiteratureFiles(folder: string): Promise<{ folder?: string; files?: LiteratureFileInfo[] }> {
  const params = new URLSearchParams({ folder });
  const response = await fetch(`${API_BASE}/api/literature/files?${params.toString()}`, { credentials: "include" });
  await requireOk(response, `获取文档列表失败：${folder}`);
  return response.json() as Promise<{ folder?: string; files?: LiteratureFileInfo[] }>;
}

export async function getLiteratureFilePreview(folder: string, filename: string): Promise<LiteratureFilePreview> {
  const params = new URLSearchParams({ folder, filename });
  const response = await fetch(`${API_BASE}/api/literature/files/preview?${params.toString()}`, { credentials: "include" });
  await requireOk(response, `获取文档预览失败：${filename}`);
  return response.json() as Promise<LiteratureFilePreview>;
}

export function getLiteraturePdfUrl(folder: string, filename: string): string {
  const params = new URLSearchParams({ folder, filename });
  return `${API_BASE}/api/literature/files/raw?${params.toString()}`;
}

// 原始上传文件的字节流地址，适用于所有格式（PDF/图片/DOCX/XLSX/PPTX），用于预览与下载
export function getLiteratureRawUrl(folder: string, filename: string): string {
  return getLiteraturePdfUrl(folder, filename);
}

export function getLiteratureImageUrl(folder: string, filename: string, image: string): string {
  const params = new URLSearchParams({ folder, filename, image });
  return `${API_BASE}/api/literature/files/image?${params.toString()}`;
}

export async function getLiteratureJobs(): Promise<{ jobs?: LiteratureJobInfo[] }> {
  const response = await fetch(`${API_BASE}/api/literature/jobs`, { credentials: "include" });
  await requireOk(response, "获取处理任务失败");
  return response.json() as Promise<{ jobs?: LiteratureJobInfo[] }>;
}

export async function startLiteratureProcessing(folder: string, options: LiteratureProcessingOptions = {}): Promise<void> {
  const response = await fetch(`${API_BASE}/api/literature/process`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ folder, ...options }),
  });
  await requireOk(response, `启动处理失败：${folder}`);
}

export async function uploadLiteraturePdf(folder: string, file: File): Promise<{ filename?: string; size?: number }> {
  const params = new URLSearchParams({ folder, filename: file.name });
  const response = await fetch(`${API_BASE}/api/literature/upload?${params.toString()}`, {
    method: "POST",
    headers: { "Content-Type": "application/pdf" },
    credentials: "include",
    body: file,
    cache: "no-store",
  });
  await requireOk(response, `上传失败：${file.name}`);
  return response.json() as Promise<{ filename?: string; size?: number }>;
}

export async function renameLiteraturePdf(folder: string, filename: string, newFilename: string): Promise<{ filename?: string }> {
  const response = await fetch(`${API_BASE}/api/literature/files/rename`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ folder, filename, new_filename: newFilename }),
  });
  await requireOk(response, `重命名失败：${filename}`);
  return response.json() as Promise<{ filename?: string }>;
}

export async function deleteLiteraturePdf(folder: string, filename: string): Promise<void> {
  const params = new URLSearchParams({ folder, filename });
  const response = await fetch(`${API_BASE}/api/literature/files?${params.toString()}`, {
    method: "DELETE",
    credentials: "include",
  });
  await requireOk(response, `删除失败：${filename}`);
}

export async function deleteLiteratureFolder(folder: string): Promise<void> {
  const params = new URLSearchParams({ folder });
  const response = await fetch(`${API_BASE}/api/literature/folders?${params.toString()}`, {
    method: "DELETE",
    credentials: "include",
  });
  await requireOk(response, `删除知识库失败：${folder}`);
}

export async function mergeLiteratureFolders(
  source: string,
  target: string,
  options: { mode?: LiteratureMergeMode; destination?: string } = {},
): Promise<{ copied_pdfs?: number; copied_output?: boolean; pdf_count?: number; destination?: string; mode?: LiteratureMergeMode }> {
  const response = await fetch(`${API_BASE}/api/literature/folders/merge`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ source, target, ...options }),
  });
  await requireOk(response, `合并知识库失败：${source}`);
  return response.json() as Promise<{ copied_pdfs?: number; copied_output?: boolean; pdf_count?: number; destination?: string; mode?: LiteratureMergeMode }>;
}

export async function deleteLiteratureJob(jobId: string): Promise<void> {
  await fetch(`${API_BASE}/api/literature/jobs/${jobId}`, { method: "DELETE", credentials: "include" });
}

export async function getLiteratureJobLogs(jobId: string): Promise<{ ok: boolean; logs?: string[] }> {
  const response = await fetch(`${API_BASE}/api/literature/jobs/${jobId}/logs`, { credentials: "include" });
  if (!response.ok) return { ok: false };
  const data = await response.json() as { logs?: string[] };
  return { ok: true, logs: data.logs };
}
