import { useCallback, useEffect, useState } from "react";
import type { AuthUserInfo } from "../LoginPage";
import { getCloudApiBaseSetting } from "../desktop/services/settings";
import { clearDesktopAuthSession, getDesktopAuthSession, isTauriRuntime } from "../desktop/services/tauri";
import { setDesktopSessionToken, setRuntimeApiBase } from "../services/api";
import { getAuthMe, logout } from "../services/auth";

export function useAuthSession() {
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [authChecked, setAuthChecked] = useState(false);
  const [authUser, setAuthUser] = useState<AuthUserInfo | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function checkAuth() {
      try {
        if (isTauriRuntime()) {
          const [session, apiBase] = await Promise.all([
            getDesktopAuthSession().catch(() => null),
            getCloudApiBaseSetting().catch(() => ""),
          ]);
          if (apiBase) setRuntimeApiBase(apiBase);
          if (session?.token) setDesktopSessionToken(session.token);
        }
        const json = await getAuthMe();
        if (!cancelled) {
          const authenticated = Boolean(json?.authenticated);
          setIsAuthenticated(authenticated);
          setAuthUser(authenticated ? {
            username: String(json?.username || "").trim(),
            role: typeof json?.role === "string" ? json.role : undefined,
            email: typeof json?.email === "string" ? json.email : undefined,
          } : null);
        }
      } catch {
        if (!cancelled) {
          setIsAuthenticated(false);
          setAuthUser(null);
        }
      } finally {
        if (!cancelled) setAuthChecked(true);
      }
    }
    checkAuth();
    return () => {
      cancelled = true;
    };
  }, []);

  const markLoggedIn = useCallback((user: AuthUserInfo | null | undefined) => {
    setIsAuthenticated(true);
    if (user?.username?.trim()) {
      setAuthUser(user);
    }
  }, []);

  const logoutAuth = useCallback(async () => {
    try {
      await logout();
    } catch {
      // Logout should still clear local UI state even if the network is unavailable.
    }
    await clearDesktopAuthSession().catch(() => {});
    setIsAuthenticated(false);
    setAuthUser(null);
  }, []);

  return {
    isAuthenticated,
    authChecked,
    authUser,
    markLoggedIn,
    logoutAuth,
  };
}
