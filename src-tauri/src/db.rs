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
            receipt         TEXT NOT NULL,
            signature       TEXT NOT NULL,
            activated_at    TEXT NOT NULL
        );

        -- A stable per-install id sent to the license server on activation
        -- and revalidation. Generated once and kept regardless of whether
        -- a license is currently active, so re-activating the same install
        -- doesn't silently count as a second device against the key's
        -- activation limit.
        CREATE TABLE IF NOT EXISTS device (
            id          INTEGER PRIMARY KEY CHECK (id = 1),
            machine_id  TEXT NOT NULL
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
        locked: false, // filled in by commands::list_backups, which knows the current quota
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
        locked: false, // it was just accepted by add_backup's own quota check
    })
}

pub fn backup_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM backups", [], |row| row.get(0))
}

/// This install's stable id, generated once on first use and kept from
/// then on — sent to the license server so it can tell "the same install
/// re-activating" from "a new device," and enforce its per-key limit.
pub fn machine_id(conn: &Connection) -> rusqlite::Result<String> {
    let existing: Option<String> = conn
        .query_row("SELECT machine_id FROM device WHERE id = 1", [], |row| row.get(0))
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    conn.execute("INSERT INTO device (id, machine_id) VALUES (1, ?1)", params![id])?;
    Ok(id)
}

pub fn save_receipt(conn: &Connection, receipt_b64: &str, signature_b64: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO license (id, receipt, signature, activated_at) VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET receipt = excluded.receipt, signature = excluded.signature,
            activated_at = excluded.activated_at",
        params![receipt_b64, signature_b64, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn stored_receipt(conn: &Connection) -> rusqlite::Result<Option<(String, String)>> {
    conn.query_row("SELECT receipt, signature FROM license WHERE id = 1", [], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .optional()
}

/// Re-verifies the stored receipt's signature and expiry on every call
/// instead of trusting the row's mere existence. The old scheme trusted
/// existence alone, so anyone could open the SQLite file in a free DB
/// browser, insert one row, and pass as licensed forever — no crypto
/// knowledge, no reverse-engineering. A tampered, forged, or expired row
/// now just fails verification and falls back to unlicensed.
pub fn get_license(conn: &Connection) -> rusqlite::Result<Option<crate::license::LicenseInfo>> {
    let stored = stored_receipt(conn)?;
    Ok(stored.and_then(|(r, s)| crate::license::status_from_stored(&r, &s)))
}

/// The key + machine_id from the stored receipt, if its signature is
/// genuine — used to ask the server for a fresh receipt. Unlike
/// `get_license`, this doesn't care whether the receipt has expired: an
/// expired-but-genuine receipt is exactly the case revalidation exists for.
pub fn receipt_for_revalidation(conn: &Connection) -> rusqlite::Result<Option<(String, String)>> {
    let stored = stored_receipt(conn)?;
    Ok(stored
        .and_then(|(r, s)| crate::license::verify_receipt(&r, &s).ok())
        .map(|receipt| (receipt.key, receipt.machine_id)))
}

/// How many backups this install is currently entitled to actively use —
/// the free-tier limit, or whatever a currently-valid receipt grants.
pub fn current_max_backups(conn: &Connection) -> rusqlite::Result<i64> {
    Ok(get_license(conn)?.map(|l| l.max_backups).unwrap_or(crate::license::FREE_TIER_BACKUP_LIMIT))
}

/// The ids of the backups this install is actually entitled to have the
/// app act on: the first `max_backups` by creation order. Enforcement is
/// re-derived from this on every read (list/schedule/check), not just once
/// when a backup is added — so a row inserted directly into `backups`
/// behind the app's back (bypassing add_backup's own count check) doesn't
/// grant a working extra backup, just an inert row the app never checks.
pub fn entitled_backup_ids(conn: &Connection, max_backups: i64) -> rusqlite::Result<Vec<String>> {
    let limit = max_backups.max(0);
    let mut stmt = conn.prepare("SELECT id FROM backups ORDER BY created_at ASC LIMIT ?1")?;
    let rows = stmt.query_map(params![limit], |row| row.get(0))?;
    rows.collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn license_only_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE license (
                id           INTEGER PRIMARY KEY CHECK (id = 1),
                receipt      TEXT NOT NULL,
                signature    TEXT NOT NULL,
                activated_at TEXT NOT NULL
            );
            CREATE TABLE backups (
                id TEXT PRIMARY KEY, created_at TEXT NOT NULL
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn no_row_means_unlicensed() {
        let conn = license_only_conn();
        assert!(get_license(&conn).unwrap().is_none());
        assert_eq!(current_max_backups(&conn).unwrap(), crate::license::FREE_TIER_BACKUP_LIMIT);
    }

    #[test]
    fn a_row_with_a_garbage_receipt_does_not_count_as_licensed() {
        // Regression test: a row used to grant a license just by existing,
        // regardless of whether the stored value was ever real — anyone
        // could open the SQLite file in a free DB browser and insert one to
        // get unlimited backups for free. get_license must re-verify the
        // receipt's signature, not just check that a row is there.
        let conn = license_only_conn();
        conn.execute(
            "INSERT INTO license (id, receipt, signature, activated_at)
             VALUES (1, 'not-a-real-receipt', 'not-a-real-signature', '2024-01-01')",
            [],
        )
        .unwrap();
        assert!(get_license(&conn).unwrap().is_none());
    }

    #[test]
    fn entitled_backup_ids_returns_only_the_first_n_by_creation_order() {
        let conn = license_only_conn();
        for (id, created_at) in [("a", "2024-01-01"), ("b", "2024-01-02"), ("c", "2024-01-03")] {
            conn.execute(
                "INSERT INTO backups (id, created_at) VALUES (?1, ?2)",
                params![id, created_at],
            )
            .unwrap();
        }
        assert_eq!(entitled_backup_ids(&conn, 2).unwrap(), vec!["a", "b"]);
        assert_eq!(entitled_backup_ids(&conn, 0).unwrap(), Vec::<String>::new());
        assert_eq!(entitled_backup_ids(&conn, 99).unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn machine_id_is_generated_once_and_kept() {
        let conn = license_only_conn();
        conn.execute(
            "CREATE TABLE device (id INTEGER PRIMARY KEY CHECK (id = 1), machine_id TEXT NOT NULL)",
            [],
        )
        .unwrap();
        let first = machine_id(&conn).unwrap();
        let second = machine_id(&conn).unwrap();
        assert_eq!(first, second);
    }
}
