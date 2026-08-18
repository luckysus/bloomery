import { useCallback, useEffect, useRef, useState } from "react";

const STATS_POS_KEY = "system_stats_position";

function getDefaultStatsPosition(): { x: number; y: number } {
  const saved = localStorage.getItem(STATS_POS_KEY);
  if (saved) {
    try {
      const pos = JSON.parse(saved);
      if (typeof pos.x === "number" && typeof pos.y === "number") return pos;
    } catch {
      // Ignore invalid saved layout state.
    }
  }
  return { x: window.innerWidth - 88, y: Math.max(0, Math.round(window.innerHeight / 2 - 110)) };
}

export default function SystemStatsIndicator() {
  const [stats, setStats] = useState<{ cpu_percent: number; gpu_percent: number; gpu_memory_percent: number } | null>(null);
  const [offline, setOffline] = useState(false);
  const [pos, setPos] = useState(getDefaultStatsPosition);
  const [dragging, setDragging] = useState(false);
  // 窄屏不渲染：竖条浮窗按桌面宽度定位，在手机上只会遮挡内容。
  // 用状态而非 CSS 隐藏，因为内联 style 的 display 会覆盖 max-md:hidden。
  const [isNarrow, setIsNarrow] = useState(
    () => typeof window !== "undefined" && window.matchMedia("(max-width: 767px)").matches,
  );
  const dragOffset = useRef({ dx: 0, dy: 0 });
  const containerRef = useRef<HTMLDivElement>(null);
  const posRef = useRef(pos);

  useEffect(() => {
    const mql = window.matchMedia("(max-width: 767px)");
    const sync = () => setIsNarrow(mql.matches);
    mql.addEventListener("change", sync);
    return () => mql.removeEventListener("change", sync);
  }, []);

  useEffect(() => {
    posRef.current = pos;
  }, [pos]);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    dragOffset.current = { dx: e.clientX - rect.left, dy: e.clientY - rect.top };
    setDragging(true);
    if (containerRef.current) {
      containerRef.current.style.transition = "none";
    }
  }, []);

  useEffect(() => {
    if (!dragging) return;
    const onMouseMove = (e: MouseEvent) => {
      const el = containerRef.current;
      if (!el) return;
      const w = el.offsetWidth;
      const h = el.offsetHeight;
      let nx = e.clientX - dragOffset.current.dx;
      let ny = e.clientY - dragOffset.current.dy;
      nx = Math.max(0, Math.min(window.innerWidth - w, nx));
      ny = Math.max(0, Math.min(window.innerHeight - h, ny));
      posRef.current = { x: nx, y: ny };
      el.style.left = `${nx}px`;
      el.style.top = `${ny}px`;
    };
    const onMouseUp = () => {
      setDragging(false);
      const finalPos = posRef.current;
      setPos(finalPos);
      localStorage.setItem(STATS_POS_KEY, JSON.stringify(finalPos));
      if (containerRef.current) {
        containerRef.current.style.transition = "";
      }
    };
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
    return () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };
  }, [dragging]);

  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | null = null;

    const fetchStats = async () => {
      if (document.hidden) return;
      const controller = new AbortController();
      const timeout = setTimeout(() => controller.abort(), 3000);
      try {
        const res = await fetch("/api/system_stats", { signal: controller.signal });
        clearTimeout(timeout);
        if (!res.ok) throw new Error("not ok");
        const data = await res.json();
        setStats(data);
        setOffline(false);
      } catch {
        clearTimeout(timeout);
        setOffline(true);
      }
    };

    fetchStats();
    timer = setInterval(fetchStats, 3000);

    const onVisChange = () => {
      if (!document.hidden) fetchStats();
    };
    document.addEventListener("visibilitychange", onVisChange);

    return () => {
      if (timer) clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisChange);
    };
  }, []);

  const getColor = (pct: number) => {
    if (pct >= 80) return "#ef4444";
    if (pct >= 50) return "#eab308";
    return "#22c55e";
  };

  const renderBar = (label: string, percent: number, tooltip?: string) => (
    <div className="flex flex-col items-center gap-1.5" title={tooltip}>
      <span style={{ fontSize: 12, fontWeight: 600, color: offline ? "#8e8b82" : "#6c6a64", letterSpacing: "0.03em" }}>{label}</span>
      <div style={{
        position: "relative", width: 15, height: 150, borderRadius: 8,
        backgroundColor: "rgba(148,163,184,0.18)", overflow: "hidden",
      }}>
        <div style={{
          position: "absolute", bottom: 0, left: 0, right: 0,
          height: offline ? 0 : `${percent}%`,
          background: offline ? "#cbd5e1" : getColor(percent),
          borderRadius: 8,
          transition: "height 0.6s cubic-bezier(.4,0,.2,1), background 0.4s",
        }} />
      </div>
      <span style={{ fontSize: 12, fontWeight: 700, fontFamily: "monospace", color: offline ? "#8e8b82" : getColor(percent), minWidth: 32, textAlign: "center" }}>
        {offline ? "--" : `${Math.round(percent)}%`}
      </span>
    </div>
  );

  const cpu = stats?.cpu_percent ?? 0;
  const gpu = stats?.gpu_percent ?? 0;
  const gpuMem = stats?.gpu_memory_percent ?? 0;

  if (isNarrow) return null;

  return (
    <div
      ref={containerRef}
      onMouseDown={onMouseDown}
      style={{
        position: "fixed", left: pos.x, top: pos.y,
        zIndex: 50, display: "flex", gap: 10,
        background: "rgba(255,255,255,0.75)", backdropFilter: "blur(8px)",
        borderRadius: 12, padding: "10px 10px",
        border: "1px solid rgba(148,163,184,0.25)",
        boxShadow: "0 1px 4px rgba(0,0,0,0.06)",
        cursor: dragging ? "grabbing" : "grab",
        userSelect: "none",
      }}
    >
      {renderBar("CPU", cpu)}
      {renderBar("GPU", gpu, `显存占用: ${offline ? "--" : Math.round(gpuMem) + "%"}`)}
    </div>
  );
}
