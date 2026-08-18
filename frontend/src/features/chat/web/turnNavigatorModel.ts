import type { WebMessage } from "./webTypes";

export interface WebUserTurn {
  messageIndex: number;
  question: string;
}

export function buildUserTurns(messages: readonly Pick<WebMessage, "role" | "content">[]) {
  return messages.flatMap((message, messageIndex) => message.role === "user" && message.content.trim()
    ? [{ messageIndex, question: message.content.trim() }]
    : []);
}

export function activeTurnIndex(offsets: readonly number[], scrollTop: number, viewportHeight: number) {
  if (offsets.length === 0) return -1;
  const line = scrollTop + Math.max(0, viewportHeight) * 0.32;
  let active = 0;
  for (let index = 0; index < offsets.length; index += 1) {
    if (offsets[index] > line) break;
    active = index;
  }
  return active;
}

export function railTranslate(active: number, count: number, height: number, viewport: number, edge: number) {
  if (count <= 0 || height <= 0 || viewport <= 0) return 0;
  const total = count * height;
  if (total <= viewport - edge * 2) return (viewport - total) / 2;
  const bounded = Math.min(Math.max(active, 0), count - 1);
  const centered = viewport / 2 - (bounded + 0.5) * height;
  return Math.min(edge, Math.max(viewport - edge - total, centered));
}
