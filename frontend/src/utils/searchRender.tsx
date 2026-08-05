import type { ReactNode } from "react";

export function proxyImg(imagePath: string) {
  return imagePath;
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
