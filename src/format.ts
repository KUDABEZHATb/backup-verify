import type { Schedule } from "./types";

const SCHEDULE_DAYS: Record<Schedule, number> = { daily: 1, weekly: 7, monthly: 30 };

export function formatDateTime(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return d.toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function nextCheckEstimate(lastCheckAt: string | null, schedule: Schedule): string {
  if (!lastCheckAt) return "скоро — при первом запуске";
  const next = new Date(lastCheckAt);
  next.setDate(next.getDate() + SCHEDULE_DAYS[schedule]);
  return formatDateTime(next.toISOString());
}
