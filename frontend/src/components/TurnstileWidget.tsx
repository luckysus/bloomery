import { useEffect, useRef } from "react";

const TURNSTILE_SCRIPT_SRC = "https://challenges.cloudflare.com/turnstile/v0/api.js?onload=onTurnstileLoad";

type TurnstileTheme = "light" | "dark" | "auto";
type TurnstileSize = "normal" | "compact" | "flexible";

interface TurnstileRenderOptions {
  sitekey: string;
  callback: (token: string) => void;
  "expired-callback"?: () => void;
  "timeout-callback"?: () => void;
  "error-callback"?: () => void;
  theme?: TurnstileTheme;
  size?: TurnstileSize;
}

interface TurnstileApi {
  render: (container: HTMLElement, options: TurnstileRenderOptions) => string;
  reset: (widgetId?: string) => void;
  remove?: (widgetId?: string) => void;
}

declare global {
  interface Window {
    turnstile?: TurnstileApi;
    onTurnstileLoad?: () => void;
  }
}

interface TurnstileWidgetProps {
  siteKey: string;
  resetSignal: number;
  theme?: TurnstileTheme;
  size?: TurnstileSize;
  onToken: (token: string) => void;
  onError: (message: string) => void;
}

let turnstileScriptPromise: Promise<void> | null = null;

function loadTurnstileScript(): Promise<void> {
  if (window.turnstile) {
    return Promise.resolve();
  }

  if (turnstileScriptPromise) {
    return turnstileScriptPromise;
  }

  turnstileScriptPromise = new Promise<void>((resolve, reject) => {
    const existingScript = document.querySelector<HTMLScriptElement>('script[src*="turnstile/v0/api.js"]');

    window.onTurnstileLoad = () => {
      resolve();
    };

    if (existingScript) {
      existingScript.addEventListener("error", () => reject(new Error("Cloudflare Turnstile 加载失败")), { once: true });
      if (window.turnstile) {
        resolve();
      }
      return;
    }

    const script = document.createElement("script");
    script.src = TURNSTILE_SCRIPT_SRC;
    script.async = true;
    script.defer = true;
    script.onerror = () => reject(new Error("Cloudflare Turnstile 加载失败"));
    document.head.appendChild(script);
  });

  return turnstileScriptPromise;
}

export default function TurnstileWidget({
  siteKey,
  resetSignal,
  theme = "light",
  size = "normal",
  onToken,
  onError,
}: TurnstileWidgetProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const widgetIdRef = useRef<string>("");

  useEffect(() => {
    let cancelled = false;

    async function renderWidget() {
      if (!siteKey) return;

      try {
        await loadTurnstileScript();
        if (cancelled || !window.turnstile || !containerRef.current) return;

        if (widgetIdRef.current && window.turnstile.remove) {
          window.turnstile.remove(widgetIdRef.current);
          widgetIdRef.current = "";
        }

        containerRef.current.innerHTML = "";
        widgetIdRef.current = window.turnstile.render(containerRef.current, {
          sitekey: siteKey,
          theme,
          size,
          callback: (token: string) => onToken(token),
          "expired-callback": () => onToken(""),
          "timeout-callback": () => onToken(""),
          "error-callback": () => {
            onToken("");
            onError("Cloudflare Turnstile 验证加载失败，请刷新后重试");
          },
        });
      } catch (error: any) {
        if (!cancelled) {
          onToken("");
          onError(error?.message || String(error));
        }
      }
    }

    void renderWidget();

    return () => {
      cancelled = true;
      if (widgetIdRef.current && window.turnstile?.remove) {
        window.turnstile.remove(widgetIdRef.current);
      }
      widgetIdRef.current = "";
    };
  }, [siteKey, theme, size, onError, onToken]);

  useEffect(() => {
    if (widgetIdRef.current && window.turnstile) {
      window.turnstile.reset(widgetIdRef.current);
      onToken("");
    }
  }, [resetSignal, onToken]);

  if (!siteKey) return null;

  return (
    <div className="w-full rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
      <div ref={containerRef} className="min-h-[65px] w-full overflow-hidden [&_iframe]:!w-full" />
    </div>
  );
}
