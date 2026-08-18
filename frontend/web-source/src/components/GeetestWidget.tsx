import { useCallback, useEffect, useRef, useState } from "react";

// 极验 Geetest v3 官方 SDK（使用官方稳定入口，内部会加载匹配的版本）
const GEETEST_SCRIPT_SRC = "https://static.geetest.com/static/tools/gt.js";

interface GeetestCaptcha {
  onReady: (handler: () => void) => GeetestCaptcha;
  onSuccess: (handler: () => void) => GeetestCaptcha;
  onError: (handler: () => void) => GeetestCaptcha;
  onClose?: (handler: () => void) => GeetestCaptcha;
  appendTo: (selector: string) => GeetestCaptcha;
  verify: () => void;
  reset: () => void;
  getValidate: () => {
    geetest_challenge: string;
    geetest_validate: string;
    geetest_seccode: string;
  } | null;
}

declare global {
  interface Window {
    initGeetest?: (
      config: Record<string, unknown>,
      callback: (captcha: GeetestCaptcha) => void,
    ) => void;
  }
}

interface GeetestWidgetProps {
  apiBase: string; // 例如 "/api/auth"
  resetSignal: number;
  onToken: (proof: string) => void;
  onError: (message: string) => void;
}

type WidgetStatus = "loading" | "ready" | "verified" | "error";

// 全局缓存：极验 SDK 只需加载一次
let geetestScriptLoaded = false;
let geetestScriptLoading: Promise<void> | null = null;

function loadGeetestScript(): Promise<void> {
  if (geetestScriptLoaded && window.initGeetest) {
    return Promise.resolve();
  }
  if (geetestScriptLoading) {
    return geetestScriptLoading;
  }
  geetestScriptLoading = new Promise<void>((resolve, reject) => {
    if (window.initGeetest) {
      geetestScriptLoaded = true;
      resolve();
      return;
    }
    const finish = () => {
      if (window.initGeetest) {
        geetestScriptLoaded = true;
        resolve();
        return true;
      }
      return false;
    };
    const existing = document.querySelector<HTMLScriptElement>('script[src*="geetest.com"]');
    if (existing) {
      const timer = window.setInterval(() => {
        if (finish()) window.clearInterval(timer);
      }, 100);
      window.setTimeout(() => {
        window.clearInterval(timer);
        if (!finish()) reject(new Error("极验脚本加载超时"));
      }, 10000);
      return;
    }
    const script = document.createElement("script");
    script.src = GEETEST_SCRIPT_SRC;
    script.async = true;
    script.onload = () => {
      const timer = window.setInterval(() => {
        if (finish()) window.clearInterval(timer);
      }, 50);
      window.setTimeout(() => {
        window.clearInterval(timer);
        if (!finish()) reject(new Error("极验脚本初始化失败"));
      }, 5000);
    };
    script.onerror = () => reject(new Error("极验脚本加载失败"));
    document.head.appendChild(script);
  }).finally(() => {
    geetestScriptLoading = null;
  });
  return geetestScriptLoading;
}

// popup 模式的内嵌验证条需要一个稳定的容器 id 供 SDK appendTo
let geetestBoxSeq = 0;

// 覆盖验证条默认文案「点击按钮进行验证」→「点击按钮开始验证」（仅待验证态，不影响检测中/成功态）
const GEETEST_STYLE_ID = "geetest-custom-style";
function ensureGeetestStyle() {
  if (document.getElementById(GEETEST_STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = GEETEST_STYLE_ID;
  style.textContent = `
    .geetest_holder.geetest_wind .geetest_radar_tip_content { font-size: 0 !important; }
    .geetest_holder.geetest_wind .geetest_radar_tip_content::after { content: "点击按钮开始验证"; font-size: 14px; }
  `;
  document.head.appendChild(style);
}

export default function GeetestWidget({ apiBase, resetSignal, onToken, onError }: GeetestWidgetProps) {
  const [status, setStatus] = useState<WidgetStatus>("loading");
  const [errorMsg, setErrorMsg] = useState("");
  const captchaRef = useRef<GeetestCaptcha | null>(null);
  const initedRef = useRef(false);
  const mountedRef = useRef(true);
  const onTokenRef = useRef(onToken);
  const onErrorRef = useRef(onError);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const boxIdRef = useRef(`geetest-box-${++geetestBoxSeq}`);

  useEffect(() => {
    onTokenRef.current = onToken;
    onErrorRef.current = onError;
  }, [onToken, onError]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const init = useCallback(async () => {
    if (initedRef.current) return;
    initedRef.current = true;
    try {
      setStatus("loading");
      setErrorMsg("");
      ensureGeetestStyle();
      await loadGeetestScript();

      // challenge 是一次性的，每次初始化都要重新获取
      const res = await fetch(`${apiBase}/geetest/register`, { credentials: "include" });
      const body = await res.json();
      if (!mountedRef.current) return;
      if (!body?.success || !body?.data) {
        throw new Error(body?.message || "极验初始化失败，请稍后重试");
      }

      const { gt, challenge, success, new_captcha } = body.data;
      if (!gt || !challenge || !window.initGeetest) {
        throw new Error("极验参数不完整");
      }

      // 重新初始化前清空容器，避免残留旧的验证条 DOM
      containerRef.current?.replaceChildren();

      window.initGeetest(
        {
          gt,
          challenge,
          offline: success === 0,
          new_captcha: !!new_captcha,
          product: "popup", // 内嵌验证条，点击后弹出拼图弹窗
          width: "100%",
          lang: "zh-cn",
        },
        (captcha: GeetestCaptcha) => {
          if (!mountedRef.current) return;
          captchaRef.current = captcha;
          captcha
            .onReady(() => {
              if (mountedRef.current) setStatus("ready");
            })
            .onSuccess(async () => {
              const result = captcha.getValidate();
              if (!result) return;
              // 后端二次校验期间保持验证条原样，避免下方闪现「加载中」占位条
              try {
                const vr = await fetch(`${apiBase}/geetest/validate`, {
                  method: "POST",
                  credentials: "include",
                  headers: { "Content-Type": "application/json" },
                  body: JSON.stringify({
                    challenge: result.geetest_challenge,
                    validate: result.geetest_validate,
                    seccode: result.geetest_seccode,
                  }),
                });
                const vb = await vr.json();
                if (!mountedRef.current) return;
                if (vb?.success) {
                  setStatus("verified");
                  onTokenRef.current(result.geetest_challenge);
                } else {
                  setStatus("error");
                  setErrorMsg(vb?.message || "极验验证失败");
                  onTokenRef.current("");
                  onErrorRef.current(vb?.message || "极验验证失败，请重试");
                  captcha.reset();
                }
              } catch (err: any) {
                if (!mountedRef.current) return;
                setStatus("error");
                setErrorMsg("验证请求失败");
                onTokenRef.current("");
                onErrorRef.current(err?.message || "极验验证请求失败");
                captcha.reset();
              }
            })
            .onError(() => {
              if (!mountedRef.current) return;
              setStatus("error");
              setErrorMsg("验证组件加载失败");
              onTokenRef.current("");
              onErrorRef.current("极验验证组件出错，请刷新后重试");
            });
          captcha.appendTo(`#${boxIdRef.current}`);
        },
      );
    } catch (error: any) {
      initedRef.current = false;
      if (!mountedRef.current) return;
      setStatus("error");
      setErrorMsg(error?.message || "初始化失败");
      onTokenRef.current("");
      onErrorRef.current(error?.message || String(error));
    }
  }, [apiBase]);

  useEffect(() => {
    void init();
    return () => {
      captchaRef.current = null;
      initedRef.current = false;
    };
  }, [init]);

  // 外部重置信号：重新获取 challenge 并回到待验证状态（首次挂载跳过）
  const skipFirstResetRef = useRef(true);
  useEffect(() => {
    if (skipFirstResetRef.current) {
      skipFirstResetRef.current = false;
      return;
    }
    captchaRef.current = null;
    initedRef.current = false;
    onTokenRef.current("");
    void init();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resetSignal]);

  const handleRetry = () => {
    initedRef.current = false;
    void init();
  };

  const placeholderClass =
    "w-full h-11 px-4 rounded-lg border text-sm font-medium flex items-center justify-center gap-2";

  return (
    <div className="w-full">
      <span className="mb-1.5 block text-[13px] font-medium tracking-normal text-[#6f6258]">
        行为验证（不要点最右边的小按钮）
      </span>
      {/* 极验 popup 内嵌验证条容器：点击验证条弹出拼图弹窗，成功后自动进入已验证态 */}
      <div id={boxIdRef.current} ref={containerRef} className="w-full" />
      {status === "loading" && (
        <div className={`${placeholderClass} border-slate-200 bg-slate-50 text-slate-400`}>加载中…</div>
      )}
      {status === "error" && (
        <button
          type="button"
          onClick={handleRetry}
          className={`${placeholderClass} cursor-pointer border-rose-300 bg-rose-50 text-rose-700 transition-colors hover:bg-rose-100`}
        >
          {`${errorMsg || "验证失败"}，点击重试`}
        </button>
      )}
    </div>
  );
}
