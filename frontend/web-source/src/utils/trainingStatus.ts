export type TrainingRunStatus = "idle" | "running" | "completed" | "failed" | "cancelled";

export function normalizeTrainingRunStatus(status: unknown): TrainingRunStatus {
  const value = String(status || "").toLowerCase();
  if (value === "completed") return "completed";
  if (value === "cancelled" || value === "canceled") return "cancelled";
  if (value === "failed" || value === "cancel_failed" || value === "error") return "failed";
  if (value === "starting" || value === "pending" || value === "training" || value === "running") return "running";
  return "idle";
}

export function isTrainingTerminalStatus(status: unknown) {
  const normalized = normalizeTrainingRunStatus(status);
  return normalized === "completed" || normalized === "failed" || normalized === "cancelled";
}
