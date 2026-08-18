import type { AgentMessage } from "../../agent/types";

export interface AgentUserTurn {
  messageIndex: number;
  question: string;
}

type TurnMessage = Pick<AgentMessage, "role" | "content">;

const TURN_WAVE_WIDTHS = [32, 24, 16, 12] as const;
const IDLE_TURN_WIDTH = 8;

export function buildUserTurns(messages: readonly TurnMessage[]): AgentUserTurn[] {
  const turns: AgentUserTurn[] = [];

  messages.forEach((message, messageIndex) => {
    if (message.role !== "user") return;

    const question = message.content.trim();
    if (question) turns.push({ messageIndex, question });
  });

  return turns;
}

export function getActiveTurnIndex(
  anchorOffsets: readonly number[],
  scrollTop: number,
  viewportHeight: number,
): number {
  if (anchorOffsets.length === 0) return -1;

  const readingLine = scrollTop + Math.max(0, viewportHeight) * 0.32;
  let activeIndex = 0;

  for (let index = 0; index < anchorOffsets.length; index += 1) {
    if (anchorOffsets[index] > readingLine) break;
    activeIndex = index;
  }

  return activeIndex;
}

export function calculateTurnWaveWidth(
  turnIndex: number,
  revealedIndex: number | null,
): number {
  if (revealedIndex === null) return IDLE_TURN_WIDTH;
  return TURN_WAVE_WIDTHS[Math.abs(turnIndex - revealedIndex)] ?? IDLE_TURN_WIDTH;
}

export function getRevealedTurnIndex(
  hoveredIndex: number | null,
  focusedIndex: number | null,
): number | null {
  return focusedIndex ?? hoveredIndex;
}

interface TurnRailLayout {
  activeIndex: number;
  itemCount: number;
  itemHeight: number;
  viewportHeight: number;
  edgePadding: number;
}

export function calculateTurnRailTranslate({
  activeIndex,
  itemCount,
  itemHeight,
  viewportHeight,
  edgePadding,
}: TurnRailLayout): number {
  if (itemCount <= 0 || itemHeight <= 0 || viewportHeight <= 0) return 0;

  const totalHeight = itemCount * itemHeight;
  const padding = Math.max(0, edgePadding);
  if (totalHeight <= viewportHeight - padding * 2) {
    return (viewportHeight - totalHeight) / 2;
  }

  const boundedActiveIndex = Math.min(Math.max(activeIndex, 0), itemCount - 1);
  const centered = viewportHeight / 2 - (boundedActiveIndex + 0.5) * itemHeight;
  const minimum = viewportHeight - padding - totalHeight;

  return Math.min(padding, Math.max(minimum, centered));
}
