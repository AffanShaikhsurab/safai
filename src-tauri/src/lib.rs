//! Safai Tauri v2 backend (thin) — builder wiring.
//!
//! Per `tauri-v2-guide.md` §1/§14, the app builder lives in `lib.rs` inside a
//! `run()` function (annotated for mobile entry), and `main.rs` calls it. This
//! keeps the same crate buildable for desktop and mobile.
//!
//! ## Background operation
//!
//! Safai can stay resident to do proactive maintenance (see [`schedule`]). That
//! turns three wiring details into requirements rather than niceties:
//!
//! * **single-instance** must be the first plugin registered, so a second launch
//!   (from the Start menu, or a duplicated logon entry) hands off to the running
//!   process instead of creating a second tray icon and a second scheduler.
//! * **autostart** registers a per-user logon entry with a `--autostart` flag,
//!   which [`is_autostart_launch`] uses to come up hidden — launching straight
//!   into a window the user didn't ask for would be obnoxious.
//! * **close-to-tray** intercepts `CloseRequested` so closing the window parks
//!   the app instead of killing the scheduler, but only while automation is
//!   actually enabled. If there's nothing to stay resident *for*, close means
//!   quit.

mod commands;
mod delete_engine;
mod dto;
mod engine;
mod error;
mod schedule;
mod state;
mod tray;
mod winsys;

use commands::*;
use state::SafaiState;
use tauri::{Manager, WindowEvent};

/// Flag the autostart plugin passes on a logon launch.
const AUTOSTART_FLAG: &str = "--autostart";

/// Was this process started by the logon entry rather than by the user?
fn is_autostart_launch() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_FLAG)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must come first: everything below assumes one live instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch just surfaces the window we already have.
            tray::show_main_window(app);
        }))
        // Plugins (§7): folder picker, reveal-in-explorer, OS info, settings.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_FLAG]),
        ))
        .setup(|app| {
            // Seed managed state (allow-list from default roots, cancel flag,
            // empty last-scan map, activity gate).
            app.manage(SafaiState::new());

            // Tray first: the scheduler's status pushes refresh it, so it has
            // to exist before the tick loop starts.
            tray::init(app.handle())?;
            schedule::init(app.handle());

            // The window is configured `visible: false` so a logon launch never
            // flashes a window the user didn't ask for. A user-initiated launch
            // shows it here instead, which also avoids the white first-paint
            // flash on a normal start.
            if !is_autostart_launch() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            // Paint the tray with real status instead of the placeholder.
            schedule::push_status(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // `try_runtime`: a close before `setup` finished should quit,
                // not panic.
                let Some(runtime) = schedule::runner::try_runtime(window.app_handle()) else {
                    return;
                };
                let cfg = runtime.config();
                // Only park in the tray when there's a reason to stay alive.
                if cfg.enabled && cfg.minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan,
            cancel_scan,
            preview_delete,
            delete,
            open_path,
            detect_tools,
            default_roots,
            cleanup_rules,
            drive_info,
            automation_status,
            set_automation_config,
            run_automation_now,
            stop_automation,
            set_ui_engaged,
            hide_to_tray
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
