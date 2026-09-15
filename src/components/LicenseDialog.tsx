import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import type { LicenseInfo } from "../types";
import { PURCHASE_URL, PRICE_LABEL } from "../config";

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
          <span className="meta-label">Ключ</span>
          <span className="meta-value">{current.license_ref}</span>
        </div>
        <div>
          <span className="meta-label">Лимит бэкапов</span>
          <span className="meta-value">{current.max_backups}</span>
        </div>
        <div>
          <span className="meta-label">Действует до</span>
          <span className="meta-value">{new Date(current.expires_at).toLocaleDateString()}</span>
        </div>
      </div>
      <p className="modal-hint">
        Приложение само продлевает лицензию в фоне, пока есть подключение к интернету хотя бы изредка.
      </p>
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
  // Selling is the default — someone opening this dialog almost never has a
  // key yet. Entering one is a secondary path, one tap away, not a second
  // competing call to action fighting the purchase button for attention.
  const [showKeyInput, setShowKeyInput] = useState(false);

  return showKeyInput ? (
    <KeyEntryPanel onClose={onClose} onActivated={onActivated} onBack={() => setShowKeyInput(false)} />
  ) : (
    <UpsellPanel onClose={onClose} onHaveKey={() => setShowKeyInput(true)} />
  );
}

function UpsellPanel({ onClose, onHaveKey }: { onClose: () => void; onHaveKey: () => void }) {
  async function buy() {
    await openUrl(PURCHASE_URL);
  }

  return (
    <div className="modal upsell" onClick={(e) => e.stopPropagation()}>
      <h2>Открыть PRO</h2>
      <p className="modal-hint">
        Бесплатная версия отслеживает один бэкап — этого достаточно, чтобы понять, нужна ли вам программа.
        PRO снимает ограничение на количество путей.
      </p>
      <ul className="upsell-features">
        <li>Неограниченное число отслеживаемых бэкапов</li>
        <li>Один платёж, без подписки и повторных списаний</li>
        <li>Активация онлайн один раз, дальше приложение само продлевает лицензию в фоне</li>
      </ul>
      <div className="upsell-price-row">
        <span className="upsell-price">{PRICE_LABEL}</span>
        <span className="upsell-price-note">разово</span>
      </div>
      <button type="button" className="btn btn-primary btn-buy" onClick={buy}>
        Купить ключ
      </button>
      <p className="upsell-hint">Ключ придёт на почту сразу после оплаты — вставите его на следующем шаге.</p>
      <div className="upsell-divider">
        <span>или</span>
      </div>
      <button type="button" className="link-button upsell-have-key" onClick={onHaveKey}>
        У меня уже есть ключ
      </button>
      <div className="modal-actions modal-actions-end">
        <button type="button" className="btn btn-ghost" onClick={onClose}>
          Закрыть
        </button>
      </div>
    </div>
  );
}

function KeyEntryPanel({
  onClose,
  onActivated,
  onBack,
}: {
  onClose: () => void;
  onActivated: (info: LicenseInfo) => void;
  onBack: () => void;
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
      <label className="field">
        <span>Ключ</span>
        <input
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="BVPR-XXXXX-XXXXX-XXXXX-XXXXX"
          autoFocus
        />
      </label>
      {error && <p className="form-error">{error}</p>}
      <button type="button" className="link-button upsell-have-key" onClick={onBack}>
        Ещё нет ключа? Купить
      </button>
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
