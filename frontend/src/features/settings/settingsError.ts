/**
 * Convert bridge failures into a short, user-facing settings error.
 * Tauri commands may include low-level paths, connection strings or a Rust
 * backtrace. Those details are useful in diagnostics but must not appear in
 * the settings UI (and should never expose credentials).
 */
export function settingsErrorMessage(cause: unknown, fallback: string): string {
  const raw = cause instanceof Error ? cause.message : typeof cause === "string" ? cause : "";
  if (!raw.trim()) return fallback;

  const normalized = raw
    .replace(/\r?\n[\s\S]*/g, "")
    .replace(/((?:password|passwd|api[_ -]?key|token|secret|authorization))\s*[=:]\s*[^,;\s]+/gi, "$1=[已隐藏]")
    .replace(/([a-z][a-z0-9+.-]*:\/\/[^:\s/@]+:)[^@\s]+@/gi, "$1[已隐藏]@")
    .trim();
  if (!normalized || /(?:RUST_BACKTRACE|panicked at|backtrace|stack trace|\btarget[\\/]src[\\/]|(?:postgres(?:ql)?|mysql|mssql|sqlite):\/\/|(?:sqlstate|connection string|database password)|\b(?:select|insert|update|delete)\b[\s\S]+\b(?:from|into|set|where)\b)/i.test(normalized)) return fallback;
  // Do not show a low-level credential diagnostic even when it did not carry
  // an explicit `key=value` pair that the redaction above could replace.
  if (/(?:password|passwd|api[_ -]?key|access[_ -]?token|bearer\s+|secret|authorization)/i.test(normalized)) return fallback;
  return normalized.length > 220 ? `${normalized.slice(0, 217)}…` : normalized;
}
