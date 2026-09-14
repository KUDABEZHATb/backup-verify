import { useState } from "react";
import { api } from "../api";
import type { LicenseInfo } from "../types";

interface Props {
  current: LicenseInfo | null;
  onClose: () => void;
  onActivated: (info: LicenseInfo) => void;
}

export function LicenseDialog({ current, onClose, onActivated }: Props) {
  // With no active license there's nothing to show, so go straight to the
  // activation form; with one, open on the info view — re-entering the key
  // is the rarer path (e.g. activating a replacement key on a new machine).
  const [mode, setMode] = useState<"view" | "edit">(current ? "view" : "edit");

  return (
    <div className="modal-backdrop" onClick={onClose}>
      {mode === "view" && current ? (
        <LicenseInfoView current={current} onClose={onClose} onEditRequested={() => setMode("edit")} />
      ) : (
        <LicenseForm onClose={onClose} onActivated={onActivated} />
      )}
    </div>
  );
}

function LicenseInfoView({
  current,
  onClose,
  onEditRequested,
}: {
  current: LicenseInfo;
  onClose: () => void;
  onEditRequested: () => void;
}) {
  return (
    <div className="modal" onClick={(e) => e.stopPropagation()}>
      <h2>Лицензия активна</h2>
      <div className="license-info">
        <div>
          <span className="meta-label">Тариф</span>
          <span className="meta-value">PRO</span>
        </div>
        <div>
          <span className="meta-label">Референс ключа</span>
          <span className="meta-value">{current.license_ref}</span>
        </div>
      </div>
      <p className="modal-hint">Ограничение на количество бэкапов снято.</p>
      <div className="modal-actions">
        <button type="button" className="btn btn-ghost" onClick={onEditRequested}>
          Ввести другой ключ
        </button>
        <button type="button" className="btn btn-primary" onClick={onClose}>
          Готово
        </button>
      </div>
    </div>
  );
}

function LicenseForm({
  onClose,
  onActivated,
}: {
  onClose: () => void;
  onActivated: (info: LicenseInfo) => void;
}) {
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
  );
}
