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

// Local commands (list_backups, get_license_status) only ever touch the
// on-disk SQLite file — if they haven't answered within this long, the
// backend is stuck (e.g. a corrupted WAL file), not just slow. Surfacing
// that as an error beats leaving "Загрузка…" on screen forever with no way
// for the user to tell a hang from a big backup list.
const LOCAL_COMMAND_TIMEOUT_MS = 8000;

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("Программа не отвечает — похоже, зависла база данных.")),
      ms,
    );
    promise.then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      },
    );
  });
}

export default function App() {
  const [backups, setBackups] = useState<BackupTarget[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [license, setLicense] = useState<LicenseInfo | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);

  function loadBackups() {
    setLoadError(null);
    withTimeout(api.listBackups(), LOCAL_COMMAND_TIMEOUT_MS)
      .then(setBackups)
      .catch((e) => setLoadError(String(e?.message ?? e)));
  }

  useEffect(() => {
    loadBackups();
    withTimeout(api.getLicenseStatus(), LOCAL_COMMAND_TIMEOUT_MS).then(setLicense).catch(() => {});
  }, []);

  useEffect(() => {
    // The background scheduler runs independently of this window and emits
    // this event whenever it finishes a check — refresh so the dashboard
    // stays accurate even if nobody opened the window during the check.
    const unlisten = listen<string>("backup-checked", loadBackups);
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
    setUpdateError(null);
    try {
      const info = await api.checkForUpdate();
      setUpdate(info);
    } catch (e) {
      setUpdate(null);
      setUpdateError(String(e));
    } finally {
      setCheckingUpdate(false);
    }
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>Проверка бэкапов</h1>
        <div className="app-header-actions">
          <button className="license-badge" onClick={() => setLicenseOpen(true)}>
            {license ? `PRO · ${license.license_ref}` : "Бесплатная версия"}
          </button>
          {update?.update_available ? (
            <button className="btn btn-ghost" onClick={() => openUrl(update.release_url)}>
              Доступна версия {update.latest_version} →
            </button>
          ) : (
            <button
              className="btn btn-ghost"
              onClick={handleCheckUpdate}
              disabled={checkingUpdate}
              title={updateError ?? undefined}
            >
              {checkingUpdate
                ? "Проверка…"
                : updateError
                  ? "Не удалось проверить"
                  : update
                    ? "Обновлений нет"
                    : "Проверить обновления"}
            </button>
          )}
          {backups && backups.length > 0 && (
            <button className="btn btn-primary" onClick={() => setAddOpen(true)}>
              Добавить бэкап
            </button>
          )}
        </div>
      </header>

      <main className="app-main">
        {backups === null && !loadError && <p className="loading">Загрузка…</p>}
        {loadError && (
          <p className="form-error">
            {loadError}{" "}
            <button className="link-button" onClick={loadBackups}>
              Повторить
            </button>
          </p>
        )}
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
