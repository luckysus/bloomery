import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import type { WebMessage } from "./webTypes";
import { activeTurnIndex, buildUserTurns, railTranslate } from "./turnNavigatorModel";

const ITEM_HEIGHT = 24;
const EDGE = 44;

export default function WebTurnNavigator({
  messages,
  scrollContainerRef,
}: {
  messages: WebMessage[];
  scrollContainerRef: RefObject<HTMLDivElement>;
}) {
  const turns = useMemo(() => buildUserTurns(messages), [messages]);
  const railRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(turns.length > 0 ? 0 : -1);
  const [hovered, setHovered] = useState<number | null>(null);
  const [height, setHeight] = useState(0);

  const sync = useCallback(() => {
    const container = scrollContainerRef.current;
    if (!container || turns.length === 0) return;
    const rect = container.getBoundingClientRect();
    const offsets = turns.flatMap((turn) => {
      const anchor = container.querySelector<HTMLElement>(`[data-agent-user-turn="${turn.messageIndex}"]`);
      return anchor ? [anchor.getBoundingClientRect().top - rect.top + container.scrollTop] : [];
    });
    const index = activeTurnIndex(offsets, container.scrollTop, container.clientHeight);
    setActive(index < 0 ? 0 : index);
  }, [scrollContainerRef, turns]);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;
    const onScroll = () => window.requestAnimationFrame(sync);
    onScroll();
    container.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    return () => {
      container.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
    };
  }, [scrollContainerRef, sync]);

  useEffect(() => {
    const rail = railRef.current;
    if (!rail) return;
    const measure = () => setHeight(rail.clientHeight);
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [turns.length]);

  if (turns.length === 0) return null;
  const translate = railTranslate(active, turns.length, ITEM_HEIGHT, height, EDGE);
  const revealed = hovered;

  return (
    <nav aria-label="本次对话问题导航" className="pointer-events-none absolute inset-y-0 left-0 z-20 w-[var(--agent-turn-gutter)] max-md:hidden">
      <div className="absolute left-0 top-1/2 h-[68%] min-h-[160px] max-h-[460px] w-full -translate-y-1/2">
        <div ref={railRef} className="absolute inset-y-0 left-3 w-12 overflow-hidden" style={{ maskImage: "linear-gradient(to bottom, transparent, black 44px, black calc(100% - 44px), transparent)" }}>
          <div className="transition-transform duration-200 ease-out" style={{ transform: `translateY(${translate}px)` }}>
            {turns.map((turn, index) => (
              <button
                key={turn.messageIndex}
                type="button"
                aria-label={`跳转到问题 ${index + 1}：${turn.question}`}
                aria-current={active === index ? "location" : undefined}
                title={turn.question}
                className="group pointer-events-auto flex w-12 items-center justify-start outline-none"
                style={{ height: ITEM_HEIGHT }}
                onClick={() => {
                  const container = scrollContainerRef.current;
                  const anchor = container?.querySelector<HTMLElement>(`[data-agent-user-turn="${turn.messageIndex}"]`);
                  if (!container || !anchor) return;
                  const top = anchor.getBoundingClientRect().top - container.getBoundingClientRect().top + container.scrollTop - 20;
                  setActive(index);
                  container.scrollTo({ top: Math.max(0, top), behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
                }}
                onMouseEnter={() => setHovered(index)}
                onMouseLeave={() => setHovered(null)}
              >
                <span className={`block h-0.5 rounded-full ${revealed === index ? "w-8 bg-[#8a7668]" : active === index ? "w-6 bg-[#cc785c]" : "w-2 bg-[#b9aa9b]/70"}`} />
              </button>
            ))}
          </div>
        </div>
        {revealed !== null && (
          <div className="pointer-events-none absolute left-14 top-1/2 z-30 w-max max-w-72 -translate-y-1/2 rounded-md border border-[#e3d7ca] bg-[#fffaf3] px-3 py-2 text-sm leading-5 text-[#2b2118] shadow-[0_10px_24px_rgba(72,52,38,0.14)]">
            <span className="line-clamp-3">{turns[revealed]?.question}</span>
          </div>
        )}
      </div>
    </nav>
  );
}
