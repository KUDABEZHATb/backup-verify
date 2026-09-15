export type Schedule = "daily" | "weekly" | "monthly";
export type CheckStatus = "ok" | "warning" | "error" | "pending";

export interface BackupTarget {
  id: string;
  name: string;
  path: string;
  schedule: Schedule;
  created_at: string;
  last_check_at: string | null;
  last_status: CheckStatus;
  last_message: string | null;
}

export interface CheckRun {
  id: number;
  backup_id: string;
  started_at: string;
  finished_at: string;
  status: CheckStatus;
  message: string;
  files_scanned: number;
  files_sampled: number;
  files_changed_unexpectedly: number;
  files_missing: number;
  newest_file_at: string | null;
}

export interface LicenseInfo {
  tier: string;
  license_ref: string;
}

export interface UpdateInfo {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  release_url: string;
}

export interface NewBackup {
  name: string;
  path: string;
  schedule: Schedule;
}

export const SCHEDULE_LABEL: Record<Schedule, string> = {
  daily: "Каждый день",
  weekly: "Каждую неделю",
  monthly: "Раз в месяц",
};

export const STATUS_LABEL: Record<CheckStatus, string> = {
  ok: "Всё в порядке",
  warning: "Стоит проверить",
  error: "Проблема",
  pending: "Ещё не проверялся",
};
