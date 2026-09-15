use crate::db::Db;
use crate::models::BackupTarget;
use crate::{checker, db, notify};
use chrono::{DateTime, Utc};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// How often the background loop wakes up to see if anything is due. This is
/// independent of the per-backup schedule (daily/weekly/monthly) — it's just
/// the polling granularity, deliberately coarse since backup checks are not
/// time-critical.
const POLL_INTERVAL: Duration = Duration::from_secs(15 * 60);

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Small delay so the window/tray finish initializing before the
        // first background pass.
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            maybe_revalidate_license(&app).await;
            run_due_checks(app.clone()).await;
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

/// Refreshes the license receipt with the server once it's getting close to
/// expiry (see `license::needs_revalidation`), so a paying customer's app
/// keeps working without them ever noticing — as long as they're online at
/// some point in that window. A failed attempt (offline, server down) is
/// silently skipped; the still-unexpired receipt keeps working until its
/// own `expires_at` actually passes, which is the real grace period.
async fn maybe_revalidate_license(app: &AppHandle) {
    let state = app.state::<Db>();
    let (key, machine_id) = {
        let conn = state.0.lock().unwrap();
        let Ok(Some(pair)) = db::receipt_for_revalidation(&conn) else { return };
        let due = match db::get_license(&conn) {
            Ok(Some(info)) => crate::license::needs_revalidation(&info.expires_at),
            Ok(None) => true, // no longer verifiable/expired — always worth a try
            Err(_) => return,
        };
        if !due {
            return;
        }
        pair
    };
    if let Ok(stored) = crate::activation::revalidate(&key, &machine_id).await {
        let conn = state.0.lock().unwrap();
        let _ = db::save_receipt(&conn, &stored.receipt_b64, &stored.signature_b64);
    }
}

async fn run_due_checks(app: AppHandle) {
    let due_ids: Vec<String> = {
        let state = app.state::<Db>();
        let conn = state.0.lock().unwrap();
        let entitled = entitled_ids(&conn);
        match db::list_backups(&conn) {
            Ok(list) => list
                .into_iter()
                .filter(is_due)
                .map(|b| b.id)
                .filter(|id| entitled.contains(id))
                .collect(),
            Err(_) => Vec::new(),
        }
    };
    for id in due_ids {
        let app2 = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || run_single_check(&app2, &id)).await;
    }
}

/// Ids of the backups this install is currently entitled to have checked —
/// re-derived every tick rather than cached, so a row added directly to the
/// database (bypassing the app's own add-backup limit check) never gets
/// auto-checked just because it exists.
fn entitled_ids(conn: &rusqlite::Connection) -> std::collections::HashSet<String> {
    let max_backups = db::current_max_backups(conn).unwrap_or(crate::license::FREE_TIER_BACKUP_LIMIT);
    db::entitled_backup_ids(conn, max_backups).unwrap_or_default().into_iter().collect()
}

fn is_due(b: &BackupTarget) -> bool {
    match &b.last_check_at {
        None => true,
        Some(ts) => match DateTime::parse_from_rfc3339(ts) {
            Ok(dt) => {
                let elapsed = Utc::now().signed_duration_since(dt.with_timezone(&Utc)).num_seconds();
                elapsed >= b.schedule.interval_seconds()
            }
            Err(_) => true,
        },
    }
}

/// Runs one check synchronously (blocking — file walking and hashing) for a
/// single backup target. Shared by the scheduler tick and the "Проверить
/// сейчас" command; callers are expected to run this via spawn_blocking so
/// it never stalls the async runtime.
pub fn run_single_check(app: &AppHandle, backup_id: &str) {
    let state = app.state::<Db>();
    let (backup, run) = {
        let conn = state.0.lock().unwrap();
        let Ok(Some(backup)) = db::get_backup(&conn, backup_id) else {
            return;
        };
        let run = checker::check_folder_backup(&conn, &backup);
        let _ = db::insert_check(&conn, &run);
        let _ = db::update_backup_result(&conn, &backup.id, run.status, &run.message, &run.finished_at);
        (backup, run)
    };
    notify::notify_if_problem(app, &backup, run.status, &run.message);
    let _ = app.emit("backup-checked", &backup.id);
}
