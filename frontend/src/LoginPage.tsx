import { CheckCircle2, Eye, EyeOff, Lock, Mail, ShieldCheck, User, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { AuthBrandPanel } from "./components/auth/AuthBrandPanel";
import { AuthField } from "./components/auth/AuthField";
import LegalDocsModal, { type LegalDocKind } from "./components/auth/LegalDocsModal";
import TurnstileWidget from "./components/TurnstileWidget";
import GeetestWidget from "./components/GeetestWidget";
import { isTauriRuntime, saveDesktopAuthSession } from "./desktop/services/tauri";
import { authHeaders as apiAuthHeaders, getApiBase, setDesktopSessionToken } from "./services/api";

type AuthMode = "login" | "register" | "forgot" | "reset";

export interface AuthUserInfo {
  username: string;
  role?: string;
  email?: string;
  session_token?: string;
}

interface LoginPageProps {
  onLogin: (user?: AuthUserInfo) => void;
  authClient?: "web";
}

type CaptchaProvider = "turnstile" | "geetest" | "slider" | "none";

interface CaptchaConfig {
  provider: CaptchaProvider;
  turnstile_site_key: string;
  geetest_captcha_id: string;
}

interface AuthSecurityConfig {
  registration_enabled: boolean;
  email_verify_enabled: boolean;
  password_reset_enabled: boolean;
  frontend_url: string;
}

const FIELD_LABELS: Record<string, string> = {
  username: "用户名",
  email: "邮箱",
  password: "密码",
  token: "重置链接",
  code: "邮箱验证码",
  captcha_token: "滑块验证",
  turnstile_token: "人机验证",
  geetest_token: "极验验证",
};

function formatValidationIssue(issue: any): string {
  const loc = Array.isArray(issue?.loc) ? issue.loc : [];
  const field = String(loc[loc.length - 1] || "");
  const label = FIELD_LABELS[field] || field || "输入内容";
  const type = String(issue?.type || "");
  const ctx = issue?.ctx || {};

  if (type.includes("string_too_short")) return `${label}至少 ${ctx.min_length || 1} 个字符`;
  if (type.includes("string_too_long")) return `${label}最多 ${ctx.max_length || ""} 个字符`;
  if (type.includes("missing")) return `请填写${label}`;
  if (typeof issue?.msg === "string" && issue.msg) return `${label}：${issue.msg}`;
  return `${label}格式不正确`;
}

function getApiErrorMessage(payload: any, fallback: string): string {
  const detail = payload?.detail;
  if (typeof detail === "string") return detail;
  if (Array.isArray(detail)) {
    const messages = detail.map(formatValidationIssue).filter(Boolean);
    return messages.join("；") || fallback;
  }
  if (detail && typeof detail === "object") return detail.message || detail.msg || fallback;
  if (typeof payload?.message === "string") return payload.message;
  return fallback;
}

export default function LoginPage({ onLogin, authClient = "web" }: LoginPageProps) {
  const [mode, setMode] = useState<AuthMode>("login");
  const [username, setUsername] = useState(() => {
    try {
      return localStorage.getItem("bloomery_remember_username") || "";
    } catch {
      return "";
    }
  });
  const [rememberAccount, setRememberAccount] = useState(() => {
    try {
      return Boolean(localStorage.getItem("bloomery_remember_username"));
    } catch {
      return false;
    }
  });
  const [email, setEmail] = useState("");
  const [resetToken, setResetToken] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [emailCode, setEmailCode] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirmPassword, setShowConfirmPassword] = useState(false);
  const [error, setError] = useState("");
  const [info, setInfo] = useState("");
  const [loading, setLoading] = useState(false);
  const [codeLoading, setCodeLoading] = useState(false);
  const [sliderOpen, setSliderOpen] = useState(false);
  const [sliderChallengeId, setSliderChallengeId] = useState("");
  const [sliderValue, setSliderValue] = useState(0);
  const [sliderStartAt, setSliderStartAt] = useState(0);
  const [sliderLoading, setSliderLoading] = useState(false);
  const [sliderError, setSliderError] = useState("");
  const [sliderDragging, setSliderDragging] = useState(false);
  const [sliderPurpose, setSliderPurpose] = useState<"email" | "login" | "register">("email");
  const [captchaConfig, setCaptchaConfig] = useState<CaptchaConfig>({ provider: "none", turnstile_site_key: "", geetest_captcha_id: "" });
  const [authSecurityConfig, setAuthSecurityConfig] = useState<AuthSecurityConfig>({
    registration_enabled: true,
    email_verify_enabled: true,
    password_reset_enabled: true,
    frontend_url: "",
  });
  const [turnstileToken, setTurnstileToken] = useState("");
  const [turnstileResetSignal, setTurnstileResetSignal] = useState(0);
  const [geetestToken, setGeetestToken] = useState("");
  const [geetestResetSignal, setGeetestResetSignal] = useState(0);
  const [agreeLegal, setAgreeLegal] = useState(() => {
    try {
      return localStorage.getItem("bloomery_legal_agreed") === "1";
    } catch {
      return false;
    }
  });
  const [legalDoc, setLegalDoc] = useState<LegalDocKind | null>(null);
  const sliderTrackRef = useRef<HTMLDivElement | null>(null);

  const isRegister = mode === "register";
  const isForgot = mode === "forgot";
  const isReset = mode === "reset";
  // Tauri 桌面端不加载人机验证组件，后端凭 X-Desktop-Client 头豁免验证码
  const isDesktopRuntime = isTauriRuntime();
  const activeProvider: CaptchaProvider = isDesktopRuntime ? "none" : captchaConfig.provider;
  const widgetVerificationEnabled = activeProvider === "turnstile" || activeProvider === "geetest";
  const authHeaders = apiAuthHeaders({
    "Content-Type": "application/json",
    ...(isDesktopRuntime ? { "X-Desktop-Client": "1" } : {}),
  });

  const resetCaptcha = () => {
    setTurnstileToken("");
    setTurnstileResetSignal((value) => value + 1);
    setGeetestToken("");
    setGeetestResetSignal((value) => value + 1);
  };

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const path = window.location.pathname.toLowerCase();
    const tokenFromUrl = params.get("token") || "";
    const emailFromUrl = params.get("email") || "";
    if ((path.endsWith("/reset-password") || tokenFromUrl) && tokenFromUrl) {
      setMode("reset");
      setResetToken(tokenFromUrl);
      setEmail(emailFromUrl);
      setInfo("请设置你的新密码。");
      window.history.replaceState({}, document.title, window.location.origin + window.location.pathname);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function loadPublicSecurityConfig() {
      try {
        const [captchaResp, authResp] = await Promise.all([
          fetch(`${getApiBase()}/api/auth/captcha-config`),
          fetch(`${getApiBase()}/api/auth/security-config`),
        ]);
        if (captchaResp.ok) {
          const json = await captchaResp.json();
          if (!cancelled) {
            const provider: CaptchaProvider = ["turnstile", "geetest", "slider", "none"].includes(json?.provider)
              ? (json.provider as CaptchaProvider)
              : "none";
            setCaptchaConfig({
              provider,
              turnstile_site_key: String(json?.turnstile_site_key || ""),
              geetest_captcha_id: String(json?.geetest_captcha_id || ""),
            });
          }
        }
        if (authResp.ok) {
          const authJson = await authResp.json();
          if (!cancelled) {
            setAuthSecurityConfig({
              registration_enabled: authJson?.registration_enabled !== false,
              email_verify_enabled: authJson?.email_verify_enabled !== false,
              password_reset_enabled: authJson?.password_reset_enabled !== false,
              frontend_url: String(authJson?.frontend_url || ""),
            });
          }
        }
      } catch {
        if (!cancelled) setCaptchaConfig({ provider: "none", turnstile_site_key: "", geetest_captcha_id: "" });
      }
    }
    void loadPublicSecurityConfig();
    return () => {
      cancelled = true;
    };
  }, []);

  const requestEmailAction = async (captchaToken: string) => {
    if (!email.trim()) {
      setError("请先输入邮箱地址");
      return;
    }
    if (isRegister && !authSecurityConfig.email_verify_enabled) {
      setError("当前注册未启用邮箱验证，无需发送验证码");
      return;
    }
    if (isForgot && !authSecurityConfig.password_reset_enabled) {
      setError("当前未开启忘记密码邮箱重置");
      return;
    }
    if (activeProvider === "turnstile" && !turnstileToken) {
      setError("请先完成 Cloudflare Turnstile 验证");
      return;
    }
    if (activeProvider === "geetest" && !geetestToken) {
      setError("请先完成极验验证");
      return;
    }

    setCodeLoading(true);
    setError("");
    setInfo("");
    try {
      const endpoint = isForgot ? "/api/auth/forgot-password" : "/api/auth/send-code";
      const captchaFields = {
        captcha_token: activeProvider === "slider" ? captchaToken : null,
        turnstile_token: activeProvider === "turnstile" ? turnstileToken : null,
        geetest_token: activeProvider === "geetest" ? geetestToken : null,
      };
      const body = isForgot
        ? { email: email.trim(), ...captchaFields }
        : { email: email.trim(), username: username.trim(), purpose: "register", ...captchaFields };
      const resp = await fetch(`${getApiBase()}${endpoint}`, {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify(body),
      });
      const json = await resp.json().catch(() => ({}));
      const fallback = isForgot ? "重置链接发送失败" : "验证码发送失败";
      if (!resp.ok) throw new Error(getApiErrorMessage(json, fallback));
      setInfo(json.message || (isForgot ? "密码重置链接已发送，请查收邮箱。" : "验证码已发送"));
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setCodeLoading(false);
      if (widgetVerificationEnabled) resetCaptcha();
    }
  };

  const openSliderChallenge = async (purpose: "email" | "login" | "register") => {
    setError("");
    setInfo("");
    setSliderError("");
    setSliderValue(0);
    setSliderLoading(true);
    try {
      const resp = await fetch(`${getApiBase()}/api/auth/slider-captcha`);
      const json = await resp.json().catch(() => ({}));
      if (!resp.ok) throw new Error(getApiErrorMessage(json, "滑块验证加载失败"));
      setSliderChallengeId(json.challenge_id || "");
      setSliderPurpose(purpose);
      setSliderOpen(true);
      setSliderStartAt(Date.now());
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setSliderLoading(false);
    }
  };

  const openSliderCaptcha = async () => {
    if (!email.trim()) {
      setError("请先输入邮箱地址");
      return;
    }
    if (activeProvider !== "slider") {
      await requestEmailAction("");
      return;
    }
    await openSliderChallenge("email");
  };

  const verifySliderCaptcha = async () => {
    if (!sliderChallengeId || sliderLoading) return;
    setSliderLoading(true);
    setSliderError("");
    try {
      const resp = await fetch(`${getApiBase()}/api/auth/slider-captcha/verify`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          challenge_id: sliderChallengeId,
          position: 1,
          duration_ms: Date.now() - sliderStartAt,
        }),
      });
      const json = await resp.json().catch(() => ({}));
      if (!resp.ok) throw new Error(getApiErrorMessage(json, "滑块验证失败"));
      const captchaToken = json.captcha_token || "";
      setSliderOpen(false);
      setSliderValue(0);
      setSliderChallengeId("");
      if (sliderPurpose === "login") {
        await submitLogin(captchaToken);
      } else if (sliderPurpose === "register") {
        await submitRegister(captchaToken);
      } else {
        await requestEmailAction(captchaToken);
      }
    } catch (err: any) {
      setSliderError(err.message || String(err));
      setSliderValue(0);
      setSliderStartAt(Date.now());
      try {
        const resp = await fetch(`${getApiBase()}/api/auth/slider-captcha`);
        const json = await resp.json().catch(() => ({}));
        if (resp.ok) setSliderChallengeId(json.challenge_id || "");
      } catch {
        // The visible error already tells the user to retry.
      }
    } finally {
      setSliderLoading(false);
    }
  };

  useEffect(() => {
    if (!sliderDragging) return;

    const updateFromPointer = (clientX: number) => {
      const track = sliderTrackRef.current;
      if (!track) return;
      const rect = track.getBoundingClientRect();
      const handleWidth = 40;
      const maxTravel = Math.max(1, rect.width - handleWidth - 8);
      const next = ((clientX - rect.left - handleWidth / 2) / maxTravel) * 100;
      setSliderValue(Math.min(100, Math.max(0, next)));
    };

    const handlePointerMove = (event: PointerEvent) => {
      event.preventDefault();
      updateFromPointer(event.clientX);
    };

    const handlePointerUp = (event: PointerEvent) => {
      event.preventDefault();
      setSliderDragging(false);
      const track = sliderTrackRef.current;
      if (!track) {
        setSliderValue(0);
        return;
      }
      const rect = track.getBoundingClientRect();
      const handleWidth = 40;
      const maxTravel = Math.max(1, rect.width - handleWidth - 8);
      const next = Math.min(100, Math.max(0, ((event.clientX - rect.left - handleWidth / 2) / maxTravel) * 100));
      if (next >= 98) {
        setSliderValue(100);
        void verifySliderCaptcha();
      } else {
        setSliderValue(0);
      }
    };

    window.addEventListener("pointermove", handlePointerMove, { passive: false });
    window.addEventListener("pointerup", handlePointerUp, { passive: false });
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
    };
  }, [sliderDragging, sliderChallengeId, sliderLoading, sliderStartAt]);

  const submitLogin = async (sliderToken?: string) => {
    if (!username.trim() || !password) {
      setError("请输入用户名和密码");
      return;
    }
    if (!agreeLegal) {
      setError("请先阅读并同意《服务条款》和《隐私政策》");
      return;
    }
    if (activeProvider === "turnstile" && !turnstileToken) {
      setError("请先完成 Cloudflare Turnstile 验证");
      return;
    }
    if (activeProvider === "geetest" && !geetestToken) {
      setError("请先完成极验验证");
      return;
    }
    if (activeProvider === "slider" && !sliderToken) {
      await openSliderChallenge("login");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const resp = await fetch(`${getApiBase()}/api/auth/login`, {
        method: "POST",
        credentials: "include",
        headers: authHeaders,
        body: JSON.stringify({
          username: username.trim(),
          password,
          captcha_token: activeProvider === "slider" ? (sliderToken || null) : null,
          turnstile_token: activeProvider === "turnstile" ? turnstileToken : null,
          geetest_token: activeProvider === "geetest" ? geetestToken : null,
        }),
      });
      const json = await resp.json().catch(() => ({}));
      if (!resp.ok) throw new Error(getApiErrorMessage(json, "用户名或密码不正确"));
      try {
        if (rememberAccount) localStorage.setItem("bloomery_remember_username", username.trim());
        else localStorage.removeItem("bloomery_remember_username");
      } catch {
        // Remembering the account is a convenience only.
      }
      const loggedInUser = {
        username: String(json?.username || username.trim()),
        role: typeof json?.role === "string" ? json.role : undefined,
        email: typeof json?.email === "string" ? json.email : undefined,
      };
      if (isDesktopRuntime) {
        const sessionToken = typeof json?.session_token === "string" ? json.session_token : "";
        setDesktopSessionToken(sessionToken);
        await saveDesktopAuthSession({ ...loggedInUser, session_token: sessionToken }).catch(() => {});
      }
      onLogin(loggedInUser);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
      if (widgetVerificationEnabled) resetCaptcha();
    }
  };

  const submitRegister = async (sliderToken?: string) => {
    if (!authSecurityConfig.registration_enabled) {
      setError("当前系统未开放注册");
      return;
    }
    if (!username.trim() || !email.trim() || !password || !confirmPassword || (authSecurityConfig.email_verify_enabled && !emailCode.trim())) {
      setError("请完整填写注册信息");
      return;
    }
    if (!agreeLegal) {
      setError("请先阅读并同意《服务条款》和《隐私政策》");
      return;
    }
    if (username.trim().length < 3) {
      setError("用户名至少 3 个字符");
      return;
    }
    if (password !== confirmPassword) {
      setError("两次输入的密码不一致");
      return;
    }
    if (activeProvider === "turnstile" && !turnstileToken) {
      setError("请先完成 Cloudflare Turnstile 验证");
      return;
    }
    if (activeProvider === "geetest" && !geetestToken) {
      setError("请先完成极验验证");
      return;
    }
    if (activeProvider === "slider" && !sliderToken) {
      await openSliderChallenge("register");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const resp = await fetch(`${getApiBase()}/api/auth/register`, {
        method: "POST",
        credentials: "include",
        headers: authHeaders,
        body: JSON.stringify({
          username: username.trim(),
          email: email.trim(),
          password,
          code: authSecurityConfig.email_verify_enabled ? emailCode.trim() : "disabled",
          captcha_token: activeProvider === "slider" ? (sliderToken || null) : null,
          turnstile_token: activeProvider === "turnstile" ? turnstileToken : null,
          geetest_token: activeProvider === "geetest" ? geetestToken : null,
        }),
      });
      const json = await resp.json().catch(() => ({}));
      if (!resp.ok) throw new Error(getApiErrorMessage(json, "注册失败"));
      const registeredUser = {
        username: String(json?.username || username.trim()),
        role: typeof json?.role === "string" ? json.role : undefined,
        email: typeof json?.email === "string" ? json.email : undefined,
      };
      if (isDesktopRuntime) {
        const sessionToken = typeof json?.session_token === "string" ? json.session_token : "";
        setDesktopSessionToken(sessionToken);
        await saveDesktopAuthSession({ ...registeredUser, session_token: sessionToken }).catch(() => {});
      }
      onLogin(registeredUser);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
      if (widgetVerificationEnabled) resetCaptcha();
    }
  };

  const submitResetPassword = async () => {
    if (!authSecurityConfig.password_reset_enabled) {
      setError("当前未开启忘记密码邮箱重置");
      return;
    }
    if (!email.trim() || !resetToken.trim() || !password || !confirmPassword) {
      setError("请通过邮件中的重置链接进入，并完整填写新密码");
      return;
    }
    if (password !== confirmPassword) {
      setError("两次输入的密码不一致");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const resp = await fetch(`${getApiBase()}/api/auth/reset-password`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          email: email.trim(),
          token: resetToken.trim(),
          password,
        }),
      });
      const json = await resp.json().catch(() => ({}));
      if (!resp.ok) throw new Error(getApiErrorMessage(json, "密码重置失败"));
      setInfo("密码已重置，请重新登录");
      setMode("login");
      setPassword("");
      setConfirmPassword("");
      setResetToken("");
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
    }
  };

  const submit = () => {
    if (mode === "login") return void submitLogin();
    if (mode === "register") return void submitRegister();
    if (mode === "reset") return void submitResetPassword();
    return void openSliderCaptcha();
  };

  const switchMode = (nextMode: AuthMode) => {
    if (nextMode === "register" && !authSecurityConfig.registration_enabled) {
      setError("当前系统未开放注册");
      return;
    }
    if ((nextMode === "forgot" || nextMode === "reset") && !authSecurityConfig.password_reset_enabled) {
      setError("当前未开启忘记密码邮箱重置");
      return;
    }
    setMode(nextMode);
    setError("");
    setInfo("");
    setPassword("");
    setConfirmPassword("");
    setEmailCode("");
    if (nextMode !== "reset") setResetToken("");
    setSliderOpen(false);
    setSliderChallengeId("");
    setSliderValue(0);
    setSliderError("");
    setSliderPurpose("email");
    resetCaptcha();
  };

  const { title, subtitle, buttonText } = useMemo(() => {
    if (isRegister) {
      return { title: "创建账户", subtitle: "注册一个新账户开始使用", buttonText: "注册" };
    }
    if (isForgot) {
      return { title: "重置密码", subtitle: "输入邮箱地址，我们会发送密码重置链接", buttonText: "发送重置链接" };
    }
    if (isReset) {
      return { title: "设置新密码", subtitle: "请为你的账户设置新密码", buttonText: "重置密码" };
    }
    return { title: "欢迎回来", subtitle: "登录你的账户以继续使用", buttonText: "登录" };
  }, [isForgot, isRegister, isReset]);

  return (
    <div className="flex h-screen overflow-hidden bg-[#f5f0e8] text-[#2b2118]">
      <AuthBrandPanel />

      <section className="relative flex min-w-0 flex-1 flex-col overflow-y-auto bg-[#f5f0e8]">
        <header className="flex shrink-0 items-center gap-2.5 px-6 pt-5 lg:px-10">
          <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-[#c96f55] to-[#8f4a34] text-lg font-black text-white shadow-[0_4px_12px_rgba(126,66,47,0.3)]">B</span>
          <span className="text-lg font-bold tracking-wide text-[#2b2118]">Bloomery</span>
        </header>

        <div className="flex flex-1 items-center justify-center px-5 py-6">
        <div className="w-full max-w-[420px] rounded-2xl border border-[#e6d8ca] bg-[#f8f1e8]/95 p-7 shadow-[0_24px_70px_rgba(72,52,38,0.14)] sm:p-8">
          <div className="mb-6">
            <h2 className="text-[22px] font-bold tracking-tight text-[#2b2118]">{title}</h2>
            <p className="mt-1.5 text-sm text-[#6f6258]">{subtitle}</p>
            {(mode === "login" || isRegister) && (
              <div className="mt-5 flex gap-8 border-b border-[#eadfd2]">
                <span className="-mb-px border-b-2 border-[#cc785c] pb-2.5 text-[15px] font-semibold text-[#cc785c]">
                  {isRegister ? "注册账号" : "账号登录"}
                </span>
              </div>
            )}
            {(isForgot || isReset) && (
              <button
                type="button"
                onClick={() => switchMode("login")}
                className="mt-4 text-sm font-medium text-[#cc785c] underline-offset-4 hover:underline"
              >
                ← 返回登录
              </button>
            )}
          </div>

          <div className="space-y-3">
            {!isForgot && !isReset && (
              <AuthField
                icon={<User size={18} />}
                label="用户名"
                required
                value={username}
                onChange={setUsername}
                onEnter={submit}
                placeholder={isRegister ? "3-32 个字符" : "请输入用户名"}
              />
            )}

            {(isForgot || isReset || (isRegister && authSecurityConfig.email_verify_enabled)) && (
              <AuthField
                icon={<Mail size={18} />}
                label="邮箱"
                required
                value={email}
                onChange={setEmail}
                onEnter={submit}
                placeholder="请输入邮箱地址"
              />
            )}

            {isRegister && authSecurityConfig.email_verify_enabled && (
              <div className="grid grid-cols-[minmax(0,1fr)_96px] items-end gap-3">
                <AuthField
                  icon={<Mail size={18} />}
                  label="邮箱验证码"
                  required
                  value={emailCode}
                  onChange={setEmailCode}
                  onEnter={submit}
                  placeholder="6 位验证码"
                />
                <button
                  type="button"
                  onClick={() => void openSliderCaptcha()}
                  disabled={codeLoading || sliderLoading}
                  className="h-11 shrink-0 rounded-lg border border-[#cc785c]/40 bg-[#fffaf3]/80 px-3 text-sm font-medium text-[#cc785c] transition hover:bg-[#cc785c]/10 active:translate-y-[1px] disabled:opacity-50"
                >
                  {codeLoading || sliderLoading ? "发送中" : "验证码"}
                </button>
              </div>
            )}

            {isForgot && (
              <button
                type="button"
                onClick={() => void openSliderCaptcha()}
                disabled={codeLoading || sliderLoading}
                className="h-11 w-full rounded-lg bg-[#c96f55] text-[15px] font-semibold tracking-wide text-white shadow-[0_10px_24px_rgba(126,66,47,0.26)] transition hover:bg-[#bd6048] active:translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60"
              >
                {codeLoading || sliderLoading ? "发送中..." : "发送密码重置链接"}
              </button>
            )}

            {(mode === "login" || isRegister || isReset) && (
              <AuthField
                icon={<Lock size={18} />}
                label={isReset ? "新密码" : "密码"}
                required
                type={showPassword ? "text" : "password"}
                value={password}
                onChange={setPassword}
                onEnter={submit}
                placeholder="至少 6 位密码"
                right={
                  <button type="button" onClick={() => setShowPassword((value) => !value)} className="text-[#b0a69e] transition hover:text-[#cc785c]">
                    {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                }
              />
            )}

            {(isRegister || isReset) && (
              <AuthField
                icon={<Lock size={18} />}
                label="确认密码"
                required
                type={showConfirmPassword ? "text" : "password"}
                value={confirmPassword}
                onChange={setConfirmPassword}
                onEnter={submit}
                placeholder="再次输入密码"
                right={
                  <button type="button" onClick={() => setShowConfirmPassword((value) => !value)} className="text-[#b0a69e] transition hover:text-[#cc785c]">
                    {showConfirmPassword ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                }
              />
            )}

            {activeProvider === "turnstile" && !isReset && (
              <TurnstileWidget
                siteKey={captchaConfig.turnstile_site_key}
                resetSignal={turnstileResetSignal}
                onToken={setTurnstileToken}
                onError={setError}
              />
            )}
            {activeProvider === "geetest" && !isReset && (
              <GeetestWidget
                apiBase={`${getApiBase()}/api/auth`}
                resetSignal={geetestResetSignal}
                onToken={setGeetestToken}
                onError={setError}
              />
            )}
          </div>

          {info && (
            <div className="mt-3 flex items-start gap-2 rounded-lg border border-green-200/70 bg-green-50/70 px-3 py-2.5 text-xs text-green-700">
              <CheckCircle2 size={14} className="mt-0.5 shrink-0 text-green-500" />
              <span>{info}</span>
            </div>
          )}
          {error && (
            <div className="mt-3 rounded-lg border border-[#e8c6b9] bg-[#fff3ee]/80 px-3 py-2.5 text-xs leading-relaxed text-[#b96549]">{error}</div>
          )}

          {mode === "login" ? (
            <div className="mt-4 flex items-center justify-between text-sm">
              <label className="flex cursor-pointer items-center gap-2 text-[13px] text-[#6f6258]">
                <input
                  type="checkbox"
                  checked={rememberAccount}
                  onChange={(event) => setRememberAccount(event.target.checked)}
                  className="h-4 w-4 cursor-pointer rounded border-[#d8c9ba] accent-[#cc785c]"
                />
                记住账号
              </label>
              {authSecurityConfig.password_reset_enabled && (
                <button className="text-[13px] font-medium text-[#cc785c] underline-offset-4 hover:underline" onClick={() => switchMode("forgot")}>
                  忘记密码？
                </button>
              )}
            </div>
          ) : null}

          {(mode === "login" || isRegister) && (
            <label className="mt-4 flex cursor-pointer items-start gap-2 text-xs leading-relaxed text-[#6f6258]">
              <input
                type="checkbox"
                checked={agreeLegal}
                onChange={(event) => {
                  const checked = event.target.checked;
                  setAgreeLegal(checked);
                  try {
                    if (checked) {
                      localStorage.setItem("bloomery_legal_agreed", "1");
                    } else {
                      localStorage.removeItem("bloomery_legal_agreed");
                    }
                  } catch {
                    // ignore storage errors
                  }
                }}
                className="mt-0.5 h-4 w-4 shrink-0 cursor-pointer rounded border-[#d8c9ba] accent-[#cc785c]"
              />
              <span>
                我已阅读并同意
                <button
                  type="button"
                  onClick={(event) => {
                    event.preventDefault();
                    setLegalDoc("terms");
                  }}
                  className="font-semibold text-[#cc785c] underline-offset-4 hover:underline"
                >
                  《服务条款》
                </button>
                和
                <button
                  type="button"
                  onClick={(event) => {
                    event.preventDefault();
                    setLegalDoc("privacy");
                  }}
                  className="font-semibold text-[#cc785c] underline-offset-4 hover:underline"
                >
                  《隐私政策》
                </button>
              </span>
            </label>
          )}

          {!isForgot && (
            <button
              onClick={() => void submit()}
              disabled={loading}
              className="mt-5 h-11 w-full rounded-lg bg-[#c96f55] text-[15px] font-semibold tracking-wide text-white shadow-[0_10px_24px_rgba(126,66,47,0.26)] transition-all hover:bg-[#bd6048] focus:outline-none focus:ring-4 focus:ring-[#cc785c]/20 active:translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60"
            >
              {loading ? "处理中..." : buttonText}
            </button>
          )}

          <div className="mt-5 flex items-center justify-center gap-1.5 text-[13px]">
            {mode === "login" && authSecurityConfig.registration_enabled ? (
              <>
                <span className="text-[#6f6258]">没有账号？</span>
                <button className="font-semibold text-[#cc785c] underline-offset-4 hover:underline" onClick={() => switchMode("register")}>
                  立即注册
                </button>
              </>
            ) : (
              <>
                <span className="text-[#6f6258]">{mode === "login" ? " " : "已有账号？"}</span>
                <button className="font-semibold text-[#cc785c] underline-offset-4 hover:underline" onClick={() => switchMode("login")}>
                  立即登录
                </button>
              </>
            )}
          </div>
        </div>
        </div>

        <footer className="shrink-0 px-4 pb-4">
          <div className="flex flex-wrap items-center justify-center gap-x-3 gap-y-1 text-center text-[13px] leading-relaxed text-[#9c8e7f]">
            <span>© 2026 Bloomery · All Rights Reserved</span>
          </div>
        </footer>
      </section>

      {sliderOpen && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-[#2b2118]/50 px-4 backdrop-blur-sm">
          <div className="w-full max-w-[360px] rounded-2xl border border-[#e6d8ca] bg-[#f5f0e8] p-5 shadow-[0_20px_60px_rgba(43,33,24,0.25)]">
            <div className="flex items-start justify-between gap-4">
              <div>
                <div className="text-sm font-bold text-[#2b2118]">完成滑块验证</div>
                <div className="mt-1 text-xs leading-relaxed text-[#6f6258]">
                  拖动滑块到最右侧，通过后自动{isForgot ? "发送密码重置链接" : sliderPurpose === "login" ? "完成登录" : sliderPurpose === "register" ? "完成注册" : "发送邮箱验证码"}。
                </div>
              </div>
              <button
                type="button"
                onClick={() => {
                  setSliderOpen(false);
                  setSliderValue(0);
                  setSliderError("");
                }}
                className="flex h-8 w-8 items-center justify-center rounded-lg border border-[#e4d8cc] text-[#6f6258] transition hover:border-[#cc785c]/40 hover:text-[#cc785c]"
                title="关闭"
              >
                <X size={18} />
              </button>
            </div>

            <div className="mt-5">
              <div ref={sliderTrackRef} className="relative h-12 select-none overflow-hidden rounded-lg border border-[#e4d8cc] bg-[#efe7db]">
                <div
                  className="absolute inset-y-0 left-0 rounded-l-lg bg-[#cc785c]/15 transition-[width] duration-75"
                  style={{ width: `${Math.min(100, Math.max(0, sliderValue))}%` }}
                />
                <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-xs font-medium text-[#6f6258]">
                  {sliderValue >= 100 ? "验证中..." : "拖动滑块完成验证"}
                </div>
                <button
                  type="button"
                  disabled={sliderLoading}
                  onPointerDown={(event) => {
                    event.preventDefault();
                    event.currentTarget.setPointerCapture?.(event.pointerId);
                    setSliderStartAt(Date.now());
                    setSliderError("");
                    setSliderDragging(true);
                  }}
                  className="absolute top-1 flex h-10 w-10 touch-none items-center justify-center rounded-lg bg-[#fffaf3] text-[#cc785c] shadow-[0_2px_8px_rgba(72,52,38,0.18)] transition-[left] duration-75 disabled:cursor-not-allowed disabled:opacity-70"
                  style={{ left: `calc(${Math.min(100, Math.max(0, sliderValue))}% - ${sliderValue >= 98 ? 42 : 0}px)` }}
                  aria-label="拖动滑块完成验证"
                >
                  <ShieldCheck size={18} />
                </button>
              </div>
              {sliderError && <div className="mt-3 rounded-lg border border-[#e8c6b9] bg-[#fff3ee]/80 px-3 py-2 text-xs text-[#b96549]">{sliderError}</div>}
            </div>
          </div>
        </div>
      )}

      <LegalDocsModal doc={legalDoc} onClose={() => setLegalDoc(null)} />
    </div>
  );
}
