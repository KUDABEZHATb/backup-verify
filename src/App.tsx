import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { BackupTarget, LicenseInfo, UpdateInfo } from "./types";
import { api } from "./api";
import { BackupCard } from "./components/BackupCard";
import { AddBackupDialog } from "./components/AddBackupDialog";
import { LicenseDialog } from "./components/LicenseDialog";
import { EmptyState } from "./components/EmptyState";
import "./App.css";

export default function App() {
  const [backups, setBackups] = useState<BackupTarget[] | null>(null);
  const [license, setLicense] = useState<LicenseInfo | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);

  useEffect(() => {
    api.listBackups().then(setBackups);
    api.getLicenseStatus().then(setLicense);
  }, []);

  useEffect(() => {
    // The background scheduler runs independently of this window and emits
    // this event whenever it finishes a check — refresh so the dashboard
    // stays accurate even if nobody opened the window during the check.
    const unlisten = listen<string>("backup-checked", () => {
      api.listBackups().then(setBackups);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  function handleAdded(backup: BackupTarget) {
    setBackups((prev) => [...(prev ?? []), backup]);
  }

  function handleChanged(updated: BackupTarget) {
    setBackups((prev) => (prev ?? []).map((b) => (b.id === updated.id ? updated : b)));
  }

  function handleRemoved(id: string) {
    setBackups((prev) => (prev ?? []).filter((b) => b.id !== id));
  }

  function openLicenseFromLimit() {
    setAddOpen(false);
    setLicenseOpen(true);
  }

  async function handleCheckUpdate() {
    setCheckingUpdate(true);
    try {
      const info = await api.checkForUpdate();
      setUpdate(info);
    } catch {
      setUpdate(null);
    } finally {
      setCheckingUpdate(false);
    }
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>Проверка бэкапов</h1>
        <div className="app-header-actions">
          {update?.update_available ? (
            <button className="btn btn-ghost" onClick={() => openUrl(update.release_url)}>
              Доступна версия {update.latest_version} →
            </button>
          ) : (
            <button className="btn btn-ghost" onClick={handleCheckUpdate} disabled={checkingUpdate}>
              {checkingUpdate
                ? "Проверка…"
                : update
                  ? "Обновлений нет"
                  : "Проверить обновления"}
            </button>
          )}
          <button className="license-badge" onClick={() => setLicenseOpen(true)}>
            {license ? `PRO · ${license.license_ref}` : "Бесплатная версия"}
          </button>
          {backups && backups.length > 0 && (
            <button className="btn btn-primary" onClick={() => setAddOpen(true)}>
              Добавить бэкап
            </button>
          )}
        </div>
      </header>

      <main className="app-main">
        {backups === null && <p className="loading">Загрузка…</p>}
        {backups && backups.length === 0 && <EmptyState onAdd={() => setAddOpen(true)} />}
        {backups && backups.length > 0 && (
          <div className="backup-list">
            {backups.map((b) => (
              <BackupCard key={b.id} backup={b} onChanged={handleChanged} onRemoved={handleRemoved} />
            ))}
          </div>
        )}
      </main>

      {addOpen && (
        <AddBackupDialog
          onClose={() => setAddOpen(false)}
          onAdded={handleAdded}
          onNeedsLicense={openLicenseFromLimit}
        />
      )}
      {licenseOpen && (
        <LicenseDialog current={license} onClose={() => setLicenseOpen(false)} onActivated={setLicense} />
      )}
    </div>
  );
}
