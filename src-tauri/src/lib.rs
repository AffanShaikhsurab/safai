//! Safai Tauri v2 backend (thin) — builder wiring.
//!
//! Per `tauri-v2-guide.md` §1/§14, the app builder lives in `lib.rs` inside a
//! `run()` function (annotated for mobile entry), and `main.rs` calls it. This
//! keeps the same crate buildable for desktop and mobile.

mod commands;
mod delete_engine;
mod dto;
mod error;
mod state;

use commands::*;
use state::SafaiState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Plugins (§7): folder picker, reveal-in-explorer, OS info, settings.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Seed managed state (allow-list from default roots, cancel flag,
            // empty last-scan map).
            app.manage(SafaiState::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan,
            cancel_scan,
            preview_delete,
            delete,
            open_path,
            detect_tools,
            default_roots,
            drive_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
