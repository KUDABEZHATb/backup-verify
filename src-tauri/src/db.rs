use crate::models::{BackupTarget, CheckRun, CheckStatus, FileRecord, NewBackup, Schedule};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

/// Thin wrapper around a single SQLite connection guarded by a mutex.
/// The app is single-user/single-process, so a connection pool would be
/// overkill — one connection shared between the UI commands and the
/// background scheduler is enough and keeps the schema/queries in one place.
pub struct Db(pub Mutex<Connection>);

pub fn app_data_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app_handle
        .path()
        .app_data_dir()
        .expect("app data dir must resolve")
}

pub fn open(app_handle: &tauri::AppHandle) -> Db {
    let dir = app_data_dir(app_handle);
    std::fs::create_dir_all(&dir).expect("failed to create app data dir");
    let db_path = dir.join("backup-verify.sqlite3");
    let conn = Connection::open(db_path).expect("failed to open sqlite database");
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS backups (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            path            TEXT NOT NULL,
            schedule        TEXT NOT NULL,
            created_at      TEXT NOT NULL,
            last_check_at   TEXT,
            last_status     TEXT NOT NULL DEFAULT 'pending',
            last_message    TEXT
        );

        CREATE TABLE IF NOT EXISTS checks (
            id                          INTEGER PRIMARY KEY AUTOINCREMENT,
            backup_id                   TEXT NOT NULL REFERENCES backups(id) ON DELETE CASCADE,
            started_at                  TEXT NOT NULL,
            finished_at                 TEXT NOT NULL,
            status                      TEXT NOT NULL,
            message                     TEXT NOT NULL,
            files_scanned               INTEGER NOT NULL,
            files_sampled               INTEGER NOT NULL,
            files_changed_unexpectedly  INTEGER NOT NULL,
            files_missing               INTEGER NOT NULL,
            newest_file_at              TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_checks_backup ON checks(backup_id, started_at DESC);

        CREATE TABLE IF NOT EXISTS license (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            raw_key         TEXT NOT NULL,
            license_ref     TEXT NOT NULL,
            tier            TEXT NOT NULL,
            activated_at    TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS file_hashes (
            backup_id   TEXT NOT NULL REFERENCES backups(id) ON DELETE CASCADE,
            rel_path    TEXT NOT NULL,
            size        INTEGER NOT NULL,
            mtime       TEXT NOT NULL,
            hash        TEXT NOT NULL,
            last_seen   TEXT NOT NULL,
            PRIMARY KEY (backup_id, rel_path)
        );
        ",
    )
    .expect("failed to init schema");
    Db(Mutex::new(conn))
}

fn row_to_backup(row: &rusqlite::Row) -> rusqlite::Result<BackupTarget> {
    Ok(BackupTarget {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        schedule: Schedule::from_str(&row.get::<_, String>(3)?),
        created_at: row.get(4)?,
        last_check_at: row.get(5)?,
        last_status: CheckStatus::from_str(&row.get::<_, String>(6)?),
        last_message: row.get(7)?,
    })
}

pub fn insert_backup(conn: &Connection, new: &NewBackup) -> rusqlite::Result<BackupTarget> {
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO backups (id, name, path, schedule, created_at, last_status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
        params![id, new.name, new.path, new.schedule.as_str(), created_at],
    )?;
    Ok(BackupTarget {
        id,
        name: new.name.clone(),
        path: new.path.clone(),
        schedule: new.schedule,
        created_at,
        last_check_at: None,
        last_status: CheckStatus::Pending,
        last_message: None,
    })
}

pub fn backup_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM backups", [], |row| row.get(0))
}

pub fn save_license(conn: &Connection, raw_key: &str, license_ref: &str, tier: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO license (id, raw_key, license_ref, tier, activated_at) VALUES (1, ?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET raw_key = excluded.raw_key, license_ref = excluded.license_ref,
            tier = excluded.tier, activated_at = excluded.activated_at",
        params![raw_key, license_ref, tier, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub fn get_license(conn: &Connection) -> rusqlite::Result<Option<(String, String)>> {
    conn.query_row("SELECT license_ref, tier FROM license WHERE id = 1", [], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .optional()
}

pub fn list_backups(conn: &Connection) -> rusqlite::Result<Vec<BackupTarget>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, path, schedule, created_at, last_check_at, last_status, last_message
         FROM backups ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], row_to_backup)?;
    rows.collect()
}

pub fn get_backup(conn: &Connection, id: &str) -> rusqlite::Result<Option<BackupTarget>> {
    conn.query_row(
        "SELECT id, name, path, schedule, created_at, last_check_at, last_status, last_message
         FROM backups WHERE id = ?1",
        params![id],
        row_to_backup,
    )
    .optional()
}

pub fn delete_backup(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM backups WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn update_backup_result(
    conn: &Connection,
    backup_id: &str,
    status: CheckStatus,
    message: &str,
    checked_at: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE backups SET last_check_at = ?1, last_status = ?2, last_message = ?3 WHERE id = ?4",
        params![checked_at, status.as_str(), message, backup_id],
    )?;
    Ok(())
}

pub fn insert_check(conn: &Connection, run: &CheckRun) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO checks (backup_id, started_at, finished_at, status, message,
            files_scanned, files_sampled, files_changed_unexpectedly, files_missing, newest_file_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            run.backup_id,
            run.started_at,
            run.finished_at,
            run.status.as_str(),
            run.message,
            run.files_scanned,
            run.files_sampled,
            run.files_changed_unexpectedly,
            run.files_missing,
            run.newest_file_at,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn history(conn: &Connection, backup_id: &str, limit: i64) -> rusqlite::Result<Vec<CheckRun>> {
    let mut stmt = conn.prepare(
        "SELECT id, backup_id, started_at, finished_at, status, message,
                files_scanned, files_sampled, files_changed_unexpectedly, files_missing, newest_file_at
         FROM checks WHERE backup_id = ?1 ORDER BY started_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![backup_id, limit], |row| {
        Ok(CheckRun {
            id: row.get(0)?,
            backup_id: row.get(1)?,
            started_at: row.get(2)?,
            finished_at: row.get(3)?,
            status: CheckStatus::from_str(&row.get::<_, String>(4)?),
            message: row.get(5)?,
            files_scanned: row.get(6)?,
            files_sampled: row.get(7)?,
            files_changed_unexpectedly: row.get(8)?,
            files_missing: row.get(9)?,
            newest_file_at: row.get(10)?,
        })
    })?;
    rows.collect()
}

/// Baseline lookup used by the checker: existing hash + mtime for a relative path, if any.
pub fn get_file_record(
    conn: &Connection,
    backup_id: &str,
    rel_path: &str,
) -> rusqlite::Result<Option<FileRecord>> {
    conn.query_row(
        "SELECT rel_path, size, mtime, hash FROM file_hashes WHERE backup_id = ?1 AND rel_path = ?2",
        params![backup_id, rel_path],
        |row| {
            Ok(FileRecord {
                rel_path: row.get(0)?,
                size: row.get(1)?,
                mtime: row.get(2)?,
                hash: row.get(3)?,
            })
        },
    )
    .optional()
}

pub fn upsert_file_record(
    conn: &Connection,
    backup_id: &str,
    rec: &FileRecord,
    seen_at: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO file_hashes (backup_id, rel_path, size, mtime, hash, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(backup_id, rel_path) DO UPDATE SET
            size = excluded.size, mtime = excluded.mtime, hash = excluded.hash, last_seen = excluded.last_seen",
        params![backup_id, rec.rel_path, rec.size, rec.mtime, rec.hash, seen_at],
    )?;
    Ok(())
}

/// Marks a tracked file as still present in this run's walk without
/// recomputing its hash (used for files that exist but weren't sampled).
pub fn touch_seen(conn: &Connection, backup_id: &str, rel_path: &str, seen_at: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE file_hashes SET last_seen = ?1 WHERE backup_id = ?2 AND rel_path = ?3",
        params![seen_at, backup_id, rel_path],
    )?;
    Ok(())
}

/// Relative paths tracked from a previous run but not touched in this one —
/// candidates for "file went missing" reporting.
pub fn stale_tracked_paths(
    conn: &Connection,
    backup_id: &str,
    seen_at: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT rel_path FROM file_hashes WHERE backup_id = ?1 AND last_seen != ?2",
    )?;
    let rows = stmt.query_map(params![backup_id, seen_at], |row| row.get(0))?;
    rows.collect()
}

pub fn forget_file_record(conn: &Connection, backup_id: &str, rel_path: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM file_hashes WHERE backup_id = ?1 AND rel_path = ?2",
        params![backup_id, rel_path],
    )?;
    Ok(())
}

/// Least-recently-verified tracked paths, used to pick the sample for this run
/// (so every file eventually gets re-hashed instead of always sampling the same ones).
pub fn least_recently_seen_paths(
    conn: &Connection,
    backup_id: &str,
    limit: i64,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT rel_path FROM file_hashes WHERE backup_id = ?1 ORDER BY last_seen ASC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![backup_id, limit], |row| row.get(0))?;
    rows.collect()
}
