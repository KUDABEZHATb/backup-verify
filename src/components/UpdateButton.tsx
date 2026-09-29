import { useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";

// One button that walks through the whole update: check → offer → download
// with progress → install. On Windows the updater plugin itself closes the
// running app when it launches the installer (installMode "passive" in
// tauri.conf.json), so there is no "restart" step to show here.
type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "none" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; percent: number | null }
  | { kind: "installing" }
  | { kind: "error"; message: string };

export function UpdateButton() {
  const [state, setState] = useState<State>({ kind: "idle" });

  async function checkForUpdate() {
    setState({ kind: "checking" });
    try {
      const update = await check();
      setState(update ? { kind: "available", update } : { kind: "none" });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }

  async function install(update: Update) {
    let total: number | null = null;
    let received = 0;
    setState({ kind: "downloading", percent: null });
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? null;
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          setState({ kind: "downloading", percent: total ? Math.min(100, Math.round((received / total) * 100)) : null });
        } else if (event.event === "Finished") {
          setState({ kind: "installing" });
        }
      });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }

  switch (state.kind) {
    case "available":
      return (
        <button className="btn btn-primary" onClick={() => install(state.update)}>
          Обновить до {state.update.version}
        </button>
      );
    case "downloading":
      return (
        <button className="btn btn-ghost" disabled>
          {state.percent === null ? "Скачивание…" : `Скачивание… ${state.percent}%`}
        </button>
      );
    case "installing":
      return (
        <button className="btn btn-ghost" disabled>
          Установка, программа закроется…
        </button>
      );
    default:
      return (
        <button
          className="btn btn-ghost"
          onClick={checkForUpdate}
          disabled={state.kind === "checking"}
          title={state.kind === "error" ? state.message : undefined}
        >
          {state.kind === "checking"
            ? "Проверка…"
            : state.kind === "error"
              ? "Не удалось проверить"
              : state.kind === "none"
                ? "Обновлений нет"
                : "Проверить обновления"}
        </button>
      );
  }
}
