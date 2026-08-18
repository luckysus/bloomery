export function formatDateTimeText(value?: string | null): string {
  if (!value) return "刚刚";
  const normalized = value.replace("T", " ");
  const withoutFraction = normalized.replace(/\.\d+/, "");
  return withoutFraction.slice(0, 19);
}
