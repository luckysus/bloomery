import { useCallback, useRef, useState } from "react";
import { getLabServiceStatus, reconnectLabService, type LabServiceStatusInfo } from "../services/labService";

export type LabCardTone = "slate" | "emerald" | "amber" | "red";

export function useLabServiceStatus() {
  const [labServiceStatus, setLabServiceStatus] = useState<LabServiceStatusInfo | null>(null);
  const [labServiceLoading, setLabServiceLoading] = useState(false);
  const labServiceCheckedRef = useRef(false);

  const refreshLabServiceStatus = useCallback(async (options?: { force?: boolean; reconnect?: boolean; quiet?: boolean }) => {
    setLabServiceLoading(true);
    try {
      const json = options?.reconnect
        ? await reconnectLabService()
        : await getLabServiceStatus({ force: options?.force });
      setLabServiceStatus(json);
      labServiceCheckedRef.current = true;
    } catch (err: any) {
      setLabServiceStatus({
        available: false,
        message: err.message || "实验室服务不可用",
        cached: false,
        checks: [],
      });
    } finally {
      setLabServiceLoading(false);
    }
  }, []);

  const handleLabServiceAction = useCallback(() => {
    void refreshLabServiceStatus({ force: true, reconnect: !labServiceStatus?.available });
  }, [labServiceStatus?.available, refreshLabServiceStatus]);

  const labRetrievalAvailable = labServiceStatus?.retrieval_available ?? labServiceStatus?.available ?? false;
  const labOptimizationAvailable = labServiceStatus?.optimization_available ?? labServiceStatus?.available ?? false;
  const labCardTone: LabCardTone = !labServiceStatus
    ? "slate"
    : labRetrievalAvailable
      ? labOptimizationAvailable ? "emerald" : "amber"
      : "red";

  return {
    labServiceStatus,
    labServiceLoading,
    labServiceCheckedRef,
    labRetrievalAvailable,
    labOptimizationAvailable,
    labCardTone,
    refreshLabServiceStatus,
    handleLabServiceAction,
  };
}
