use crate::db::Db;
use crate::license::{self, LicenseInfo, FREE_TIER_BACKUP_LIMIT};
use crate::models::{BackupTarget, CheckRun, NewBackup};
use crate::{activation, db, scheduler};
use std::collections::HashSet;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn add_backup(db: State<Db>, new: NewBackup) -> Result<BackupTarget, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let max_backups = db::current_max_backups(&conn).map_err(|e| e.to_string())?;
    let count = db::backup_count(&conn).map_err(|e| e.to_string())?;
    if count >= max_backups {
        return Err(if max_backups <= FREE_TIER_BACKUP_LIMIT {
            format!(
                "Бесплатная версия отслеживает {FREE_TIER_BACKUP_LIMIT} бэкап. Введите лицензионный ключ, чтобы добавить ещё."
            )
        } else {
            format!("Текущая лицензия разрешает отслеживать до {max_backups} бэкапов.")
        });
    }
    db::insert_backup(&conn, &new).map_err(|e| e.to_string())
}

/// Activates a key against the license server and stores the signed
/// receipt it returns. The receipt is re-verified here before it's ever
/// written to disk — nothing the server sends is trusted without a
/// passing signature check first (see `license::verify_receipt`).
#[tauri::command]
pub async fn activate_license(db: State<'_, Db>, key: String) -> Result<LicenseInfo, String> {
    let machine_id = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        db::machine_id(&conn).map_err(|e| e.to_string())?
    };
    let stored = activation::activate(&key, &machine_id).await?;
    let receipt = license::verify_receipt(&stored.receipt_b64, &stored.signature_b64)?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::save_receipt(&conn, &stored.receipt_b64, &stored.signature_b64).map_err(|e| e.to_string())?;
    Ok(LicenseInfo {
        tier: receipt.tier,
        license_ref: receipt.key,
        max_backups: receipt.max_backups,
        expires_at: receipt.expires_at,
    })
}

#[tauri::command]
pub fn get_license_status(db: State<Db>) -> Result<Option<LicenseInfo>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::get_license(&conn).map_err(|e| e.to_string())
}

/// Marks each backup `locked` if it falls outside the install's current
/// quota (see `db::entitled_backup_ids`) — re-derived from the live count
/// on every call, so a backup inserted some other way than `add_backup`
/// never silently counts as an active, checked backup.
#[tauri::command]
pub fn list_backups(db: State<Db>) -> Result<Vec<BackupTarget>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let max_backups = db::current_max_backups(&conn).map_err(|e| e.to_string())?;
    let entitled: HashSet<String> =
        db::entitled_backup_ids(&conn, max_backups).map_err(|e| e.to_string())?.into_iter().collect();
    let mut backups = db::list_backups(&conn).map_err(|e| e.to_string())?;
    for backup in &mut backups {
        backup.locked = !entitled.contains(&backup.id);
    }
    Ok(backups)
}

#[tauri::command]
pub fn remove_backup(db: State<Db>, id: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::delete_backup(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_history(db: State<Db>, id: String, limit: i64) -> Result<Vec<CheckRun>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::history(&conn, &id, limit).map_err(|e| e.to_string())
}

/// Triggers an out-of-schedule check and waits for it to finish, so the UI
/// can show the fresh status right after the user clicks "Проверить сейчас".
/// Runs off the async executor via spawn_blocking since the check itself
/// does synchronous file I/O and hashing.
#[tauri::command]
pub async fn run_check_now(app: AppHandle, id: String) -> Result<BackupTarget, String> {
    {
        let state = app.state::<Db>();
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let max_backups = db::current_max_backups(&conn).map_err(|e| e.to_string())?;
        let entitled = db::entitled_backup_ids(&conn, max_backups).map_err(|e| e.to_string())?;
        if !entitled.contains(&id) {
            return Err(
                "Эта проверка недоступна — превышен лимит бесплатной версии или срок действия лицензии истёк."
                    .to_string(),
            );
        }
    }

    let id_for_check = id.clone();
    let app_for_check = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        scheduler::run_single_check(&app_for_check, &id_for_check);
    })
    .await
    .map_err(|e| e.to_string())?;

    let state = app.state::<Db>();
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    db::get_backup(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "backup not found".to_string())
}
