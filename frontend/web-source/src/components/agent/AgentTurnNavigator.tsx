import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import type { AgentMessage } from "../../agent/types";
import {
  buildUserTurns,
  calculateTurnRailTranslate,
  calculateTurnWaveWidth,
  getActiveTurnIndex,
  getRevealedTurnIndex,
} from "./agentTurnNavigator";

const TURN_ITEM_HEIGHT = 24;
const RAIL_EDGE_PADDING = 44;

interface AgentTurnNavigatorProps {
  messages: AgentMessage[];
  scrollContainerRef: RefObject<HTMLDivElement>;
}

export default function AgentTurnNavigator({
  messages,
  scrollContainerRef,
}: AgentTurnNavigatorProps) {
  const turns = useMemo(() => buildUserTurns(messages), [messages]);
  const railViewportRef = useRef<HTMLDivElement>(null);
  const [activeIndex, setActiveIndex] = useState(turns.length > 0 ? 0 : -1);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
  const [railViewportHeight, setRailViewportHeight] = useState(0);
  const revealedIndex = getRevealedTurnIndex(hoveredIndex, focusedIndex);

  const syncActiveTurn = useCallback(() => {
    const scrollContainer = scrollContainerRef.current;
    if (!scrollContainer || turns.length === 0) {
      setActiveIndex(-1);
      return;
    }

    const containerRect = scrollContainer.getBoundingClientRect();
    const anchoredTurns = turns.flatMap((turn, turnIndex) => {
      const anchor = scrollContainer.querySelector<HTMLElement>(
        `[data-agent-user-turn="${turn.messageIndex}"]`,
      );
      if (!anchor) return [];

      return [{
        offset: anchor.getBoundingClientRect().top - containerRect.top + scrollContainer.scrollTop,
        turnIndex,
      }];
    });
    const anchoredIndex = getActiveTurnIndex(
      anchoredTurns.map(({ offset }) => offset),
      scrollContainer.scrollTop,
      scrollContainer.clientHeight,
    );

    setActiveIndex(anchoredIndex < 0 ? 0 : anchoredTurns[anchoredIndex].turnIndex);
  }, [scrollContainerRef, turns]);

  useEffect(() => {
    const scrollContainer = scrollContainerRef.current;
    if (!scrollContainer) return;

    let animationFrame = 0;
    const scheduleSync = () => {
      window.cancelAnimationFrame(animationFrame);
      animationFrame = window.requestAnimationFrame(syncActiveTurn);
    };

    scheduleSync();
    scrollContainer.addEventListener("scroll", scheduleSync, { passive: true });
    window.addEventListener("resize", scheduleSync);

    const resizeObserver = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(scheduleSync);
    resizeObserver?.observe(scrollContainer);
    if (scrollContainer.firstElementChild) {
      resizeObserver?.observe(scrollContainer.firstElementChild);
    }

    return () => {
      window.cancelAnimationFrame(animationFrame);
      scrollContainer.removeEventListener("scroll", scheduleSync);
      window.removeEventListener("resize", scheduleSync);
      resizeObserver?.disconnect();
    };
  }, [scrollContainerRef, syncActiveTurn]);

  useEffect(() => {
    const railViewport = railViewportRef.current;
    if (!railViewport) return;

    const measure = () => setRailViewportHeight(railViewport.clientHeight);
    measure();
    window.addEventListener("resize", measure);

    const resizeObserver = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(measure);
    resizeObserver?.observe(railViewport);

    return () => {
      window.removeEventListener("resize", measure);
      resizeObserver?.disconnect();
    };
  }, [turns.length]);

  const scrollToTurn = (messageIndex: number, turnIndex: number) => {
    const scrollContainer = scrollContainerRef.current;
    const anchor = scrollContainer?.querySelector<HTMLElement>(
      `[data-agent-user-turn="${messageIndex}"]`,
    );
    if (!scrollContainer || !anchor) return;

    const top = anchor.getBoundingClientRect().top
      - scrollContainer.getBoundingClientRect().top
      + scrollContainer.scrollTop
      - 20;
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    setActiveIndex(turnIndex);
    scrollContainer.scrollTo({
      top: Math.max(0, top),
      behavior: reduceMotion ? "auto" : "smooth",
    });
  };

  if (turns.length === 0) return null;

  const railTranslate = calculateTurnRailTranslate({
    activeIndex: focusedIndex ?? activeIndex,
    itemCount: turns.length,
    itemHeight: TURN_ITEM_HEIGHT,
    viewportHeight: railViewportHeight,
    edgePadding: RAIL_EDGE_PADDING,
  });

  return (
    <nav
      aria-label="本次对话问题导航"
      className="pointer-events-none absolute inset-y-0 left-0 z-20 w-[var(--agent-turn-gutter)] max-md:hidden"
    >
      <div className="absolute left-0 top-1/2 h-[68%] min-h-[160px] max-h-[460px] w-full -translate-y-1/2">
        <div
          ref={railViewportRef}
          className="absolute inset-y-0 left-3 w-12 overflow-hidden"
          style={{
            maskImage: "linear-gradient(to bottom, transparent, black 44px, black calc(100% - 44px), transparent)",
            WebkitMaskImage: "linear-gradient(to bottom, transparent, black 44px, black calc(100% - 44px), transparent)",
          }}
        >
          <div
            className="will-change-transform transition-transform duration-200 ease-out motion-reduce:transition-none"
            style={{ transform: `translateY(${railTranslate}px)` }}
          >
            {turns.map((turn, index) => {
              const active = index === activeIndex;
              const revealed = index === revealedIndex;
              return (
                <button
                  key={turn.messageIndex}
                  type="button"
                  aria-label={`跳转到问题 ${index + 1}：${turn.question}`}
                  aria-current={active ? "location" : undefined}
                  aria-describedby={revealedIndex === index ? `agent-turn-question-${index}` : undefined}
                  title={turn.question}
                  className="group pointer-events-auto flex w-12 items-center justify-start outline-none"
                  style={{ height: TURN_ITEM_HEIGHT }}
                  onClick={() => scrollToTurn(turn.messageIndex, index)}
                  onMouseEnter={() => setHoveredIndex(index)}
                  onMouseLeave={() => setHoveredIndex(null)}
                  onFocus={(event) => setFocusedIndex(
                    event.currentTarget.matches(":focus-visible") ? index : null,
                  )}
                  onBlur={() => setFocusedIndex(null)}
                >
                  <span
                    aria-hidden="true"
                    className={`block h-0.5 rounded-full transition-[width,background-color] duration-150 motion-reduce:transition-none ${
                      revealed
                        ? "bg-[#8a7668]"
                        : active
                          ? "bg-[#cc785c]"
                          : "bg-[#b9aa9b]/70"
                    }`}
                    style={{ width: calculateTurnWaveWidth(index, revealedIndex) }}
                  />
                </button>
              );
            })}
          </div>
        </div>

        {turns.map((turn, index) => {
          if (revealedIndex !== index) return null;
          const top = Math.min(
            Math.max(railTranslate + (index + 0.5) * TURN_ITEM_HEIGHT, RAIL_EDGE_PADDING),
            Math.max(RAIL_EDGE_PADDING, railViewportHeight - RAIL_EDGE_PADDING),
          );

          return (
            <div
              key={turn.messageIndex}
              id={`agent-turn-question-${index}`}
              role="tooltip"
              className="absolute left-14 z-30 w-max max-w-72 -translate-y-1/2 rounded-md border border-[#e3d7ca] bg-[#fffaf3] px-3 py-2 text-sm leading-5 text-[#2b2118] shadow-[0_10px_24px_rgba(72,52,38,0.14)] pointer-events-none"
              style={{ top }}
            >
              <span className="line-clamp-3">{turn.question}</span>
            </div>
          );
        })}
      </div>
    </nav>
  );
}
