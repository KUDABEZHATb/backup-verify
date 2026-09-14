use crate::db::Db;
use crate::license::{self, LicenseInfo, FREE_TIER_BACKUP_LIMIT};
use crate::models::{BackupTarget, CheckRun, NewBackup};
use crate::{db, scheduler};
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn add_backup(db: State<Db>, new: NewBackup) -> Result<BackupTarget, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let licensed = db::get_license(&conn).map_err(|e| e.to_string())?.is_some();
    if !licensed {
        let count = db::backup_count(&conn).map_err(|e| e.to_string())?;
        if count as usize >= FREE_TIER_BACKUP_LIMIT {
            return Err(format!(
                "Бесплатная версия отслеживает {FREE_TIER_BACKUP_LIMIT} бэкап. Введите лицензионный ключ, чтобы добавить ещё."
            ));
        }
    }
    db::insert_backup(&conn, &new).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn activate_license(db: State<Db>, key: String) -> Result<LicenseInfo, String> {
    let info = license::verify_key(&key)?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::save_license(&conn, &key, &info.license_ref, &info.tier).map_err(|e| e.to_string())?;
    Ok(info)
}

#[tauri::command]
pub fn get_license_status(db: State<Db>) -> Result<Option<LicenseInfo>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    Ok(db::get_license(&conn)
        .map_err(|e| e.to_string())?
        .map(|(license_ref, tier)| LicenseInfo { tier, license_ref }))
}

#[tauri::command]
pub fn list_backups(db: State<Db>) -> Result<Vec<BackupTarget>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::list_backups(&conn).map_err(|e| e.to_string())
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
