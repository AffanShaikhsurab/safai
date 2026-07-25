//! The Tauri command layer (implementation-plan.md §4).
//!
//! This layer is deliberately **thin**: all scanning/rules logic lives in
//! `safai-rules` (which itself calls `safai-core`), the shared execution cores
//! live in `engine`, and the automation policy lives in `schedule`. Here we only:
//!   * adapt the engine's event sink to a Tauri `Channel`,
//!   * hold the activity gate so interactive and scheduled work can't collide,
//!   * move heavy work off the async runtime via `spawn_blocking`,
//!   * and marshal results into the §3.4 DTOs.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use safai_rules::{CleanupItem, ScanEvent};

use crate::dto::{
    DeleteEvent, DeletePlan, DeletePlanItem, DeleteReport, DriveInfo, RuleInfo, SafetyTier,
    ToolInfo,
};
use crate::engine::{self, drive_mount, is_within_allowed, normalize_slashes};
use crate::error::{Result, SafaiError};
use crate::schedule::{self, AutomationStatus, ScheduleConfig};
use crate::state::SafaiState;
use crate::winsys;

// -------------------------------------------------------------------------
// scan
// -------------------------------------------------------------------------

/// Run the rules-based scan, streaming `ScanEvent`s through `on_event` and
/// returning the aggregated `ScanReport`. The heavy walk runs on a blocking
/// thread; progress is streamed live via the (cloneable) `Channel`.
#[tauri::command]
pub async fn scan(
    roots: Vec<String>,
    on_event: Channel<ScanEvent>,
    state: State<'_, SafaiState>,
) -> Result<safai_rules::ScanReport> {
    // Hold the activity slot for the duration so a scheduled run can't start a
    // second scan and invalidate the id map underneath this one.
    let _hold = state
        .try_acquire()
        .ok_or_else(|| SafaiError::Other("a scan or cleanup is already running".to_string()))?;

    // Fresh run: clear any stale cancellation, then hand a clone to the worker.
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();

    let cfg = engine::build_scan_config(&roots, true);

    // Move the blocking rayon scan off the async runtime worker threads. The
    // engine takes a `&(dyn Fn(ScanEvent) + Send + Sync)` sink; we adapt it to
    // the Channel by cloning `on_event` into a closure and forwarding each
    // event (ignoring send errors — a dropped receiver just means no UI).
    let report = tauri::async_runtime::spawn_blocking(move || {
        let sink = move |ev: ScanEvent| {
            let _ = on_event.send(ev);
        };
        engine::scan_blocking(&cfg, &cancel, &sink)
    })
    .await
    .map_err(|e| SafaiError::Other(format!("scan task join error: {e}")))?;

    // Populate the id → item map from the finished report so later
    // preview/delete calls resolve ids server-side.
    {
        let mut items = state.last_items.lock().unwrap();
        items.clear();
        for group in &report.groups {
            for item in &group.items {
                items.insert(item.id.clone(), item.clone());
            }
        }
    }

    Ok(report)
}

// -------------------------------------------------------------------------
// cancel_scan
// -------------------------------------------------------------------------

/// Flip the shared cancel flag; the running scan observes it and winds down.
#[tauri::command]
pub fn cancel_scan(state: State<'_, SafaiState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

// -------------------------------------------------------------------------
// preview_delete
// -------------------------------------------------------------------------

/// Dry run: resolve ids server-side, re-check the allow-list, and report what
/// would happen without touching the disk.
#[tauri::command]
pub async fn preview_delete(ids: Vec<String>, state: State<'_, SafaiState>) -> Result<DeletePlan> {
    let roots = state.allowed_roots.lock().unwrap().clone();
    let map = state.last_items.lock().unwrap();

    let mut items: Vec<DeletePlanItem> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut blocked_count: u32 = 0;

    for id in &ids {
        match map.get(id) {
            Some(item) => {
                let path = PathBuf::from(&item.path);
                let allowed = is_within_allowed(&path, &roots);
                let reason = if allowed {
                    None
                } else {
                    Some("outside allowed roots".to_string())
                };
                if allowed {
                    total_bytes = total_bytes.saturating_add(item.size_bytes);
                } else {
                    blocked_count += 1;
                }
                items.push(DeletePlanItem {
                    id: item.id.clone(),
                    path: item.path.clone(),
                    size_bytes: item.size_bytes,
                    tier: item.tier,
                    allowed,
                    reason,
                });
            }
            None => {
                // Unknown id — never trust it; report it as blocked.
                blocked_count += 1;
                items.push(DeletePlanItem {
                    id: id.clone(),
                    path: String::new(),
                    size_bytes: 0,
                    tier: SafetyTier::Caution,
                    allowed: false,
                    reason: Some("item not found in last scan".to_string()),
                });
            }
        }
    }

    Ok(DeletePlan {
        items,
        total_bytes,
        blocked_count,
    })
}

// -------------------------------------------------------------------------
// delete
// -------------------------------------------------------------------------

/// Delete the selected items (Recycle Bin by default). Resolves ids
/// server-side, re-checks the guardrail per item, streams `DeleteEvent`s, and
/// returns a `DeleteReport`.
#[tauri::command]
pub async fn delete(
    ids: Vec<String>,
    to_recycle_bin: bool,
    on_event: Channel<DeleteEvent>,
    state: State<'_, SafaiState>,
) -> Result<DeleteReport> {
    let _hold = state
        .try_acquire()
        .ok_or_else(|| SafaiError::Other("a scan or cleanup is already running".to_string()))?;

    // A cancelled scan leaves the shared flag set; clear it so the deletion
    // doesn't abort on its first item.
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();

    let roots = state.allowed_roots.lock().unwrap().clone();

    // Resolve ids → items up front (locks can't be held across the blocking
    // task); pair each id with its stored item (or `None` if unknown).
    let resolved: Vec<(String, Option<CleanupItem>)> = {
        let map = state.last_items.lock().unwrap();
        ids.iter()
            .map(|id| (id.clone(), map.get(id).cloned()))
            .collect()
    };

    let report = tauri::async_runtime::spawn_blocking(move || {
        let sink = move |ev: DeleteEvent| {
            let _ = on_event.send(ev);
        };
        engine::delete_blocking(resolved, &roots, to_recycle_bin, &cancel, &sink)
    })
    .await
    .map_err(|e| SafaiError::Other(format!("delete task join error: {e}")))?;

    Ok(report)
}

// -------------------------------------------------------------------------
// open_path
// -------------------------------------------------------------------------

/// Reveal (highlight) a path in the system file explorer.
#[tauri::command]
pub fn open_path(path: String, app: AppHandle) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| SafaiError::Other(e.to_string()))?;
    Ok(())
}

// -------------------------------------------------------------------------
// detect_tools / default_roots / cleanup_rules
// -------------------------------------------------------------------------

/// Which dev tools are installed (UI chips + rule gating).
#[tauri::command]
pub fn detect_tools() -> Vec<ToolInfo> {
    safai_rules::detect_tools()
        .into_iter()
        .map(|(id, label, detected)| ToolInfo {
            id,
            label,
            detected,
        })
        .collect()
}

/// Suggested scan roots as forward-slash display strings.
#[tauri::command]
pub fn default_roots(_app: AppHandle) -> Vec<String> {
    safai_rules::default_roots()
        .iter()
        .map(|p| normalize_slashes(&p.to_string_lossy()))
        .collect()
}

/// The full cleanup rule table, so the Automation screen can offer a per-rule
/// opt-in for autopilot instead of making the user trust a category blanket.
#[tauri::command]
pub fn cleanup_rules() -> Vec<RuleInfo> {
    safai_rules::rules::all_rules()
        .into_iter()
        .map(|rule| RuleInfo {
            id: rule.id.to_string(),
            label: rule.label.to_string(),
            category: rule.category,
            tier: rule.tier,
            regenerates: rule.regenerates,
            note: rule.note.to_string(),
            // Pattern rules are discovered by directory name, not a fixed path.
            pattern_based: rule.pattern.is_some(),
        })
        .collect()
}

// -------------------------------------------------------------------------
// drive_info
// -------------------------------------------------------------------------

/// Free/total bytes for the drive containing `path` (header gauge).
///
/// Backed by `GetDiskFreeSpaceExW` on Windows (see [`crate::winsys`]); other
/// platforms report zeros, as Safai is Windows-first.
#[tauri::command]
pub fn drive_info(path: String) -> Result<DriveInfo> {
    let p = PathBuf::from(&path);
    let mount = drive_mount(&p);

    match winsys::disk_free_total(&p) {
        Some((free_bytes, total_bytes)) => Ok(DriveInfo {
            mount,
            free_bytes,
            total_bytes,
        }),
        None if cfg!(windows) => Err(SafaiError::Other(format!(
            "failed to query disk space for {path}"
        ))),
        // Non-Windows fallback: report zeros rather than failing the UI.
        None => Ok(DriveInfo {
            mount,
            free_bytes: 0,
            total_bytes: 0,
        }),
    }
}

// -------------------------------------------------------------------------
// Automation
// -------------------------------------------------------------------------

/// Current automation config + schedule + audit trail.
#[tauri::command]
pub fn automation_status(app: AppHandle) -> AutomationStatus {
    schedule::status(&app)
}

/// Persist a new automation config. Sanitizes the input, reconciles the OS
/// logon entry, and re-evaluates the schedule immediately.
#[tauri::command]
pub fn set_automation_config(app: AppHandle, config: ScheduleConfig) -> AutomationStatus {
    schedule::apply_config(&app, config)
}

/// Queue an immediate automation run, bypassing cadence and constraints.
#[tauri::command]
pub fn run_automation_now(app: AppHandle) {
    schedule::runtime(&app).request_run();
}

/// Ask the in-flight automation run to wind down.
#[tauri::command]
pub fn stop_automation(app: AppHandle) {
    schedule::runtime(&app).cancel_run();
}

/// Tell the backend whether the user is mid-flow in the Clean screens, so
/// automation stays out of the way instead of invalidating their selection.
#[tauri::command]
pub fn set_ui_engaged(engaged: bool, state: State<'_, SafaiState>) {
    state.set_ui_engaged(engaged);
}

/// Hide the main window to the tray (used by the "close to tray" affordance).
#[tauri::command]
pub fn hide_to_tray(app: AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}
