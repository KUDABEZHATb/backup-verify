use crate::models::{BackupTarget, CheckStatus};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// Silence-unless-broken: we only ever surface a native OS notification when
/// a check comes back warning/error. A clean check is not worth interrupting
/// anyone for.
pub fn notify_if_problem(app: &AppHandle, backup: &BackupTarget, status: CheckStatus, message: &str) {
    if status == CheckStatus::Ok || status == CheckStatus::Pending {
        return;
    }
    let title = match status {
        CheckStatus::Error => format!("Проблема с бэкапом «{}»", backup.name),
        CheckStatus::Warning => format!("Стоит проверить бэкап «{}»", backup.name),
        _ => return,
    };
    let _ = app.notification().builder().title(title).body(message).show();
}
