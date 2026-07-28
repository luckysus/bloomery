import { AppSettingsProvider } from "../context/AppSettingsContext";
import { AuthProvider } from "../context/AuthContext";
import RagAppPage from "../pages/RagAppPage";
import DesktopExtrasDock from "./DesktopExtrasDock";
import DesktopRuntimeBridge from "./DesktopRuntimeBridge";

export default function DesktopApp() {
  return (
    <AppSettingsProvider>
      <AuthProvider>
        <DesktopRuntimeBridge />
        <RagAppPage />
        <DesktopExtrasDock />
      </AuthProvider>
    </AppSettingsProvider>
  );
}
