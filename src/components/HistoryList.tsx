import type { CheckRun } from "../types";
import { StatusPill } from "./StatusPill";
import { formatDateTime } from "../format";

export function HistoryList({ runs }: { runs: CheckRun[] }) {
  if (runs.length === 0) {
    return <p className="history-empty">Пока нет ни одной проверки.</p>;
  }
  return (
    <ul className="history-list">
      {runs.map((run) => (
        <li key={run.id} className="history-row">
          <StatusPill status={run.status} />
          <div className="history-row-body">
            <span className="history-time">{formatDateTime(run.started_at)}</span>
            <span className="history-message">{run.message}</span>
          </div>
        </li>
      ))}
    </ul>
  );
}
