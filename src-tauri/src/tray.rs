//! System-tray presence.
//!
//! The tray is what makes proactive maintenance possible: it lets Safai stay
//! resident (and be launched at logon straight into the background) while
//! keeping a visible, one-click way back to the window. Without it, an app that
//! quietly runs scans would be indistinguishable from malware.
//!
//! The menu is rebuilt on every status change rather than mutated item by item.
//! It has a handful of entries and rebuilding costs microseconds, so this trades
//! nothing meaningful for not having to keep item handles in sync with the
//! runtime state.

use chrono::{Local, TimeZone};
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::schedule::runner::human_bytes;
use crate::schedule::{self, AutomationStatus, RunPhase};

/// Stable id so the tray can be looked up again for updates.
const TRAY_ID: &str = "safai-tray";

// Menu item ids.
const ID_OPEN: &str = "open";
const ID_RUN_NOW: &str = "run_now";
const ID_STOP: &str = "stop";
const ID_TOGGLE: &str = "toggle";
const ID_STATUS: &str = "status";
const ID_QUIT: &str = "quit";

/// Build the tray icon and wire its events. Call once from `setup`.
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, None)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Safai — disk cleanup")
        .menu(&menu)
        // Left click opens the window; the menu stays on right click, which is
        // what Windows users expect from a tray icon.
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

/// Bring the main window back from the tray.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        ID_OPEN => show_main_window(app),
        ID_RUN_NOW => {
            schedule::runtime(app).request_run();
            show_main_window(app);
        }
        ID_STOP => schedule::runtime(app).cancel_run(),
        ID_TOGGLE => {
            let mut cfg = schedule::runtime(app).config();
            cfg.enabled = !cfg.enabled;
            schedule::apply_config(app, cfg);
        }
        ID_QUIT => app.exit(0),
        _ => {}
    }
}

/// Rebuild the tray menu + tooltip from the latest status.
///
/// Safe to call from any thread: the mutation is marshalled onto the main
/// thread, which menu APIs require on some platforms.
pub fn refresh(app: &AppHandle, status: &AutomationStatus) {
    let snapshot = TraySnapshot::from(status);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(TRAY_ID) else {
            return;
        };
        if let Ok(menu) = build_menu(&handle, Some(&snapshot)) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_tooltip(Some(&snapshot.tooltip));
    });
}

/// The few strings and flags the tray needs, extracted so the closure sent to
/// the main thread doesn't have to own the whole status payload.
#[derive(Clone)]
struct TraySnapshot {
    enabled: bool,
    running: bool,
    status_line: String,
    tooltip: String,
}

impl From<&AutomationStatus> for TraySnapshot {
    fn from(status: &AutomationStatus) -> Self {
        let status_line = if status.running {
            match status.phase {
                RunPhase::Cleaning => format!(
                    "Cleaning… {} freed",
                    human_bytes(status.progress.reclaimed_bytes)
                ),
                _ => format!(
                    "Scanning… {} found",
                    human_bytes(status.progress.found_bytes)
                ),
            }
        } else if let Some(reason) = &status.deferred_reason {
            capitalize(reason)
        } else if !status.config.enabled {
            "Automation is off".to_string()
        } else if let Some(next) = status.next_due_at {
            format!("Next run {}", format_when(next))
        } else {
            "Watching disk usage".to_string()
        };

        let tooltip = match status.disk_used_percent {
            Some(pct) => format!("Safai — drive {pct:.0}% full\n{status_line}"),
            None => format!("Safai\n{status_line}"),
        };

        TraySnapshot {
            enabled: status.config.enabled,
            running: status.running,
            status_line,
            tooltip,
        }
    }
}

fn build_menu(app: &AppHandle, snapshot: Option<&TraySnapshot>) -> tauri::Result<Menu<Wry>> {
    let default = TraySnapshot {
        enabled: false,
        running: false,
        status_line: "Starting…".to_string(),
        tooltip: String::new(),
    };
    let snap = snapshot.unwrap_or(&default);

    // A disabled item used purely as a status read-out.
    let status_item = MenuItem::with_id(app, ID_STATUS, &snap.status_line, false, None::<&str>)?;
    let open = MenuItem::with_id(app, ID_OPEN, "Open Safai", true, None::<&str>)?;
    let run_now = MenuItem::with_id(
        app,
        ID_RUN_NOW,
        "Run maintenance now",
        !snap.running,
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(app, ID_STOP, "Stop", snap.running, None::<&str>)?;
    let toggle = MenuItem::with_id(
        app,
        ID_TOGGLE,
        if snap.enabled {
            "Turn automation off"
        } else {
            "Turn automation on"
        },
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, ID_QUIT, "Quit Safai", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &status_item,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &PredefinedMenuItem::separator(app)?,
            &run_now,
            &stop,
            &toggle,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )
}

/// "in 40m", "in 6h", "in 3d" — short enough for a tray line.
fn format_when(unix_secs: u64) -> String {
    let Some(target) = Local.timestamp_opt(unix_secs as i64, 0).single() else {
        return "soon".to_string();
    };
    let delta = target.signed_duration_since(Local::now());
    let mins = delta.num_minutes();

    if mins <= 0 {
        "now".to_string()
    } else if mins < 60 {
        format!("in {mins}m")
    } else if delta.num_hours() < 24 {
        format!("in {}h", delta.num_hours())
    } else {
        format!("in {}d", delta.num_days().max(1))
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
