//! The scan driver (implementation-plan.md §5 WS2).
//!
//! [`run_scan`] walks the rule table, measures matching locations via the
//! `safai-core` `measure` API, streams [`ScanEvent`]s through a plain callback,
//! and aggregates the findings into a [`ScanReport`]. The callback shape keeps
//! this crate free of any Tauri dependency — WS3 adapts it to a `Channel`.

use std::collections::HashSet;
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use crate::detect::{detect_tools, display_path, expand};
use crate::model::{
    Category, CategoryGroup, CleanupItem, ScanEvent, ScanReport, SafetyTier,
};
use crate::rules::{all_rules, CleanupRule};

/// Configuration for a scan run.
pub struct ScanConfig {
    /// Roots reported to the UI (typically the user profile + cache parents).
    /// Also swept (one level deep) for the generic large-folder discovery.
    pub roots: Vec<PathBuf>,
    /// Where to hunt for pattern targets (`node_modules`, build dirs) via the
    /// pruned walk. The caller chooses these (e.g. common dev folders) so this
    /// crate stays policy-free.
    pub project_scan_roots: Vec<PathBuf>,
    /// When true, run the generic "large folders" discovery pass: surface big
    /// directories under `roots` that no specific rule matched, so any user's
    /// software shows up (Caution tier, never pre-selected).
    pub discover_large_folders: bool,
}

/// Minimum size for a folder to be surfaced by the generic discovery pass.
const MIN_LARGE_FOLDER_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB
/// Cap on how many generic large folders to report (largest first).
const MAX_LARGE_FOLDERS: usize = 20;

/// Stable category ordering for deterministic group output.
const CATEGORY_ORDER: [Category; 7] = [
    Category::PackageCache,
    Category::EditorStorage,
    Category::BuildArtifact,
    Category::Temp,
    Category::Model,
    Category::Browser,
    Category::Other,
];

/// Human-readable label for a category group header.
pub fn label_for(category: Category) -> &'static str {
    match category {
        Category::PackageCache => "Package caches",
        Category::EditorStorage => "Editor storage",
        Category::BuildArtifact => "Build artifacts",
        Category::Temp => "Temporary files",
        Category::Model => "Downloaded models",
        Category::Browser => "Browser binaries",
        Category::Other => "Other large items",
    }
}

/// Is `candidate` (normalized, lowercased) equal to, an ancestor of, or a
/// descendant of any already-found path? Used by the generic discovery pass to
/// avoid double-counting space already attributed to a specific rule.
fn path_overlaps(candidate: &str, existing_lower: &[String]) -> bool {
    existing_lower.iter().any(|p| {
        candidate == p
            || candidate.starts_with(&format!("{p}/"))
            || p.starts_with(&format!("{candidate}/"))
    })
}

/// Well-known user/system folder names that the generic discovery skips: they
/// are large but are user *data* (not reclaimable software space) and/or are
/// slow to size. Skipping them keeps discovery fast and its results relevant.
/// `appdata` is skipped here because we already sweep `Local`/`Roaming`
/// directly as roots — re-sizing the whole `AppData` tree would be huge and
/// redundant.
const DISCOVERY_SKIP_NAMES: &[&str] = &[
    "documents",
    "pictures",
    "videos",
    "music",
    "desktop",
    "downloads",
    "favorites",
    "contacts",
    "links",
    "searches",
    "saved games",
    "3d objects",
    "appdata",
    "onedrive",
    "onedrive - personal",
    "dropbox",
    "google drive",
];

/// Cheaply enumerate the *direct child* directories of each root that are
/// candidates for the generic large-folder pass. This does **no** sizing (just
/// `read_dir` + a name denylist), so the caller can learn the candidate count
/// up front and drive an accurate, smoothly-advancing progress bar — then size
/// each candidate one at a time, streaming a `Progress` event per folder.
fn enumerate_large_folder_candidates(parents: &[PathBuf], cancel: &AtomicBool) -> Vec<PathBuf> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    for parent in parents {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let rd = match std::fs::read_dir(parent) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let ft = match entry.file_type() {
                Ok(f) => f,
                Err(_) => continue,
            };
            if !ft.is_dir() {
                continue;
            }
            let path = entry.path();
            let name_low = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            if DISCOVERY_SKIP_NAMES.contains(&name_low.as_str()) {
                continue;
            }
            let low = display_path(&path).trim_end_matches('/').to_lowercase();
            if seen.insert(low) {
                out.push(path);
            }
        }
    }
    out
}

/// 64-bit FNV-1a hash — chosen over `DefaultHasher` because FNV is a fixed,
/// specified algorithm, so an item's `id` is stable across runs and builds
/// (the same path always yields the same id). Rendered as 16 hex chars.
fn fnv1a_hex(input: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

/// Stable, unique id for a finding: a short hash of its (display) path.
pub fn stable_id(path: &str) -> String {
    fnv1a_hex(path)
}

/// Unix seconds of a path's last-modified time, if available (for staleness).
fn last_modified_secs(path: &Path) -> Option<u64> {
    let md = std::fs::metadata(path).ok()?;
    let modified = md.modified().ok()?;
    modified.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// How often the streaming sizing loop republishes progress to the UI.
const SIZE_PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

/// Safety net: if sizing a single directory exceeds this budget, stop walking
/// it and report the partial size (with a warning). This guarantees the scan
/// can never hang indefinitely on a pathological tree (a giant model cache on a
/// slow disk, a network mount, a reparse-point maze, etc.). Sized generously so
/// it rarely triggers on normal local caches — the live progress below is the
/// primary "it's not stuck" signal.
const SIZE_TIME_BUDGET: Duration = Duration::from_secs(90);

/// Measure a file's length. Directories are handled by [`measure_dir_streaming`].
fn measure_file(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Measure a directory while streaming a live `Progress` event as the running
/// byte total climbs — so the UI never appears frozen on a huge directory
/// (e.g. the Hugging Face model cache), and the scan is bounded by a time
/// budget so it can never hang forever.
///
/// The heavy walk runs on a scoped worker thread that updates a shared atomic
/// counter (`safai_core::dir_size_into`); meanwhile *this* thread polls that
/// counter and forwards `Progress` events through `on_event` (which stays
/// single-threaded, so it needs no `Sync` bound). Global `cancel` and the time
/// budget are both propagated to the walk via a local cancel flag.
#[allow(clippy::too_many_arguments)]
fn measure_dir_streaming(
    path: &Path,
    cancel: &AtomicBool,
    label: &str,
    base_found: u64,
    rules_checked: u32,
    rules_total: u32,
    on_event: &mut dyn FnMut(ScanEvent),
    warnings: &mut Vec<String>,
) -> u64 {
    let progress = AtomicU64::new(0);
    let local_cancel = AtomicBool::new(false);
    let start = Instant::now();
    let mut size = 0u64;

    std::thread::scope(|s| {
        let progress_ref = &progress;
        let local_ref = &local_cancel;
        let handle = s.spawn(move || safai_core::dir_size_into(path, local_ref, progress_ref));

        // Poll on this (the caller's) thread while the walk runs.
        while !handle.is_finished() {
            std::thread::sleep(SIZE_PROGRESS_INTERVAL);

            // Propagate global cancellation or a blown time budget to the walk.
            if cancel.load(Ordering::Relaxed) || start.elapsed() >= SIZE_TIME_BUDGET {
                local_cancel.store(true, Ordering::Relaxed);
            }

            let cur = progress_ref.load(Ordering::Relaxed);
            on_event(ScanEvent::Progress {
                current_path: label.to_string(),
                found_bytes: base_found.saturating_add(cur),
                rules_checked,
                rules_total,
            });
        }

        size = handle.join().unwrap_or(0);
    });

    // If we stopped because of the budget (not a user cancel), be honest about
    // the partial number.
    if start.elapsed() >= SIZE_TIME_BUDGET && !cancel.load(Ordering::Relaxed) {
        warnings.push(format!(
            "Sizing '{label}' stopped after {}s; its size may be underestimated.",
            SIZE_TIME_BUDGET.as_secs()
        ));
    }

    size
}

/// Run the rules-based scan.
///
/// Emits [`ScanEvent`]s through `on_event` and returns the aggregated
/// [`ScanReport`]. Honors `cancel` throughout: when set, the scan stops adding
/// new findings and finishes with whatever it has gathered so far.
pub fn run_scan(
    cfg: &ScanConfig,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> ScanReport {
    // ------------------------------------------------------------------
    // Scanned roots (deduped) + Started event.
    // ------------------------------------------------------------------
    let mut scanned_roots: Vec<String> = Vec::new();
    for r in cfg.roots.iter().chain(cfg.project_scan_roots.iter()) {
        let d = display_path(r);
        if !scanned_roots.contains(&d) {
            scanned_roots.push(d);
        }
    }
    on_event(ScanEvent::Started {
        roots: scanned_roots.clone(),
    });

    let rules = all_rules();

    // Detected tool ids, for `requires_tool` gating.
    let detected: Vec<String> = detect_tools()
        .into_iter()
        .filter(|(_, _, ok)| *ok)
        .map(|(id, _, _)| id)
        .collect();
    let tool_detected = |id: &str| detected.iter().any(|d| d == id);

    // Partition rules; count only rules that will actually be processed so the
    // progress denominator is meaningful.
    let mut fixed_rules: Vec<&CleanupRule> = Vec::new();
    let mut pattern_rules: Vec<&CleanupRule> = Vec::new();
    for rule in &rules {
        if rule.pattern.is_some() {
            pattern_rules.push(rule);
        } else {
            // Gate fixed rules on their required tool (if any).
            let gated_out = matches!(rule.requires_tool, Some(tool) if !tool_detected(tool));
            if !gated_out {
                fixed_rules.push(rule);
            }
        }
    }
    // Enumerate generic large-folder candidates up front (cheap `read_dir`, no
    // sizing) so `rules_total` is accurate and the bar advances one notch per
    // folder as we measure them — no long silent stall at the end.
    let candidates: Vec<PathBuf> = if cfg.discover_large_folders {
        enumerate_large_folder_candidates(&cfg.roots, cancel)
    } else {
        Vec::new()
    };
    let rules_total: u32 =
        (fixed_rules.len() + pattern_rules.len() + candidates.len()) as u32;

    let mut items: Vec<CleanupItem> = Vec::new();
    let mut found_bytes: u64 = 0;
    let mut rules_checked: u32 = 0;
    let mut warnings: Vec<String> = Vec::new();

    // ------------------------------------------------------------------
    // Fixed-location rules.
    // ------------------------------------------------------------------
    for rule in &fixed_rules {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        for spec in &rule.locations {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if let Some(path) = expand(spec.raw) {
                let is_dir = path.is_dir();
                let size = if is_dir {
                    measure_dir_streaming(
                        &path,
                        cancel,
                        rule.label,
                        found_bytes,
                        rules_checked,
                        rules_total,
                        on_event,
                        &mut warnings,
                    )
                } else {
                    measure_file(&path)
                };
                let disp = display_path(&path);
                let item = CleanupItem {
                    id: stable_id(&disp),
                    rule_id: rule.id.to_string(),
                    label: rule.label.to_string(),
                    category: rule.category,
                    tier: rule.tier,
                    path: disp.clone(),
                    size_bytes: size,
                    regenerates: rule.regenerates,
                    last_modified_secs: last_modified_secs(&path),
                    note: rule.note.to_string(),
                    selected_by_default: rule.tier == SafetyTier::Safe,
                };
                found_bytes = found_bytes.saturating_add(size);
                on_event(ScanEvent::Found { item: item.clone() });
                items.push(item);
            }
        }

        rules_checked += 1;
        on_event(ScanEvent::Progress {
            current_path: rule.label.to_string(),
            found_bytes,
            rules_checked,
            rules_total,
        });
    }

    // ------------------------------------------------------------------
    // Pattern rules (single pruned walk over the project scan roots).
    // ------------------------------------------------------------------
    if !pattern_rules.is_empty() && !cfg.project_scan_roots.is_empty() {
        // Collect matched directories during the walk; measure afterward so we
        // never call the (potentially heavy) sizing inside the walk callback.
        let mut matches: Vec<PathBuf> = Vec::new();

        {
            // The prune predicate returns true for any directory whose name is
            // one of the pattern names — we report it but never descend.
            let is_target_name = |name: &str| -> bool {
                pattern_rules.iter().any(|r| {
                    r.pattern
                        .as_ref()
                        .map(|p| p.names.iter().any(|n| *n == name))
                        .unwrap_or(false)
                })
            };

            let prune = |path: &Path, ft: &FileType| -> bool {
                if !ft.is_dir() {
                    return false;
                }
                match path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => is_target_name(name),
                    None => false,
                }
            };

            let mut on_dir = |path: &Path| {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_target_name(name) {
                        matches.push(path.to_path_buf());
                    }
                }
            };

            safai_core::walk_pruned(&cfg.project_scan_roots, cancel, &prune, &mut on_dir);
        }

        for matched in matches {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            // Resolve which pattern rule owns this directory name.
            let name = match matched.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            let rule = pattern_rules.iter().find(|r| {
                r.pattern
                    .as_ref()
                    .map(|p| p.names.iter().any(|n| *n == name))
                    .unwrap_or(false)
            });
            let rule = match rule {
                Some(r) => r,
                None => continue,
            };

            let disp = display_path(&matched);
            let size = measure_dir_streaming(
                &matched,
                cancel,
                &disp,
                found_bytes,
                rules_checked,
                rules_total,
                on_event,
                &mut warnings,
            );
            let item = CleanupItem {
                id: stable_id(&disp),
                rule_id: rule.id.to_string(),
                label: rule.label.to_string(),
                category: rule.category,
                tier: rule.tier,
                path: disp.clone(),
                size_bytes: size,
                regenerates: rule.regenerates,
                last_modified_secs: last_modified_secs(&matched),
                note: rule.note.to_string(),
                // Pattern (build artifact) targets are Review-tier and never
                // pre-selected.
                selected_by_default: rule.tier == SafetyTier::Safe,
            };
            found_bytes = found_bytes.saturating_add(size);
            on_event(ScanEvent::Found { item: item.clone() });
            items.push(item);
        }

        rules_checked += pattern_rules.len() as u32;
        on_event(ScanEvent::Progress {
            current_path: "Build artifacts".to_string(),
            found_bytes,
            rules_checked,
            rules_total,
        });
    }

    // ------------------------------------------------------------------
    // Generic large-folder discovery (software-agnostic; Caution tier).
    // Streams a Progress event *before* sizing each candidate so the UI shows
    // exactly which folder is being measured and the bar advances per folder.
    // ------------------------------------------------------------------
    if !candidates.is_empty() {
        // Paths already attributed to a specific rule — don't re-size or
        // double-count them (e.g. a cache folder inside a swept parent).
        let existing_lower: Vec<String> = items
            .iter()
            .map(|it| it.path.trim_end_matches('/').to_lowercase())
            .collect();
        let mut emitted = 0usize;

        for cand in &candidates {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let disp = display_path(cand);

            // Advance the bar and show the folder BEFORE the (slow) sizing, so
            // the user always sees what Safai is currently working on.
            rules_checked += 1;
            on_event(ScanEvent::Progress {
                current_path: disp.clone(),
                found_bytes,
                rules_checked,
                rules_total,
            });

            let low = disp.trim_end_matches('/').to_lowercase();
            if path_overlaps(&low, &existing_lower) {
                continue;
            }
            // Cap how many large folders we surface (keeps the list focused);
            // we still advance progress through the rest.
            if emitted >= MAX_LARGE_FOLDERS {
                continue;
            }

            let size = measure_dir_streaming(
                cand,
                cancel,
                &disp,
                found_bytes,
                rules_checked,
                rules_total,
                on_event,
                &mut warnings,
            );
            if size < MIN_LARGE_FOLDER_BYTES {
                continue;
            }

            let name = cand.file_name().and_then(|n| n.to_str()).unwrap_or("folder");
            let item = CleanupItem {
                id: stable_id(&disp),
                rule_id: "large-folder".to_string(),
                label: name.to_string(),
                category: Category::Other,
                tier: SafetyTier::Caution,
                path: disp.clone(),
                size_bytes: size,
                regenerates: false,
                last_modified_secs: last_modified_secs(cand),
                note: "A large folder that no specific rule matched. It may hold \
                       important data — use Reveal to inspect it before removing. \
                       Never pre-selected."
                    .to_string(),
                selected_by_default: false,
            };
            found_bytes = found_bytes.saturating_add(size);
            emitted += 1;
            on_event(ScanEvent::Found { item: item.clone() });
            items.push(item);
        }
    }

    if cancel.load(Ordering::Relaxed) {
        warnings.push("Scan was cancelled; results may be incomplete.".to_string());
    }

    // ------------------------------------------------------------------
    // Aggregate into category groups (stable order) + totals.
    // ------------------------------------------------------------------
    let total_reclaimable_bytes: u64 = items
        .iter()
        .fold(0u64, |acc, it| acc.saturating_add(it.size_bytes));

    let mut groups: Vec<CategoryGroup> = Vec::new();
    for category in CATEGORY_ORDER {
        let group_items: Vec<CleanupItem> = items
            .iter()
            .filter(|it| it.category == category)
            .cloned()
            .collect();
        if group_items.is_empty() {
            continue;
        }
        let total_bytes = group_items
            .iter()
            .fold(0u64, |acc, it| acc.saturating_add(it.size_bytes));
        groups.push(CategoryGroup {
            category,
            label: label_for(category).to_string(),
            total_bytes,
            items: group_items,
        });
    }

    on_event(ScanEvent::Finished {
        total_reclaimable_bytes,
        item_count: items.len() as u32,
    });

    ScanReport {
        total_reclaimable_bytes,
        groups,
        scanned_roots,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn stable_id_is_deterministic() {
        let a = stable_id("C:/Users/x/node_modules");
        let b = stable_id("C:/Users/x/node_modules");
        let c = stable_id("C:/Users/x/other");
        assert_eq!(a, b, "same path must yield same id");
        assert_ne!(a, c, "different paths should differ");
        assert_eq!(a.len(), 16, "id is 16 hex chars");
    }

    #[test]
    fn aggregates_node_modules_from_tempdir() {
        // Build: root/proj/node_modules/pkg/a.bin (30 bytes)
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        let nm = root.join("proj").join("node_modules");
        let pkg = nm.join("pkg");
        fs::create_dir_all(&pkg).expect("create tree");
        let mut f = File::create(pkg.join("a.bin")).expect("file");
        f.write_all(&[0u8; 30]).expect("write");
        drop(f); // Flush and close before scanning.

        let cfg = ScanConfig {
            roots: vec![root.clone()],
            project_scan_roots: vec![root.clone()],
            discover_large_folders: false,
        };
        let cancel = AtomicBool::new(false);

        let mut found_count = 0usize;
        let report = run_scan(&cfg, &cancel, &mut |ev| {
            if let ScanEvent::Found { item } = ev {
                if item.rule_id == "node-modules" {
                    found_count += 1;
                }
            }
        });

        // Exactly one node_modules Found item, sized 30 bytes.
        assert_eq!(found_count, 1, "should find the single node_modules");
        let nm_items: Vec<&CleanupItem> = report
            .groups
            .iter()
            .flat_map(|g| g.items.iter())
            .filter(|it| it.rule_id == "node-modules")
            .collect();
        assert_eq!(nm_items.len(), 1);
        assert_eq!(nm_items[0].size_bytes, 30);
        assert_eq!(nm_items[0].category, Category::BuildArtifact);
        assert_eq!(nm_items[0].tier, SafetyTier::Review);
        assert!(!nm_items[0].selected_by_default);
        // The build-artifact bytes contribute to the reclaimable total.
        assert!(report.total_reclaimable_bytes >= 30);
    }

    #[test]
    fn cancelled_scan_records_warning() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = ScanConfig {
            roots: vec![tmp.path().to_path_buf()],
            project_scan_roots: vec![tmp.path().to_path_buf()],
            discover_large_folders: false,
        };
        let cancel = AtomicBool::new(true); // pre-cancelled
        let report = run_scan(&cfg, &cancel, &mut |_ev| {});
        assert!(!report.warnings.is_empty(), "cancellation should warn");
    }
}
