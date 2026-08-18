import type { ImageResult, LitResult, ProductionRecord, ProductionStats, SearchResponse } from "../types/rag";

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function arrayOrEmpty<T>(value: unknown): T[] {
  return Array.isArray(value) ? value as T[] : [];
}

function errorMessage(input: Record<string, unknown>) {
  const raw = input.error ?? input.detail ?? input.message;
  if (raw == null) return undefined;
  if (typeof raw === "string") return raw;
  try {
    return JSON.stringify(raw);
  } catch {
    return String(raw);
  }
}

function emptySearchResponse(error?: string): SearchResponse {
  return {
    success: false,
    production_stats: null,
    production_columns: [],
    production_records: [],
    advice_mode: null,
    advice_prompt: null,
    advice_contexts: [],
    advice_record_count: 0,
    advice_standard_columns: [],
    advice_standard_records: [],
    literature_results: [],
    literature_images: [],
    experimental_images: [],
    ...(error ? { error } : {}),
  };
}

export function normalizeSearchResponse(value: unknown): SearchResponse {
  if (!isRecord(value)) return emptySearchResponse();

  const mode = value.advice_mode === "composition" || value.advice_mode === "process"
    ? value.advice_mode
    : null;
  const recordCount = typeof value.advice_record_count === "number" && Number.isFinite(value.advice_record_count)
    ? value.advice_record_count
    : 0;
  const error = errorMessage(value);

  return {
    success: value.success === true,
    production_stats: isRecord(value.production_stats) ? value.production_stats as unknown as ProductionStats : null,
    production_columns: arrayOrEmpty<string>(value.production_columns),
    production_records: arrayOrEmpty<ProductionRecord>(value.production_records),
    advice_mode: mode,
    advice_prompt: typeof value.advice_prompt === "string" ? value.advice_prompt : null,
    advice_contexts: arrayOrEmpty<string>(value.advice_contexts),
    advice_record_count: recordCount,
    advice_standard_columns: arrayOrEmpty<string>(value.advice_standard_columns),
    advice_standard_records: arrayOrEmpty<Record<string, unknown> | null>(value.advice_standard_records),
    literature_results: arrayOrEmpty<LitResult>(value.literature_results),
    literature_images: arrayOrEmpty<ImageResult>(value.literature_images),
    experimental_images: arrayOrEmpty<ImageResult>(value.experimental_images),
    ...(error ? { error } : {}),
  };
}

export async function readSearchResponse(res: Response): Promise<SearchResponse> {
  let payload: unknown;

  try {
    const contentType = res.headers.get("content-type") ?? "";
    payload = contentType.includes("application/json")
      ? await res.json()
      : { error: await res.text() };
  } catch (err) {
    payload = { error: err instanceof Error ? err.message : String(err) };
  }

  const normalized = normalizeSearchResponse(payload);
  if (res.ok) return normalized;

  return {
    ...normalized,
    success: false,
    error: normalized.error || `Request failed: ${res.status}`,
  };
}
