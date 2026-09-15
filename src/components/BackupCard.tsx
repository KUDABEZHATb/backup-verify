import { useState } from "react";
import type { BackupTarget, CheckRun } from "../types";
import { SCHEDULE_LABEL } from "../types";
import { StatusPill } from "./StatusPill";
import { HistoryList } from "./HistoryList";
import { formatDateTime, nextCheckEstimate } from "../format";
import { api } from "../api";

interface Props {
  backup: BackupTarget;
  onChanged: (updated: BackupTarget) => void;
  onRemoved: (id: string) => void;
}

export function BackupCard({ backup, onChanged, onRemoved }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [history, setHistory] = useState<CheckRun[] | null>(null);
  const [checking, setChecking] = useState(false);

  async function toggleHistory() {
    const next = !expanded;
    setExpanded(next);
    if (next && history === null) {
      setHistory(await api.getHistory(backup.id));
    }
  }

  async function checkNow() {
    setChecking(true);
    try {
      const updated = await api.runCheckNow(backup.id);
      onChanged(updated);
      if (expanded) setHistory(await api.getHistory(backup.id));
    } finally {
      setChecking(false);
    }
  }

  async function remove() {
    if (!confirm(`Убрать «${backup.name}» из отслеживаемых бэкапов?`)) return;
    await api.removeBackup(backup.id);
    onRemoved(backup.id);
  }

  return (
    <div className={`backup-card status-border-${backup.last_status}${backup.locked ? " backup-card-locked" : ""}`}>
      <div className="backup-card-head">
        <div className="backup-card-titles">
          <h3>{backup.name}</h3>
          <span className="backup-path" title={backup.path}>
            {backup.path}
          </span>
        </div>
        {backup.locked ? <span className="locked-pill">Заблокировано лицензией</span> : <StatusPill status={backup.last_status} />}
      </div>

      {backup.locked && (
        <p className="backup-message">
          Этот бэкап превышает лимит текущей лицензии и не проверяется. Активируйте лицензию с бóльшим лимитом или
          уберите лишние бэкапы.
        </p>
      )}
      {!backup.locked && backup.last_message && <p className="backup-message">{backup.last_message}</p>}

      <div className="backup-meta">
        <div>
          <span className="meta-label">Проверено</span>
          <span className="meta-value">{formatDateTime(backup.last_check_at)}</span>
        </div>
        <div>
          <span className="meta-label">Следующая проверка</span>
          <span className="meta-value">{nextCheckEstimate(backup.last_check_at, backup.schedule)}</span>
        </div>
        <div>
          <span className="meta-label">Период</span>
          <span className="meta-value">{SCHEDULE_LABEL[backup.schedule]}</span>
        </div>
      </div>

      <div className="backup-card-actions">
        <button onClick={checkNow} disabled={checking || backup.locked} className="btn btn-primary">
          {checking ? "Проверяю…" : "Проверить сейчас"}
        </button>
        <button onClick={toggleHistory} className="btn btn-ghost">
          {expanded ? "Скрыть историю" : "История проверок"}
        </button>
        <button onClick={remove} className="btn btn-ghost btn-danger">
          Убрать
        </button>
      </div>

      {expanded && (
        <div className="backup-history">{history ? <HistoryList runs={history} /> : <p>Загрузка…</p>}</div>
      )}
    </div>
  );
}
