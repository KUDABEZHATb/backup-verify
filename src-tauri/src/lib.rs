mod checker;
mod commands;
mod db;
mod hashing;
mod license;
mod models;
mod notify;
mod scheduler;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

/// Brings the main window to front, including from minimized — `.show()`
/// alone does not un-minimize on Windows.
fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            let db = db::open(&handle);
            app.manage(db);

            // Tray icon: the app's real home. Checks keep running in the
            // background whether the window is open or not — closing the
            // window just hides it, it doesn't quit the scheduler.
            let show_item = MenuItem::with_id(app, "show", "Открыть", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Проверка бэкапов")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => show_main_window(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Only a left click opens the window — a right click
                    // must fall through to the tray's own context menu
                    // (Открыть/Выход), not steal the click.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            scheduler::start(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close = hide, not quit — the point of the app is the
            // background schedule, and the window is just for checking in.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_backup,
            commands::list_backups,
            commands::remove_backup,
            commands::get_history,
            commands::run_check_now,
            commands::activate_license,
            commands::get_license_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
