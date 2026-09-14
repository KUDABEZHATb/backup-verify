import type { CheckStatus } from "../types";
import { STATUS_LABEL } from "../types";

export function StatusPill({ status }: { status: CheckStatus }) {
  return <span className={`status-pill status-${status}`}>{STATUS_LABEL[status]}</span>;
}
