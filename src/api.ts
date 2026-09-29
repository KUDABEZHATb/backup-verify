import { invoke } from "@tauri-apps/api/core";
import type { BackupTarget, CheckRun, LicenseInfo, NewBackup } from "./types";

export const api = {
  listBackups: () => invoke<BackupTarget[]>("list_backups"),
  addBackup: (backup: NewBackup) => invoke<BackupTarget>("add_backup", { new: backup }),
  removeBackup: (id: string) => invoke<void>("remove_backup", { id }),
  getHistory: (id: string, limit = 20) => invoke<CheckRun[]>("get_history", { id, limit }),
  runCheckNow: (id: string) => invoke<BackupTarget>("run_check_now", { id }),
  activateLicense: (key: string) => invoke<LicenseInfo>("activate_license", { key }),
  getLicenseStatus: () => invoke<LicenseInfo | null>("get_license_status"),
};
