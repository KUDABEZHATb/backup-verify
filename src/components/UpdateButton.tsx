import { useEffect, useState } from "react";
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

// The app lives in the tray and is usually started with Windows, so a check at
// startup alone would only run once per reboot. The webview keeps running while
// the window is hidden, hence a slow repeat: one tiny manifest request per day.
const AUTO_CHECK_DELAY_MS = 15_000;
const AUTO_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

export function UpdateButton() {
  const [state, setState] = useState<State>({ kind: "idle" });

  // Silent background check: only an available update changes the UI, "no
  // update" and network errors stay invisible (the manual button reports them).
  useEffect(() => {
    async function autoCheck() {
      try {
        const update = await check();
        if (update) {
          setState((s) => (s.kind === "idle" || s.kind === "none" || s.kind === "error" ? { kind: "available", update } : s));
        }
      } catch {
        // offline or server down — try again at the next tick
      }
    }
    const first = setTimeout(autoCheck, AUTO_CHECK_DELAY_MS);
    const repeat = setInterval(autoCheck, AUTO_CHECK_INTERVAL_MS);
    return () => {
      clearTimeout(first);
      clearInterval(repeat);
    };
  }, []);

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
