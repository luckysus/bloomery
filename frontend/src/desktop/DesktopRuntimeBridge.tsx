import { useEffect } from "react";
import { useAuth } from "../context/AuthContext";
import { setDesktopSessionToken, setRuntimeApiBase } from "../services/api";
import { getCloudApiBaseSetting } from "./services/settings";
import { getDesktopAuthSession, initDesktopDb, isTauriRuntime, saveDesktopAuthSession } from "./services/tauri";
import { setupWindowStatePersistence } from "./services/windowState";

export default function DesktopRuntimeBridge() {
  const { authChecked, isAuthenticated, authUser } = useAuth();

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let dispose: (() => void) | undefined;

    void setupWindowStatePersistence().then((cleanup) => {
      dispose = cleanup;
    });
    void (async () => {
      const session = await getDesktopAuthSession().catch(() => null);
      if (session?.token) setDesktopSessionToken(session.token);
      await initDesktopDb().catch(() => {});
      await getCloudApiBaseSetting().then(setRuntimeApiBase).catch(() => {});
    })();

    return () => {
      dispose?.();
    };
  }, []);

  useEffect(() => {
    if (!isTauriRuntime() || !authChecked || !isAuthenticated) return;
    if (authUser?.username && authUser.session_token) void saveDesktopAuthSession(authUser).catch(() => {});
    void initDesktopDb().catch(() => {});
  }, [authChecked, authUser, isAuthenticated]);

  return null;
}
