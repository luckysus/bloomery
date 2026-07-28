import { AppSettingsProvider } from "./context/AppSettingsContext";
import { AuthProvider } from "./context/AuthContext";
import RagAppPage from "./pages/RagAppPage";

export default function App() {
  return (
    <AppSettingsProvider>
      <AuthProvider>
        <RagAppPage />
      </AuthProvider>
    </AppSettingsProvider>
  );
}
