import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { Schedule } from "../types";
import { SCHEDULE_LABEL } from "../types";
import { api } from "../api";
import type { BackupTarget } from "../types";

interface Props {
  onClose: () => void;
  onAdded: (backup: BackupTarget) => void;
  onNeedsLicense: () => void;
}

export function AddBackupDialog({ onClose, onAdded, onNeedsLicense }: Props) {
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [schedule, setSchedule] = useState<Schedule>("daily");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function pickFolder() {
    const selected = await open({ directory: true, multiple: false, title: "Выберите папку с бэкапом" });
    if (typeof selected === "string") {
      setPath(selected);
      if (!name) {
        const parts = selected.split(/[\\/]/).filter(Boolean);
        setName(parts[parts.length - 1] ?? selected);
      }
    }
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim() || !path.trim()) {
      setError("Укажите название и путь к папке");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const created = await api.addBackup({ name: name.trim(), path: path.trim(), schedule });
      onAdded(created);
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <form className="modal" onClick={(e) => e.stopPropagation()} onSubmit={submit}>
        <h2>Добавить бэкап</h2>

        <label className="field">
          <span>Путь к папке с бэкапом</span>
          <div className="field-row">
            <input
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="например, D:\\Backups\\Documents"
            />
            <button type="button" className="btn btn-ghost" onClick={pickFolder}>
              Обзор…
            </button>
          </div>
        </label>

        <label className="field">
          <span>Название</span>
          <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Например, «Документы»" />
        </label>

        <label className="field">
          <span>Как часто проверять</span>
          <select value={schedule} onChange={(e) => setSchedule(e.target.value as Schedule)}>
            {(Object.keys(SCHEDULE_LABEL) as Schedule[]).map((s) => (
              <option key={s} value={s}>
                {SCHEDULE_LABEL[s]}
              </option>
            ))}
          </select>
        </label>

        {error && (
          <p className="form-error">
            {error}
            {error.includes("лицензионный ключ") && (
              <>
                {" "}
                <button type="button" className="link-button" onClick={onNeedsLicense}>
                  Ввести ключ
                </button>
              </>
            )}
          </p>
        )}

        <div className="modal-actions">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Отмена
          </button>
          <button type="submit" className="btn btn-primary" disabled={saving}>
            {saving ? "Добавляю…" : "Добавить"}
          </button>
        </div>
      </form>
    </div>
  );
}
