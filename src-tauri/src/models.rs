use serde::{Deserialize, Serialize};

/// How often a backup target should be checked.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Schedule {
    Daily,
    Weekly,
    Monthly,
}

impl Schedule {
    pub fn as_str(&self) -> &'static str {
        match self {
            Schedule::Daily => "daily",
            Schedule::Weekly => "weekly",
            Schedule::Monthly => "monthly",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "weekly" => Schedule::Weekly,
            "monthly" => Schedule::Monthly,
            _ => Schedule::Daily,
        }
    }

    /// Interval used both to decide when a check is due and to judge staleness
    /// (a backup that hasn't been touched for longer than this is suspicious).
    pub fn interval_seconds(&self) -> i64 {
        match self {
            Schedule::Daily => 24 * 3600,
            Schedule::Weekly => 7 * 24 * 3600,
            Schedule::Monthly => 30 * 24 * 3600,
        }
    }
}

/// Overall verdict of the most recent check, shown as the status pill in the UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
    /// Never checked yet (just added).
    Pending,
}

impl CheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Warning => "warning",
            CheckStatus::Error => "error",
            CheckStatus::Pending => "pending",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "ok" => CheckStatus::Ok,
            "warning" => CheckStatus::Warning,
            "error" => CheckStatus::Error,
            _ => CheckStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTarget {
    pub id: String,
    pub name: String,
    pub path: String,
    pub schedule: Schedule,
    pub created_at: String,
    pub last_check_at: Option<String>,
    pub last_status: CheckStatus,
    pub last_message: Option<String>,
    /// True if this backup is beyond the install's current license quota —
    /// set by `commands::list_backups`, not stored in the database. The
    /// scheduler and manual checks both refuse to act on a locked backup,
    /// so a row that ends up here some other way than `add_backup` (e.g.
    /// inserted directly into the database) is inert, not a working extra
    /// backup.
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRun {
    pub id: i64,
    pub backup_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: CheckStatus,
    pub message: String,
    pub files_scanned: i64,
    pub files_sampled: i64,
    pub files_changed_unexpectedly: i64,
    pub files_missing: i64,
    pub newest_file_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub rel_path: String,
    pub size: i64,
    pub mtime: String,
    pub hash: String,
}

/// Payload the frontend sends to register a new backup target.
#[derive(Debug, Deserialize)]
pub struct NewBackup {
    pub name: String,
    pub path: String,
    pub schedule: Schedule,
}
