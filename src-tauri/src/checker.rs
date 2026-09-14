use crate::db;
use crate::hashing::hash_file;
use crate::models::{BackupTarget, CheckRun, CheckStatus, FileRecord};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

// Tuned for a personal-computer backup folder (thousands to a few hundred
// thousand files), not a datacenter fileserver. See docs/architecture.md.
const NEW_FILE_HASH_CAP: usize = 3000;
const SAMPLE_MIN: usize = 25;
const SAMPLE_FRACTION: f64 = 0.10;
const SAMPLE_MAX: usize = 1000;
const MISSING_RATIO_WARN: f64 = 0.20;
const STALE_MULTIPLIER: i64 = 2;

struct WalkedFile {
    rel_path: String,
    abs_path: std::path::PathBuf,
    size: i64,
    mtime: String,
}

fn mtime_to_rfc3339(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .ok()
        .map(|t| {
            let dt: DateTime<Utc> = t.into();
            dt.to_rfc3339()
        })
        .unwrap_or_else(|| DateTime::<Utc>::from(UNIX_EPOCH).to_rfc3339())
}

/// Runs a check for a plain-folder backup target: walks the tree, verifies
/// the path is reachable, checks freshness, and hashes a sample of files to
/// catch silent corruption (bit rot) rather than just "did the job run".
pub fn check_folder_backup(conn: &Connection, backup: &BackupTarget) -> CheckRun {
    let started_at = Utc::now().to_rfc3339();
    let root = Path::new(&backup.path);

    if !root.exists() {
        return finish(
            backup,
            &started_at,
            CheckStatus::Error,
            "Путь недоступен — диск отключён или папка перемещена/удалена".to_string(),
            0,
            0,
            0,
            0,
            None,
        );
    }
    if !root.is_dir() {
        return finish(
            backup,
            &started_at,
            CheckStatus::Error,
            "Указанный путь — не папка".to_string(),
            0,
            0,
            0,
            0,
            None,
        );
    }

    let mut files: Vec<WalkedFile> = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        files.push(WalkedFile {
            rel_path: rel.to_string_lossy().replace('\\', "/"),
            abs_path: entry.path().to_path_buf(),
            size: meta.len() as i64,
            mtime: mtime_to_rfc3339(&meta),
        });
    }

    if files.is_empty() {
        return finish(
            backup,
            &started_at,
            CheckStatus::Warning,
            "Папка пуста — проверьте, что путь указывает на правильное место".to_string(),
            0,
            0,
            0,
            0,
            None,
        );
    }

    let newest_file_at = files.iter().map(|f| f.mtime.clone()).max();
    let seen_at = started_at.clone();

    // Split into "known before this run" vs "new to us", then decide what to hash.
    let mut known: Vec<&WalkedFile> = Vec::new();
    let mut unknown: Vec<&WalkedFile> = Vec::new();
    for f in &files {
        match db::get_file_record(conn, &backup.id, &f.rel_path) {
            Ok(Some(_)) => known.push(f),
            _ => unknown.push(f),
        }
    }

    // Sample the files least recently re-verified rather than a random draw,
    // so every file eventually gets re-hashed instead of a lucky few being
    // checked every run and the rest never — full coverage over time is the
    // property we actually want, not just "some sampling happened".
    let sample_target = ((known.len() as f64 * SAMPLE_FRACTION).ceil() as usize)
        .max(SAMPLE_MIN)
        .min(SAMPLE_MAX)
        .min(known.len());
    let due_for_resample: HashSet<String> = db::least_recently_seen_paths(conn, &backup.id, sample_target as i64)
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut files_sampled: i64 = 0;
    let mut files_changed_unexpectedly: i64 = 0;

    for f in known.iter() {
        if due_for_resample.contains(&f.rel_path) {
            files_sampled += 1;
            if let Ok(hash) = hash_file(&f.abs_path) {
                if let Ok(Some(prev)) = db::get_file_record(conn, &backup.id, &f.rel_path) {
                    handle_sampled_file(conn, backup, f, &hash, &prev, &seen_at, &mut files_changed_unexpectedly);
                }
            }
        } else {
            let _ = db::touch_seen(conn, &backup.id, &f.rel_path, &seen_at);
        }
    }

    let mut new_files_hashed: i64 = 0;
    for f in unknown.iter().take(NEW_FILE_HASH_CAP) {
        if let Ok(hash) = hash_file(&f.abs_path) {
            let rec = FileRecord {
                rel_path: f.rel_path.clone(),
                size: f.size,
                mtime: f.mtime.clone(),
                hash,
            };
            let _ = db::upsert_file_record(conn, &backup.id, &rec, &seen_at);
            new_files_hashed += 1;
        }
    }

    let missing = db::stale_tracked_paths(conn, &backup.id, &seen_at).unwrap_or_default();
    let files_missing = missing.len() as i64;
    let tracked_before = known.len() as i64 + files_missing; // rough baseline size before this run
    for rel in &missing {
        let _ = db::forget_file_record(conn, &backup.id, rel);
    }

    // --- decide overall status ---
    let mut status = CheckStatus::Ok;
    let mut reasons: Vec<String> = Vec::new();

    if let Some(newest) = &newest_file_at {
        if let Ok(newest_dt) = DateTime::parse_from_rfc3339(newest) {
            let age = Utc::now().signed_duration_since(newest_dt.with_timezone(&Utc));
            let threshold = backup.schedule.interval_seconds() * STALE_MULTIPLIER;
            if age.num_seconds() > threshold {
                status = CheckStatus::Warning;
                reasons.push(format!(
                    "самый свежий файл в бэкапе от {} — дольше ожидаемого периода",
                    newest_dt.format("%d.%m.%Y")
                ));
            }
        }
    }

    if files_changed_unexpectedly > 0 {
        status = CheckStatus::Error;
        reasons.push(format!(
            "{} файл(ов) изменились без изменения даты — похоже на повреждение данных",
            files_changed_unexpectedly
        ));
    }

    if tracked_before > 0 {
        let ratio = files_missing as f64 / tracked_before as f64;
        if ratio > MISSING_RATIO_WARN {
            if status == CheckStatus::Ok {
                status = CheckStatus::Warning;
            }
            reasons.push(format!(
                "пропало {} из {} ранее отслеженных файлов ({:.0}%)",
                files_missing,
                tracked_before,
                ratio * 100.0
            ));
        }
    }

    let message = if reasons.is_empty() {
        format!(
            "Всё в порядке: {} файлов, проверено выборочно {}",
            files.len(),
            files_sampled + new_files_hashed
        )
    } else {
        reasons.join("; ")
    };

    finish(
        backup,
        &started_at,
        status,
        message,
        files.len() as i64,
        files_sampled + new_files_hashed,
        files_changed_unexpectedly,
        files_missing,
        newest_file_at,
    )
}

fn handle_sampled_file(
    conn: &Connection,
    backup: &BackupTarget,
    f: &WalkedFile,
    hash: &str,
    prev: &FileRecord,
    seen_at: &str,
    files_changed_unexpectedly: &mut i64,
) {
    let mtime_unchanged = prev.mtime == f.mtime;
    let hash_unchanged = prev.hash == hash;

    if mtime_unchanged && !hash_unchanged {
        // Bytes differ but the file claims it wasn't touched — the signature
        // of silent corruption (bit rot, a bad sector, a failed sync) rather
        // than a legitimate edit. Flag it once, then adopt the new hash so
        // the same event doesn't re-alert on every future run.
        *files_changed_unexpectedly += 1;
    }

    let rec = FileRecord {
        rel_path: f.rel_path.clone(),
        size: f.size,
        mtime: f.mtime.clone(),
        hash: hash.to_string(),
    };
    let _ = db::upsert_file_record(conn, &backup.id, &rec, seen_at);
}

#[allow(clippy::too_many_arguments)]
fn finish(
    backup: &BackupTarget,
    started_at: &str,
    status: CheckStatus,
    message: String,
    files_scanned: i64,
    files_sampled: i64,
    files_changed_unexpectedly: i64,
    files_missing: i64,
    newest_file_at: Option<String>,
) -> CheckRun {
    CheckRun {
        id: 0,
        backup_id: backup.id.clone(),
        started_at: started_at.to_string(),
        finished_at: Utc::now().to_rfc3339(),
        status,
        message,
        files_scanned,
        files_sampled,
        files_changed_unexpectedly,
        files_missing,
        newest_file_at,
    }
}
