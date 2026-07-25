//! Shared scan/delete execution cores.
//!
//! Both the interactive Tauri commands (`commands.rs`) and the background
//! scheduler (`schedule/runner.rs`) drive cleanup through this module, so an
//! automatic run is byte-for-byte the same code path as a run the user starts
//! by hand — including every guardrail. The only difference between the two is
//! *who* receives the progress events and whether the process is in background
//! I/O priority mode.
//!
//! Progress is delivered through a `&(dyn Fn(Event) + Send + Sync)` sink rather
//! than a `Channel` so callers can plug in either the IPC channel (interactive)
//! or an in-process aggregator (scheduler).

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use safai_rules::{run_scan, CleanupItem, ScanConfig, ScanEvent, ScanReport};

use crate::delete_engine::{self, DeleteOutcome};
use crate::dto::{DeleteEvent, DeleteReport};

/// Maximum number of items deleted concurrently. Capped low on purpose: past
/// ~4 in-flight deletions the disk is the bottleneck and extra threads only
/// add seek pressure (badly so on spinning disks).
const DELETE_CONCURRENCY: usize = 4;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Normalize a path string for the UI: forward slashes only.
pub fn normalize_slashes(s: &str) -> String {
    s.replace('\\', "/")
}

/// Guardrail: is `path` inside one of the allow-listed roots?
///
/// Canonicalizes both sides (resolving symlinks and `..`) so a path can't
/// escape an allowed root via a link or relative segment. A path that fails to
/// canonicalize (e.g. already gone) is treated as **not** allowed.
pub fn is_within_allowed(path: &Path, roots: &[PathBuf]) -> bool {
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
pub fn drive_mount(path: &Path) -> String {
    for comp in path.components() {
        if let Component::Prefix(prefix) = comp {
            return normalize_slashes(&prefix.as_os_str().to_string_lossy());
        }
    }
    normalize_slashes(&path.to_string_lossy())
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

/// Build a [`ScanConfig`] from display roots (empty = the crate's defaults).
///
/// Pattern rules hunt for project artifacts (`node_modules`, `target`, …),
/// which live in dev folders — not across the whole profile + AppData — so the
/// project roots come from the rules crate's bounded, targeted list.
pub fn build_scan_config(roots: &[String], discover_large_folders: bool) -> ScanConfig {
    let resolved: Vec<PathBuf> = if roots.is_empty() {
        safai_rules::default_roots()
    } else {
        roots.iter().map(PathBuf::from).collect()
    };

    ScanConfig {
        roots: resolved,
        project_scan_roots: safai_rules::project_scan_roots(),
        discover_large_folders,
    }
}

/// Run a scan to completion on the calling (blocking) thread.
pub fn scan_blocking(
    cfg: &ScanConfig,
    cancel: &AtomicBool,
    sink: &(dyn Fn(ScanEvent) + Send + Sync),
) -> ScanReport {
    let mut forward = |ev: ScanEvent| sink(ev);
    run_scan(cfg, cancel, &mut forward)
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

/// Delete `items` on the calling (blocking) thread, streaming `DeleteEvent`s
/// into `sink` and returning the aggregate [`DeleteReport`].
///
/// Every item is re-checked against `roots` here, so this is the single
/// chokepoint for the allow-list guardrail regardless of how the caller
/// assembled its list. Items are processed in a bounded parallel pool with a
/// per-item timeout so one locked file can never stall the run.
///
/// `resolved` pairs each requested id with the item it resolved to (`None` for
/// ids that aren't in the last scan), matching what the command layer can
/// produce from its id map.
pub fn delete_blocking(
    resolved: Vec<(String, Option<CleanupItem>)>,
    roots: &[PathBuf],
    to_recycle_bin: bool,
    cancel: &AtomicBool,
    sink: &(dyn Fn(DeleteEvent) + Send + Sync),
) -> DeleteReport {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU32, AtomicU64};
    use std::sync::Mutex;

    let total = resolved.len() as u32;
    sink(DeleteEvent::Started { total });

    let deleted_count = AtomicU32::new(0);
    let reclaimed_total = AtomicU64::new(0);
    let skipped_paths: Mutex<Vec<String>> = Mutex::new(Vec::new());

    // Split into items that pass the guardrails and ones rejected up front.
    let mut valid_items: Vec<CleanupItem> = Vec::with_capacity(resolved.len());

    for (id, maybe_item) in resolved {
        let item = match maybe_item {
            Some(it) => it,
            None => {
                sink(DeleteEvent::Skipped {
                    id: id.clone(),
                    path: String::new(),
                    reason: "item not found in last scan".to_string(),
                });
                skipped_paths.lock().unwrap().push(id);
                continue;
            }
        };

        let path = PathBuf::from(&item.path);

        // Already gone — count it as done.
        //
        // This has to be checked *before* the allow-list, because
        // `is_within_allowed` canonicalizes and so cannot prove anything about
        // a path that no longer exists. Checking the guardrail first would
        // report every vanished item as "outside allowed roots", which is both
        // alarming and wrong: items disappear between scan and delete all the
        // time (temp files, caches the owning tool cleared itself).
        //
        // Ordering them this way is safe because there is nothing here to
        // delete: this branch reports and moves on without touching the disk,
        // so it cannot be used to reach outside a cleanup root.
        if !path.exists() {
            deleted_count.fetch_add(1, Ordering::Relaxed);
            reclaimed_total.fetch_add(item.size_bytes, Ordering::Relaxed);
            sink(DeleteEvent::Deleted {
                id: item.id.clone(),
                path: item.path.clone(),
                size_bytes: item.size_bytes,
            });
            continue;
        }

        // Guardrail: never delete outside an allowed root.
        if !is_within_allowed(&path, roots) {
            sink(DeleteEvent::Skipped {
                id: item.id.clone(),
                path: item.path.clone(),
                reason: "outside allowed roots".to_string(),
            });
            skipped_paths.lock().unwrap().push(item.path.clone());
            continue;
        }

        valid_items.push(item);
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(DELETE_CONCURRENCY.min(valid_items.len().max(1)))
        .build()
        .unwrap_or_else(|_| {
            // Fall back to a single-threaded pool rather than the global one,
            // so we never inherit a saturated rayon pool from the scan.
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build()
                .expect("single-threaded rayon pool")
        });

    pool.install(|| {
        valid_items.par_iter().for_each(|item| {
            // Cancellation (Stop automation / cancel_scan) short-circuits the
            // remaining items; already-deleted ones stay deleted.
            if cancel.load(Ordering::Relaxed) {
                return;
            }

            let path = PathBuf::from(&item.path);

            // Preflight. The "already deleted" arm is the race fallback for an
            // item that existed during the partition pass above but vanished
            // before this worker reached it.
            match delete_engine::preflight_check(&path) {
                Err(reason) if reason == "already deleted" => {
                    deleted_count.fetch_add(1, Ordering::Relaxed);
                    reclaimed_total.fetch_add(item.size_bytes, Ordering::Relaxed);
                    sink(DeleteEvent::Deleted {
                        id: item.id.clone(),
                        path: item.path.clone(),
                        size_bytes: item.size_bytes,
                    });
                    return;
                }
                Err(reason) => {
                    sink(DeleteEvent::Skipped {
                        id: item.id.clone(),
                        path: item.path.clone(),
                        reason,
                    });
                    skipped_paths.lock().unwrap().push(item.path.clone());
                    return;
                }
                Ok(()) => {}
            }

            match delete_engine::delete_with_timeout(&path, to_recycle_bin) {
                DeleteOutcome::Success => {
                    deleted_count.fetch_add(1, Ordering::Relaxed);
                    reclaimed_total.fetch_add(item.size_bytes, Ordering::Relaxed);
                    sink(DeleteEvent::Deleted {
                        id: item.id.clone(),
                        path: item.path.clone(),
                        size_bytes: item.size_bytes,
                    });
                }
                DeleteOutcome::Error(msg) => {
                    sink(DeleteEvent::Skipped {
                        id: item.id.clone(),
                        path: item.path.clone(),
                        reason: msg,
                    });
                    skipped_paths.lock().unwrap().push(item.path.clone());
                }
                DeleteOutcome::Timeout => {
                    sink(DeleteEvent::Skipped {
                        id: item.id.clone(),
                        path: item.path.clone(),
                        reason: "timed out after 30s (file may be locked by another program)"
                            .to_string(),
                    });
                    skipped_paths.lock().unwrap().push(item.path.clone());
                }
            }
        });
    });

    let deleted = deleted_count.load(Ordering::Relaxed);
    let reclaimed_bytes = reclaimed_total.load(Ordering::Relaxed);
    let skipped = skipped_paths.into_inner().unwrap();

    sink(DeleteEvent::Finished {
        deleted,
        reclaimed_bytes,
        skipped: skipped.len() as u32,
    });

    DeleteReport {
        deleted,
        reclaimed_bytes,
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safai_rules::{Category, SafetyTier};
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;

    /// A cleanup item pointing at `path`.
    fn item_at(id: &str, path: &Path, size_bytes: u64) -> CleanupItem {
        CleanupItem {
            id: id.to_string(),
            rule_id: "test-rule".to_string(),
            label: id.to_string(),
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            path: path.to_string_lossy().to_string(),
            size_bytes,
            regenerates: true,
            last_modified_secs: None,
            note: String::new(),
            selected_by_default: true,
        }
    }

    /// Collects the streamed events so assertions can inspect the reasons the
    /// engine reported, not just the totals.
    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<DeleteEvent>>,
    }

    impl Recorder {
        fn sink(&self) -> impl Fn(DeleteEvent) + Send + Sync + '_ {
            move |event| self.events.lock().unwrap().push(event)
        }

        fn skip_reasons(&self) -> Vec<String> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter_map(|e| match e {
                    DeleteEvent::Skipped { reason, .. } => Some(reason.clone()),
                    _ => None,
                })
                .collect()
        }
    }

    // -- is_within_allowed ------------------------------------------------
    //
    // This is *the* deletion guardrail. Every one of these is a case where a
    // regression would let Safai delete something outside a cleanup root.

    #[test]
    fn a_path_inside_an_allowed_root_is_permitted() {
        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("cache");
        std::fs::create_dir(&target).unwrap();

        assert!(is_within_allowed(&target, &[root.path().to_path_buf()]));
    }

    #[test]
    fn the_root_itself_is_permitted() {
        let root = tempfile::tempdir().expect("temp root");
        assert!(is_within_allowed(root.path(), &[root.path().to_path_buf()]));
    }

    #[test]
    fn a_sibling_directory_is_refused() {
        let parent = tempfile::tempdir().expect("temp parent");
        let allowed = parent.path().join("allowed");
        let other = parent.path().join("other");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&other).unwrap();

        assert!(!is_within_allowed(&other, &[allowed]));
    }

    /// Canonicalization has to collapse `..` before the prefix comparison, or a
    /// crafted path could walk out of an allowed root while still *starting*
    /// with it as a string.
    #[test]
    fn a_dot_dot_escape_is_refused() {
        let parent = tempfile::tempdir().expect("temp parent");
        let allowed = parent.path().join("allowed");
        let secret = parent.path().join("secret");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&secret).unwrap();

        let escaped = allowed.join("..").join("secret");
        assert!(
            !is_within_allowed(&escaped, &[allowed]),
            "`..` must not escape the allowed root"
        );
    }

    /// A sibling whose name merely starts with the root's name must not pass a
    /// naive string prefix check (`.../allowed` vs `.../allowed-evil`).
    #[test]
    fn a_sibling_with_a_shared_name_prefix_is_refused() {
        let parent = tempfile::tempdir().expect("temp parent");
        let allowed = parent.path().join("allowed");
        let lookalike = parent.path().join("allowed-evil");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&lookalike).unwrap();

        assert!(!is_within_allowed(&lookalike, &[allowed]));
    }

    #[test]
    fn a_nonexistent_path_is_refused() {
        let root = tempfile::tempdir().expect("temp root");
        let ghost = root.path().join("not-here");
        // Can't be canonicalized, so it can't be proven safe.
        assert!(!is_within_allowed(&ghost, &[root.path().to_path_buf()]));
    }

    #[test]
    fn an_empty_allow_list_permits_nothing() {
        let root = tempfile::tempdir().expect("temp root");
        assert!(!is_within_allowed(root.path(), &[]));
    }

    // -- delete_blocking guardrails --------------------------------------

    #[test]
    fn deleting_removes_an_item_inside_an_allowed_root() {
        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("cache");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("blob.bin"), b"payload").unwrap();

        let recorder = Recorder::default();
        let report = delete_blocking(
            vec![("a".to_string(), Some(item_at("a", &target, 7)))],
            &[root.path().to_path_buf()],
            false, // permanent: the Recycle Bin isn't available in CI
            &AtomicBool::new(false),
            &recorder.sink(),
        );

        assert_eq!(report.deleted, 1);
        assert_eq!(report.reclaimed_bytes, 7);
        assert!(report.skipped.is_empty());
        assert!(!target.exists(), "the directory should be gone");
    }

    /// The guardrail is re-checked inside the engine, so it holds no matter how
    /// the caller assembled its list.
    #[test]
    fn deleting_refuses_a_path_outside_the_allowed_roots() {
        let parent = tempfile::tempdir().expect("temp parent");
        let allowed = parent.path().join("allowed");
        let outside = parent.path().join("outside");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), b"important").unwrap();

        let recorder = Recorder::default();
        let report = delete_blocking(
            vec![("x".to_string(), Some(item_at("x", &outside, 9)))],
            &[allowed],
            false,
            &AtomicBool::new(false),
            &recorder.sink(),
        );

        assert_eq!(report.deleted, 0);
        assert_eq!(report.reclaimed_bytes, 0);
        assert_eq!(report.skipped.len(), 1);
        assert!(outside.exists(), "the path must be untouched");
        assert!(recorder
            .skip_reasons()
            .iter()
            .any(|r| r.contains("outside allowed roots")));
    }

    #[test]
    fn deleting_skips_ids_that_did_not_resolve() {
        let root = tempfile::tempdir().expect("temp root");
        let recorder = Recorder::default();

        let report = delete_blocking(
            vec![("ghost".to_string(), None)],
            &[root.path().to_path_buf()],
            false,
            &AtomicBool::new(false),
            &recorder.sink(),
        );

        assert_eq!(report.deleted, 0);
        assert_eq!(report.skipped, vec!["ghost".to_string()]);
        assert!(recorder
            .skip_reasons()
            .iter()
            .any(|r| r.contains("not found in last scan")));
    }

    /// An already-deleted item counts as success, so a retried or duplicated
    /// request is idempotent instead of reporting a spurious failure.
    #[test]
    fn deleting_treats_an_already_gone_item_as_success() {
        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("cache");
        std::fs::create_dir(&target).unwrap();

        let item = item_at("a", &target, 500);
        std::fs::remove_dir_all(&target).unwrap();

        let recorder = Recorder::default();
        let report = delete_blocking(
            vec![("a".to_string(), Some(item))],
            &[root.path().to_path_buf()],
            false,
            &AtomicBool::new(false),
            &recorder.sink(),
        );

        assert_eq!(report.deleted, 1);
        assert_eq!(report.reclaimed_bytes, 500);
    }

    /// Pins the ordering of the two checks. A vanished item is reported as done
    /// rather than as an allow-list violation, because `is_within_allowed`
    /// cannot canonicalize a path that isn't there — and reporting "outside
    /// allowed roots" for a temp file that cleaned itself up would be wrong.
    /// Safe regardless of the root, since this path touches no disk.
    #[test]
    fn a_vanished_item_is_reported_as_done_not_as_a_guardrail_violation() {
        let parent = tempfile::tempdir().expect("temp parent");
        let allowed = parent.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();

        let gone = parent.path().join("outside").join("already-cleared");
        let recorder = Recorder::default();

        let report = delete_blocking(
            vec![("a".to_string(), Some(item_at("a", &gone, 42)))],
            &[allowed],
            false,
            &AtomicBool::new(false),
            &recorder.sink(),
        );

        assert_eq!(report.deleted, 1);
        assert!(
            !recorder
                .skip_reasons()
                .iter()
                .any(|r| r.contains("outside allowed roots")),
            "a missing path must not be blamed on the allow-list"
        );
    }

    #[test]
    fn a_preset_cancel_flag_deletes_nothing() {
        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("cache");
        std::fs::create_dir(&target).unwrap();

        let recorder = Recorder::default();
        let report = delete_blocking(
            vec![("a".to_string(), Some(item_at("a", &target, 1)))],
            &[root.path().to_path_buf()],
            false,
            &AtomicBool::new(true), // already cancelled
            &recorder.sink(),
        );

        assert_eq!(report.deleted, 0);
        assert!(target.exists(), "a cancelled run must not delete");
    }

    #[test]
    fn a_delete_run_always_brackets_its_events() {
        let root = tempfile::tempdir().expect("temp root");
        let recorder = Recorder::default();

        delete_blocking(
            Vec::new(),
            &[root.path().to_path_buf()],
            false,
            &AtomicBool::new(false),
            &recorder.sink(),
        );

        let events = recorder.events.lock().unwrap();
        assert!(matches!(
            events.first(),
            Some(DeleteEvent::Started { total: 0 })
        ));
        assert!(matches!(events.last(), Some(DeleteEvent::Finished { .. })));
    }

    // -- build_scan_config ------------------------------------------------

    #[test]
    fn an_empty_root_list_falls_back_to_the_defaults() {
        let cfg = build_scan_config(&[], true);
        assert_eq!(cfg.roots, safai_rules::default_roots());
        assert!(cfg.discover_large_folders);
    }

    #[test]
    fn explicit_roots_are_used_verbatim() {
        let cfg = build_scan_config(&["C:/tmp/one".to_string()], false);
        assert_eq!(cfg.roots, vec![PathBuf::from("C:/tmp/one")]);
        assert!(!cfg.discover_large_folders);
    }

    // -- path helpers -----------------------------------------------------

    #[test]
    fn slashes_are_normalized_for_display() {
        assert_eq!(
            normalize_slashes(r"C:\Users\a\AppData"),
            "C:/Users/a/AppData"
        );
    }

    #[test]
    fn the_drive_prefix_is_extracted_as_the_mount() {
        assert_eq!(drive_mount(Path::new(r"C:\Users\a")), "C:");
    }
}
