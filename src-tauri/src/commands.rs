//! The Tauri command layer (implementation-plan.md §4).
//!
//! This layer is deliberately **thin**: all scanning/rules logic lives in
//! `safai-rules` (which itself calls `safai-core`). Here we only:
//!   * adapt `safai_rules::run_scan`'s plain callback to a Tauri `Channel`,
//!   * enforce the deletion guardrails (allow-list + canonicalization),
//!   * move heavy work off the async runtime via `spawn_blocking`,
//!   * and marshal results into the §3.4 DTOs.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use safai_rules::{run_scan, CleanupItem, ScanConfig, ScanEvent};

use crate::dto::{
    DeleteEvent, DeletePlan, DeletePlanItem, DeleteReport, DriveInfo, SafetyTier, ToolInfo,
};
use crate::error::{Result, SafaiError};
use crate::state::SafaiState;

// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

/// Normalize a path string for the UI: forward slashes only (§7).
fn normalize_slashes(s: &str) -> String {
    s.replace('\\', "/")
}

/// Guardrail: is `path` inside one of the allow-listed roots?
///
/// Canonicalizes both sides (resolving symlinks and `..`) so a path can't
/// escape an allowed root via a link or relative segment. A path that fails to
/// canonicalize (e.g. already gone) is treated as **not** allowed.
fn is_within_allowed(path: &Path, roots: &[PathBuf]) -> bool {
    let canon = match std::fs::canonicalize(path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    roots.iter().any(|root| match std::fs::canonicalize(root) {
        Ok(root_canon) => canon.starts_with(&root_canon),
        // If the root itself can't be canonicalized, fall back to a raw compare.
        Err(_) => canon.starts_with(root),
    })
}

/// Best-effort "mount" label for a path (the drive prefix, e.g. `C:`).
fn drive_mount(path: &Path) -> String {
    for comp in path.components() {
        if let Component::Prefix(prefix) = comp {
            return normalize_slashes(&prefix.as_os_str().to_string_lossy());
        }
    }
    normalize_slashes(&path.to_string_lossy())
}

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
    // Fresh run: clear any stale cancellation, then hand a clone to the worker.
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();

    // Resolve roots: use what the UI provided, else the crate's defaults. The
    // same set feeds `project_scan_roots` (where pattern rules hunt).
    let resolved: Vec<PathBuf> = if roots.is_empty() {
        safai_rules::default_roots()
    } else {
        roots.iter().map(PathBuf::from).collect()
    };
    // Pattern rules hunt for project artifacts (`node_modules`, `target`, …),
    // which live in dev folders — NOT in the whole profile + AppData. Using a
    // targeted, bounded set of project roots (plus the depth limit + time
    // budget inside `run_scan`) is what stops the scan from crawling all of
    // AppData and appearing to hang.
    let project_roots = safai_rules::project_scan_roots();
    let cfg = ScanConfig {
        roots: resolved,
        project_scan_roots: project_roots,
        discover_large_folders: true,
    };

    // Move the blocking rayon scan off the async runtime worker threads. The
    // crate expects a `&mut dyn FnMut(ScanEvent)`; we adapt it to the Channel
    // by cloning `on_event` into the closure and forwarding each event via
    // `send` (ignoring send errors — a dropped receiver just means no UI).
    let report = tauri::async_runtime::spawn_blocking(move || {
        let mut forward = |ev: ScanEvent| {
            let _ = on_event.send(ev);
        };
        // `&cancel` (an `&Arc<AtomicBool>`) coerces to the `&AtomicBool` the
        // signature wants via Deref. `run_scan` returns a value, not a Result.
        run_scan(&cfg, &cancel, &mut forward)
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

/// Dry-run: resolve each id to its stored item, validate it against the
/// allow-list, and report per-item `allowed`/`reason` plus totals.
#[tauri::command]
pub async fn preview_delete(
    ids: Vec<String>,
    state: State<'_, SafaiState>,
) -> Result<DeletePlan> {
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
///
/// **Architecture (fixes the "stuck at 0/12" bug):**
/// - Each item is deleted on a dedicated thread with a 30-second timeout.
/// - Items are processed in parallel (up to 4 concurrent) so one stuck item
///   doesn't block the others.
/// - Transient sharing violations are retried with exponential backoff.
/// - Read-only files have their attribute stripped automatically.
/// - Progress events are emitted before AND after each item attempt.
#[tauri::command]
pub async fn delete(
    ids: Vec<String>,
    to_recycle_bin: bool,
    on_event: Channel<DeleteEvent>,
    state: State<'_, SafaiState>,
) -> Result<DeleteReport> {
    let roots = state.allowed_roots.lock().unwrap().clone();

    // Resolve ids → items up front (locks can't be held across the blocking
    // task); pair each id with its stored item (or `None` if unknown).
    let resolved: Vec<(String, Option<CleanupItem>)> = {
        let map = state.last_items.lock().unwrap();
        ids.iter().map(|id| (id.clone(), map.get(id).cloned())).collect()
    };

    let total = resolved.len() as u32;

    let report = tauri::async_runtime::spawn_blocking(move || -> DeleteReport {
        use crate::delete_engine::{self, DeleteOutcome};
        use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
        use std::sync::Mutex;

        let _ = on_event.send(DeleteEvent::Started { total });

        // Shared counters for parallel workers.
        let deleted_count = AtomicU32::new(0);
        let reclaimed_total = AtomicU64::new(0);
        let skipped_paths: Mutex<Vec<String>> = Mutex::new(Vec::new());

        // Separate items into "valid" (pass guardrails) and "rejected" (skip immediately).
        let mut valid_items: Vec<(String, CleanupItem)> = Vec::new();

        for (id, maybe_item) in resolved {
            let item = match maybe_item {
                Some(it) => it,
                None => {
                    let _ = on_event.send(DeleteEvent::Skipped {
                        id: id.clone(),
                        path: String::new(),
                        reason: "item not found in last scan".to_string(),
                    });
                    skipped_paths.lock().unwrap().push(id);
                    continue;
                }
            };

            let path = PathBuf::from(&item.path);

            // Guardrail: never delete outside an allowed root.
            if !is_within_allowed(&path, &roots) {
                let _ = on_event.send(DeleteEvent::Skipped {
                    id: item.id.clone(),
                    path: item.path.clone(),
                    reason: "outside allowed roots".to_string(),
                });
                skipped_paths.lock().unwrap().push(item.path.clone());
                continue;
            }

            valid_items.push((id, item));
        }

        // Process valid items in parallel using a dedicated rayon pool.
        // Cap at 4 threads to avoid saturating I/O (especially on HDD).
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4.min(valid_items.len().max(1)))
            .build()
            .unwrap_or_else(|_| {
                // Fallback: use global pool if custom pool fails.
                rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap()
            });

        pool.install(|| {
            use rayon::prelude::*;

            valid_items.par_iter().for_each(|(_id, item)| {
                let path = PathBuf::from(&item.path);

                // Preflight: check if the path is accessible.
                // If already gone, count as success (idempotent deletion).
                match delete_engine::preflight_check(&path) {
                    Err(reason) if reason == "already deleted" => {
                        // The item is already gone — report success.
                        deleted_count.fetch_add(1, Ordering::Relaxed);
                        reclaimed_total.fetch_add(item.size_bytes, Ordering::Relaxed);
                        let _ = on_event.send(DeleteEvent::Deleted {
                            id: item.id.clone(),
                            path: item.path.clone(),
                            size_bytes: item.size_bytes,
                        });
                        return;
                    }
                    Err(reason) => {
                        // Can't even access it — skip with a helpful message.
                        let _ = on_event.send(DeleteEvent::Skipped {
                            id: item.id.clone(),
                            path: item.path.clone(),
                            reason,
                        });
                        skipped_paths.lock().unwrap().push(item.path.clone());
                        return;
                    }
                    Ok(()) => {} // Proceed with deletion.
                }

                // Attempt deletion with timeout protection.
                let outcome = delete_engine::delete_with_timeout(&path, to_recycle_bin);

                match outcome {
                    DeleteOutcome::Success => {
                        deleted_count.fetch_add(1, Ordering::Relaxed);
                        reclaimed_total.fetch_add(item.size_bytes, Ordering::Relaxed);
                        let _ = on_event.send(DeleteEvent::Deleted {
                            id: item.id.clone(),
                            path: item.path.clone(),
                            size_bytes: item.size_bytes,
                        });
                    }
                    DeleteOutcome::Error(msg) => {
                        let _ = on_event.send(DeleteEvent::Skipped {
                            id: item.id.clone(),
                            path: item.path.clone(),
                            reason: msg,
                        });
                        skipped_paths.lock().unwrap().push(item.path.clone());
                    }
                    DeleteOutcome::Timeout => {
                        let _ = on_event.send(DeleteEvent::Skipped {
                            id: item.id.clone(),
                            path: item.path.clone(),
                            reason: "timed out after 30s (file may be locked by another program)".to_string(),
                        });
                        skipped_paths.lock().unwrap().push(item.path.clone());
                    }
                }
            });
        });

        let deleted = deleted_count.load(Ordering::Relaxed);
        let reclaimed_bytes = reclaimed_total.load(Ordering::Relaxed);
        let skipped = skipped_paths.into_inner().unwrap();

        let _ = on_event.send(DeleteEvent::Finished {
            deleted,
            reclaimed_bytes,
            skipped: skipped.len() as u32,
        });

        DeleteReport {
            deleted,
            reclaimed_bytes,
            skipped,
        }
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
// detect_tools
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

// -------------------------------------------------------------------------
// default_roots
// -------------------------------------------------------------------------

/// Suggested scan roots as forward-slash display strings.
#[tauri::command]
pub fn default_roots(_app: AppHandle) -> Vec<String> {
    safai_rules::default_roots()
        .iter()
        .map(|p| normalize_slashes(&p.to_string_lossy()))
        .collect()
}

// -------------------------------------------------------------------------
// drive_info
// -------------------------------------------------------------------------

/// Free/total bytes for the drive containing `path` (header gauge).
///
/// On Windows this calls `GetDiskFreeSpaceExW` through a tiny FFI declaration
/// (see [`win_disk`]) so we don't pull in an extra crate. On other platforms
/// it returns zeros (Safai is Windows-first; a cross-platform impl is a later
/// change per §7).
#[tauri::command]
pub fn drive_info(path: String) -> Result<DriveInfo> {
    let p = PathBuf::from(&path);
    let mount = drive_mount(&p);

    #[cfg(windows)]
    {
        match win_disk::disk_free_total(&p) {
            Some((free_bytes, total_bytes)) => Ok(DriveInfo {
                mount,
                free_bytes,
                total_bytes,
            }),
            None => Err(SafaiError::Other(format!(
                "failed to query disk space for {path}"
            ))),
        }
    }

    #[cfg(not(windows))]
    {
        // Non-Windows fallback: report zeros (see doc comment).
        Ok(DriveInfo {
            mount,
            free_bytes: 0,
            total_bytes: 0,
        })
    }
}

/// Minimal Windows FFI for querying free/total disk space without an extra
/// crate. Isolated here so the single `unsafe` block stays small and audited.
#[cfg(windows)]
mod win_disk {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    #[link(name = "kernel32")]
    extern "system" {
        // https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available_to_caller: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    /// Returns `(free_bytes_available_to_caller, total_bytes)` for the volume
    /// containing `path`, or `None` if the query fails.
    pub fn disk_free_total(path: &Path) -> Option<(u64, u64)> {
        // Build a null-terminated UTF-16 path for the wide API.
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);

        let mut free_to_caller: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free: u64 = 0;

        // SAFETY: `wide` is a valid, null-terminated UTF-16 buffer that lives
        // for the duration of the call. The three out-pointers reference stack
        // locals that outlive the call. `GetDiskFreeSpaceExW` only reads the
        // path and writes the three u64 out-params; it has no other effects.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_to_caller,
                &mut total_bytes,
                &mut total_free,
            )
        };

        if ok != 0 {
            Some((free_to_caller, total_bytes))
        } else {
            None
        }
    }
}
