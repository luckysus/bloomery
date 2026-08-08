import type { ReactNode } from "react";

// Only these schemes may be attached to an <img src>: remote images plus the
// blob URLs the app mints for local resources. Everything else (javascript:,
// data:, vbscript:, file:, ...) is rejected as an untrusted rendering vector.
const SAFE_IMAGE_SCHEMES = new Set(["http", "https", "blob"]);
// Schemes allowed on Markdown link/image URLs. Relative and anchor URLs carry
// no scheme and are always allowed.
const SAFE_LINK_SCHEMES = new Set(["http", "https", "mailto", "tel"]);
const SCHEME_PATTERN = /^\s*([a-z][a-z0-9+.-]*):/i;

function schemeOf(url: string): string | null {
  const match = SCHEME_PATTERN.exec(url);
  return match ? match[1].toLowerCase() : null;
}

// Sanitize an image source before it reaches the DOM. Relative/local paths are
// passed through untouched; absolute URLs must use a safe scheme, otherwise an
// empty string is returned so the browser never fetches a dangerous resource.
export function proxyImg(imagePath: string) {
  if (typeof imagePath !== "string") return "";
  if (!imagePath.trim()) return "";
  const scheme = schemeOf(imagePath);
  if (scheme === null) return imagePath;
  return SAFE_IMAGE_SCHEMES.has(scheme) ? imagePath : "";
}

// urlTransform hook for react-markdown link/image URLs. Rejects dangerous
// schemes (javascript:, data:, vbscript:, ...) by returning an empty string,
// while leaving relative URLs and safe schemes intact.
export function sanitizeUrl(url: string): string {
  if (typeof url !== "string") return "";
  if (!url.trim()) return "";
  const scheme = schemeOf(url);
  if (scheme === null) return url;
  return SAFE_LINK_SCHEMES.has(scheme) ? url : "";
}

function escapeRegExp(text: string) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function extractQueryTerms(query: string): string[] {
  const trimmed = query.trim();
  if (!trimmed) return [];
  const terms = trimmed.split(/\s+/).filter(Boolean);
  if (terms.length > 1) {
    return Array.from(new Set(terms)).sort((a, b) => b.length - a.length);
  }
  return [trimmed];
}

export function renderHighlighted(text: unknown, query: string): ReactNode {
  const value = text == null ? "" : String(text);
  const terms = extractQueryTerms(query);
  if (!value || terms.length === 0) return value;

  const regex = new RegExp(`(${terms.map(escapeRegExp).join("|")})`, "gi");
  const parts = value.split(regex);
  const lowerTermSet = new Set(terms.map((term) => term.toLowerCase()));

  return parts.map((part, idx) => {
    const isMatch = lowerTermSet.has(part.toLowerCase());
    if (!isMatch) return <span key={idx}>{part}</span>;
    return (
      <mark key={idx} className="rounded bg-yellow-200 px-0.5 text-slate-900">
        {part}
      </mark>
    );
  });
}
