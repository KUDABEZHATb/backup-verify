import { useState } from "react";
import { api } from "../api";
import type { LicenseInfo } from "../types";

interface Props {
  onClose: () => void;
  onActivated: (info: LicenseInfo) => void;
}

export function LicenseDialog({ onClose, onActivated }: Props) {
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      const info = await api.activateLicense(key.trim());
      onActivated(info);
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
        <h2>Лицензионный ключ</h2>
        <p className="modal-hint">
          Бесплатная версия отслеживает один бэкап. Ключ снимает это ограничение — вставьте его из письма
          после покупки.
        </p>
        <label className="field">
          <span>Ключ</span>
          <input
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="BVPR-XXXXX-XXXXX-XXXXX-XXXXX-XXXXX-XXX"
            autoFocus
          />
        </label>
        {error && <p className="form-error">{error}</p>}
        <div className="modal-actions">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Отмена
          </button>
          <button type="submit" className="btn btn-primary" disabled={saving || !key.trim()}>
            {saving ? "Проверяю…" : "Активировать"}
          </button>
        </div>
      </form>
    </div>
  );
}
