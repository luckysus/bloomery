import RagEntryGate from "../components/layout/RagEntryGate";
import RagAppShell from "../components/layout/RagAppShell";
import { useRagAppController } from "../hooks/useRagAppController";

export default function RagAppPage() {
  const { shouldShowEntryGate, entryGateProps, shellProps } = useRagAppController();

  if (shouldShowEntryGate) {
    return <RagEntryGate {...entryGateProps} />;
  }

  return <RagAppShell {...shellProps} />;
}
